//! Bounded observations from the existing analyzer; export completion is not
//! complete call resolution or a new callgraph analysis.
use super::FnCallMap;
use crate::cli::CallgraphArgs;
use rustc_hir::def_id::LOCAL_CRATE;
use rustc_middle::ty::TyCtxt;
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashSet},
    io::{self, Write},
};

const SCHEMA: &str = "rapx.callgraph/v1";
// Cargo consumes bare JSON objects on rustc stderr as its diagnostic protocol.
// A fixed frame preserves each JSON payload alongside ordinary diagnostics.
const FRAME: &[u8] = b"RAPX_CALLGRAPH_JSONL ";

#[derive(Serialize)]
struct Header<'a> {
    schema: &'static str,
    kind: &'static str,
    scope: &'static str,
    cargo_package: Option<&'a str>,
    compiler_crate: &'a str,
    requested_package: Option<&'a str>,
    max_edges: u32,
    max_bytes: u32,
    limitations: [&'static str; 3],
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    caller: String,
    callee: String,
    caller_id: String,
    callee_id: String,
}

#[derive(Serialize)]
struct EdgeRecord<'a> {
    schema: &'static str,
    kind: &'static str,
    #[serde(flatten)]
    edge: &'a Edge,
}

#[derive(Serialize)]
struct Footer {
    schema: &'static str,
    kind: &'static str,
    export_complete: bool,
    coverage: &'static str,
    total_observed_records: usize,
    total_unique_edges: usize,
    duplicate_records: usize,
    emitted_edges: usize,
    omitted_edges: usize,
}

pub(crate) fn emit(
    calls: &FnCallMap,
    tcx: TyCtxt<'_>,
    requested_package: Option<&str>,
    args: &CallgraphArgs,
) -> io::Result<()> {
    let total_records = calls.values().map(Vec::len).sum();
    // Deduplicate each caller using compiler identities. Display paths alone
    // can collide across dependency crate versions.
    let edges = calls.iter().flat_map(|(caller, callees)| {
        let caller_name = tcx.def_path_str(*caller);
        let caller_id = format!("{caller:?}");
        let unique: HashSet<_> = callees.iter().copied().collect();
        unique.into_iter().map(move |callee| Edge {
            caller: caller_name.clone(),
            callee: tcx.def_path_str(callee),
            caller_id: caller_id.clone(),
            callee_id: format!("{callee:?}"),
        })
    });
    let package = std::env::var("CARGO_PKG_NAME").ok();
    let compiler_crate = tcx.crate_name(LOCAL_CRATE).to_string();
    let header = Header {
        schema: SCHEMA,
        kind: "header",
        scope: "current_compiler_crate",
        cargo_package: package.as_deref(),
        compiler_crate: &compiler_crate,
        requested_package,
        max_edges: args.max_edges,
        max_bytes: args.max_bytes,
        limitations: [
            "Counts describe the existing analyzer's observed relation records, not all program calls or callsites. Callers without observed relations are not exported.",
            "Unresolved function-pointer calls can be skipped and virtual dispatch can contain implementation candidates; untruncated export does not prove complete call resolution or runtime dispatch.",
            "Endpoint IDs are compiler-local DefId observations. Repeated records are collapsed by endpoint identity, and the deterministic bounded subset may omit relevant relations.",
        ],
    };
    write_export(&mut io::stderr().lock(), &header, total_records, edges)
}

fn json_line(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let mut encoded = FRAME.to_vec();
    serde_json::to_writer(&mut encoded, value).map_err(io::Error::other)?;
    encoded.push(b'\n');
    Ok(encoded)
}

fn write_export(
    writer: &mut impl Write,
    header: &Header<'_>,
    total_records: usize,
    edges: impl Iterator<Item = Edge>,
) -> io::Result<()> {
    let mut retained = BTreeSet::new();
    let mut total_unique_edges = 0;
    for edge in edges {
        total_unique_edges += 1;
        if header.max_edges > 0 {
            retained.insert(edge);
            if retained.len() > header.max_edges as usize {
                retained.pop_last();
            }
        }
    }
    if total_unique_edges > total_records {
        return Err(io::Error::other(
            "Unique edge count exceeds observed record count",
        ));
    }
    let mut footer = Footer {
        schema: SCHEMA,
        kind: "complete",
        export_complete: true,
        coverage: "untruncated",
        total_observed_records: total_records,
        total_unique_edges,
        duplicate_records: total_records - total_unique_edges,
        emitted_edges: total_unique_edges,
        omitted_edges: total_unique_edges,
    };
    // Both counters at their maximum give an upper bound for the footer's
    // encoded length; reserve it before emitting any relation.
    let reserve = json_line(&footer)?.len();
    let header_bytes = json_line(header)?;
    let mut used = header_bytes.len() + reserve;
    if used > header.max_bytes as usize {
        return Err(io::Error::other(
            "JSONL budget cannot hold scope metadata and footer",
        ));
    }
    writer.write_all(&header_bytes)?;
    let mut emitted = 0;
    for edge in retained {
        let bytes = json_line(&EdgeRecord {
            schema: SCHEMA,
            kind: "edge",
            edge: &edge,
        })?;
        if bytes.len() <= header.max_bytes as usize - used {
            writer.write_all(&bytes)?;
            used += bytes.len();
            emitted += 1;
        }
    }
    footer.emitted_edges = emitted;
    footer.omitted_edges = total_unique_edges - emitted;
    footer.coverage = if footer.omitted_edges == 0 {
        "untruncated"
    } else {
        "truncated"
    };
    writer.write_all(&json_line(&footer)?)?;
    writer.flush()
}

#[cfg(test)]
mod tests;
