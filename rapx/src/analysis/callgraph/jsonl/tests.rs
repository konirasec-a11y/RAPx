use super::*;
use clap::Parser;
use serde_json::Value;

fn header(max_edges: u32, max_bytes: u32) -> Header<'static> {
    Header {
        schema: SCHEMA,
        kind: "header",
        scope: "current_compiler_crate",
        cargo_package: Some("sample"),
        compiler_crate: "sample",
        requested_package: Some("sample"),
        max_edges,
        max_bytes,
        limitations: [
            "not complete resolution",
            "compiler IDs",
            "observed relations",
        ],
    }
}

fn edge(name: &str) -> Edge {
    Edge {
        caller: "sample::root".into(),
        callee: name.into(),
        caller_id: "0:1".into(),
        callee_id: format!("0:{name}"),
    }
}

fn render(header: &Header<'_>, raw_count: usize, edges: Vec<Edge>) -> (Vec<u8>, Vec<Value>) {
    let mut output = Vec::new();
    write_export(&mut output, header, raw_count, edges.into_iter()).unwrap();
    let parsed = String::from_utf8(output.clone())
        .unwrap()
        .lines()
        .map(|line| {
            serde_json::from_str(line.strip_prefix("RAPX_CALLGRAPH_JSONL ").unwrap()).unwrap()
        })
        .collect();
    (output, parsed)
}

#[test]
fn bounded_unique_output_is_deterministic_with_exact_record_and_omission_counts() {
    let head = header(2, 4096);
    let (first, records) = render(&head, 7, vec![edge("z"), edge("b"), edge("a")]);
    let (second, _) = render(&head, 7, vec![edge("a"), edge("z"), edge("b")]);
    assert_eq!(first, second);
    assert_eq!(records[1]["callee"], "a");
    assert_eq!(records[2]["callee"], "b");
    let footer = records.last().unwrap();
    assert_eq!(footer["total_observed_records"], 7);
    assert_eq!(footer["total_unique_edges"], 3);
    assert_eq!(footer["duplicate_records"], 4);
    assert_eq!(footer["emitted_edges"], 2);
    assert_eq!(footer["omitted_edges"], 1);
    assert_eq!(footer["coverage"], "truncated");
    assert_eq!(footer["export_complete"], true);
}

#[test]
fn zero_limit_exports_only_metadata_and_an_honest_truncation_footer() {
    let (_, records) = render(&header(0, 4096), 2, vec![edge("a"), edge("b")]);
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["emitted_edges"], 0);
    assert_eq!(records[1]["omitted_edges"], 2);
    assert_eq!(records[1]["coverage"], "truncated");
    let (_, empty) = render(&header(0, 4096), 0, vec![]);
    assert_eq!(empty[1]["coverage"], "untruncated");
    assert_eq!(empty[1]["total_unique_edges"], 0);
}

#[test]
fn bytes_are_bounded_and_json_escaping_preserves_endpoint_names() {
    let special = "unicode::ameaça::<\"quote\">\nline\\path";
    let (output, records) = render(
        &header(3, 4096),
        2,
        vec![edge(special), edge(&"x".repeat(8192))],
    );
    assert!(output.len() <= 4096);
    assert_eq!(records[1]["callee"], special);
    assert_eq!(records.last().unwrap()["emitted_edges"], 1);
    assert_eq!(records.last().unwrap()["omitted_edges"], 1);
    assert_eq!(records.last().unwrap()["coverage"], "truncated");
}

#[test]
fn identical_display_paths_keep_distinct_compiler_identities() {
    let one = edge("dependency::same");
    let mut two = one.clone();
    two.callee_id = "different-crate-version:1".into();
    let (_, records) = render(&header(10, 4096), 2, vec![one, two]);
    assert_eq!(records.len(), 4);
    assert_eq!(records.last().unwrap()["total_unique_edges"], 2);
    assert_ne!(records[1]["callee_id"], records[2]["callee_id"]);
}

#[test]
fn failed_writes_cannot_produce_a_completion_footer() {
    struct Broken {
        bytes: Vec<u8>,
        remaining: usize,
    }
    impl Write for Broken {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::other("broken pipe"));
            }
            let count = bytes.len().min(self.remaining);
            self.bytes.extend_from_slice(&bytes[..count]);
            self.remaining -= count;
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut broken = Broken {
        bytes: Vec::new(),
        remaining: 100,
    };
    assert!(write_export(&mut broken, &header(10, 4096), 1, [edge("a")].into_iter()).is_err());
    assert!(!String::from_utf8_lossy(&broken.bytes).contains("export_complete"));
}

#[test]
fn metadata_that_exceeds_byte_budget_fails_before_writing() {
    let mut output = Vec::new();
    assert!(write_export(&mut output, &header(0, 1), 0, [].into_iter()).is_err());
    assert!(output.is_empty());
}

#[test]
fn cli_preserves_text_default_and_validates_jsonl_limits() {
    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: crate::cli::RapxArgs,
    }
    let default = TestCli::try_parse_from(["rapx", "analyze", "callgraph"]).unwrap();
    let crate::cli::Commands::Analyze {
        kind: crate::cli::AnalysisKind::Callgraph(args),
    } = default.args.command
    else {
        panic!("wrong command");
    };
    assert_eq!(args.format, crate::cli::CallgraphFormat::Text);
    assert_eq!(args.max_edges, 1000);
    for (flag, value) in [
        ("--max-edges", "-1"),
        ("--max-edges", "100001"),
        ("--max-bytes", "4095"),
        ("--max-bytes", "67108865"),
    ] {
        assert!(
            TestCli::try_parse_from([
                "rapx",
                "analyze",
                "callgraph",
                "--format",
                "jsonl",
                flag,
                value
            ])
            .is_err()
        );
    }
    assert!(
        TestCli::try_parse_from([
            "rapx",
            "analyze",
            "callgraph",
            "--format",
            "jsonl",
            "--max-edges",
            "0"
        ])
        .is_ok()
    );
}
