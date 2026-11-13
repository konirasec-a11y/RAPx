# Bounded callgraph observations

The existing `cargo rapx analyze callgraph` text output remains the default.
An opt-in format exports a deterministic bounded subset of the relations
already observed by the analyzer:

```sh
cargo rapx --test-crate my_package analyze callgraph --format jsonl --max-edges 1000 --max-bytes 4194304 -- --package my_package --lib --locked
```

Each record is written to stderr as `RAPX_CALLGRAPH_JSONL ` followed by one
JSON object and a newline. Consumers must strip that exact frame before JSON
decoding. The frame prevents Cargo from consuming bare JSON as rustc diagnostic
messages. Other compiler/analyzer diagnostics remain in the same stderr stream.

The schema is `rapx.callgraph/v1`. A `header` identifies the current compiler
crate, actual Cargo package when available, requested package filter and output
limits. Each `edge` carries display paths and compiler-local endpoint IDs.
Relations with the same endpoint identities are collapsed; different crate
identities remain distinct even when their display paths are equal.

The final `complete` record reports `export_complete=true`,
`total_observed_records`, `total_unique_edges`, `duplicate_records`,
`emitted_edges` and `omitted_edges`. `coverage=truncated` when any observed
unique relations were omitted by either limit. Otherwise it is `untruncated`.
Consumers requiring all observed relations must reject truncated output, a
missing footer, inconsistent counts, failed execution or externally cut output.
Export completion never means complete call resolution or program safety.

`--max-edges` accepts 0 through 100000; zero exports metadata and counts only.
`--max-bytes` accepts 4096 through 67108864 and bounds the framed JSONL bytes,
including header/footer. It does not bound unrelated Cargo diagnostic output,
analyzer computation or the analyzer's existing in-memory graph. The exporter
retains at most the requested number of lexically ordered relations and a
per-caller deduplication set rather than formatting the entire graph.

The existing analyzer can skip unresolved function-pointer calls and include
implementation candidates for virtual dispatch. Counts describe its existing
observed relation records, not all runtime calls or full callsite coverage.
Callers without observed relations are not exported. These limitations apply
even when the export is untruncated. No second analysis or global architecture
graph is introduced.
