# Competitor benchmark

This independent harness compares output-equivalent, exact byte-range edits in:

- `weavatrix-edit` from the repository under test;
- `mago-text-edit = 1.45.0`;
- `ra_ap_text_edit = 0.0.241`;
- `typst-edit = 0.1.0` for its available total-operation API.

`textum = 0.4.0` was also audited, but is intentionally not put in the same
timed table. Its public direct-range primitive applies one character-range edit
to a persistent `Rope`; its `PatchSet` batch API resolves snippets and reads
files. Looping the primitive would invent batch atomicity, while timing
`PatchSet` would add target resolution and filesystem work absent from this
contract.

Run it outside Cargo's built-in benchmark profile so every competitor is built
with the same release settings:

```console
cargo run --release --manifest-path tools/competitor-bench/Cargo.toml
```

Append `-- --raw` to print every normalized sample in nanoseconds per operation.

The harness verifies every one-shot and prepared result against an independent
reference splicer before timing. This includes the declared order of many
insertions at one offset. Measurements use three rotated warmup rounds, then
rotated measured rounds; sub-millisecond workloads are batched and normalized
to one operation. It reports median and p25/p75 for every workload. Nearest-rank
p95 and p95/median are shown only with at least 21 measured samples; smaller
runs print `n/a` because their p95 would be only the single worst sample.

The phases are deliberately separate:

- `batch+apply` includes the native API's valid-input admission/preparation and
  output production from an already-native edit collection;
- `prepare` creates a reusable native edit object without producing output;
- `prepared-apply` replays that object against the same immutable source.

Absolute timings are shown for every engine, but a cross-engine ratio is shown
only for `prepared-apply`. Valid-output parity does not make admission contracts
equivalent: Weavatrix checks mandatory `before` evidence and hard budgets, Mago
has no per-range `before` proof, rust-analyzer does not validate the source, and
typst-edit performs its own strict string-boundary validation.

Output-producing `batch+apply`, `prepared-apply`, and `write_to` rows report
output MiB/s. `prepare` reports edits/s because the engines do not all scan the
complete source.

Two rows are explicitly Weavatrix-only: zero-copy `chunks` traversal and
`write_to` into a preallocated `Vec<u8>`. They have no competitor ratio. The
chunk traversal only observes borrowed chunk lengths, so it does not report
byte throughput.

Typst has no reusable prepared-plan API and is therefore shown only under
`batch+apply`. The matrix contains sparse mixed edits, replacement-only edits,
and 1,000 ordered insertions at the same offset.

For one-shot and prepare operations, cloning an already-native input collection
needed only to repeat a consuming API happens before the timer. Mago's `finish`
consumes its `TextEditor`, so its clone is inside every prepared-replay timing;
otherwise the row would claim reuse the API does not provide. Rust-analyzer
mutates a `String`, so cloning the immutable source into the output buffer also
remains inside the timer. Native edit construction and adapter conversion are
excluded for every engine.

The Weavatrix `prepare` row uses its consuming
`prepare_byte_edits_owned_with_limits` API, matching Mago's consuming
`apply_batch(Vec<_>)` ownership contract. Both move native replacement buffers
into a prepared value; the repeated input clones required by the harness occur
before timing. The separate borrowed Weavatrix prepare API remains available
when the caller needs to retain its edit collection.

This is a byte-range fast-path comparison. It deliberately excludes
Weavatrix-only `expected_before` construction, UTF-16 indexing, JSON parsing,
multi-file validation, and source fingerprints from competitor claims. Those
features need separate correctness and secure-profile measurements. The actual
Weavatrix `before` comparison remains inside its `batch+apply` and `prepare`
timings; only adapter construction is outside the timer. Rejection semantics
are also not interchangeable: rust-analyzer relies on trusted ranges and
assertions, while the other measured one-shot APIs return admission errors.

Weavatrix's `PreparedEdits::chunks` and `write_to` are timed only as labelled
single-engine diagnostics. None of the three timed competitors exposes an
equivalent prepared batch-to-`std::io::Write` API; measuring their
allocate-then-write adapters would answer a different question.
