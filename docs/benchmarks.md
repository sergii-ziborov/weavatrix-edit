# Benchmark methodology

No public competitor result is reported for `weavatrix-edit` 0.1.1. The current
`edit_engine` benchmark is an internal smoke measurement. The independent
`tools/competitor-bench` harness verifies exact byte-output parity before timing
Mago, rust-analyzer, typst-edit, and Weavatrix, but no run is public evidence
until its environment and raw samples are recorded.

This file defines the conditions that must be met before a result is added to
the README or release notes.

## Separate contracts

Report these workloads independently:

1. **Coordinate mapping** — line/character to byte offset and byte offset back
   to position for UTF-8, UTF-16, and UTF-32.
2. **Prepared byte application** — all candidates receive already validated
   UTF-8 byte ranges over the same immutable source.
3. **Validated batch application** — bounds, scalar boundaries, overlap, and
   whole-set admission are included.
4. **Exact-plan application** — UTF-16 mapping, provenance, mandatory `before`
   comparison, bounds, overlap, and output-size checks are included.
5. **Composition** — merge two disjoint prepared plans and project original
   offsets through the result.
6. **Rejected inputs** — out-of-range, overlap, split Unicode scalar, and
   `before` mismatch return their documented failure without producing output.

Do not compare one contract with another in a single speed-ratio column.

## Corpus matrix

At minimum include:

| Source | Sizes | Edit counts | Edit shapes |
| --- | --- | --- | --- |
| ASCII with LF | 1 KiB, 1 MiB, 16 MiB | 1, 100, 2,000 | insert, replace, delete, mixed |
| ASCII with CRLF | 1 KiB, 1 MiB | 1, 100, 2,000 | line-local and multi-line |
| BMP Unicode | 1 KiB, 1 MiB | 1, 100, 2,000 | around multi-byte BMP scalar boundaries |
| Astral Unicode | 1 KiB, 1 MiB | 1, 100, 2,000 | valid surrogate-width positions |
| Same-position insertions | 1 MiB | 2, 100, 2,000 | stable input order |

Fixed seeds and generated fixture hashes must be recorded. Every edit set must
have a precomputed expected output.

## Competitor routing

- Mago Text Edit receives prepared byte-buffer edits for byte and validated
  batch workloads.
- rust-analyzer `TextEdit` receives equivalent non-overlapping byte edits for
  apply and composition workloads.
- typst-edit receives equivalent valid UTF-8 byte-range edits for strict byte
  application.
- textum 0.4.0 is adjacent but not included in byte-batch ratios: its in-memory
  direct-range primitive applies one character-range edit to a persistent Rope,
  while its PatchSet batch API resolves snippets and reads files. Neither is the
  same contract as a validated in-memory byte batch.
- lsp-textdocument participates in coordinate mapping, not in an exact-before
  batch comparison.
- rope libraries participate only in explicitly labelled persistent-buffer
  workloads.

Unsupported contracts are reported as unsupported, not emulated in a way that
attributes adapter cost or guarantees to the competitor.

## Correctness gate

Before any timed sample:

1. the implementation must accept the intended valid input;
2. the produced bytes must exactly equal the expected output;
3. insertion order must match the declared contract;
4. the source must remain unchanged after a rejected input;
5. a successful run must report or contain the expected edit count.

Any implementation that fails the gate is excluded from that row with the
reason recorded. A process exit code alone is insufficient evidence.

## Timing protocol

- compile optimized release artifacts;
- pin exact dependency versions and source revisions;
- keep source, edits, and expected outputs immutable across implementations;
- warm each implementation before measurement;
- run an odd number of measured samples, at least 31 for microbenchmarks;
- rotate or deterministically shuffle implementation order between rounds;
- use `black_box` or an equivalent barrier around input and output;
- report median wall time and p95 dispersion only with at least 21 samples;
- report throughput only when input and output accounting are identical;
- do not include dependency download, compilation, or CLI startup unless the
  row is explicitly a full-process benchmark;
- record peak RSS or allocations separately from wall time.

## Environment record

Every published result must include:

- UTC date;
- repository commit and dirty/clean state;
- competitor versions and immutable source revisions;
- OS and kernel/build;
- CPU model, logical cores, and memory;
- Rust version and target triple;
- build flags;
- warmup count, sample count, fixture seed, and corpus hashes;
- whether antivirus, indexing, or other material background load was present.

Machine-specific results must not be presented as a universal ranking.

## Current native smoke benchmark

Run:

```text
cargo bench --locked --bench edit_engine
```

The checked-in benchmark applies one verified UTF-16 edit to an approximately
1 MiB source and reports the median of 31 samples. It is useful as a local
regression signal. It does not warm up, compare competitors, report memory, or
cover the full matrix above, so its number must not appear as competitive
evidence.

## Current output-equivalent competitor harness

Run:

```text
cargo run --release --manifest-path tools/competitor-bench/Cargo.toml
```

To include every normalized timing sample in the output:

```text
cargo run --release --manifest-path tools/competitor-bench/Cargo.toml -- --raw
```

The harness uses pinned competitor versions and constructs native edit
collections before timing. Before any sample, both one-shot and reusable-plan
outputs are checked byte-for-byte against an independent reference splicer,
including declared order for 1,000 insertions at one offset. Three warmup rounds
and every measured round rotate the engine order. Reports include median,
p25/p75, deterministic fixture hashes, and optional raw nanoseconds per
operation. Nearest-rank p95 and p95/median are shown only for workloads with at
least 21 measured samples; smaller runs print `n/a` instead of presenting their
single worst sample as p95 evidence.

It reports three operations independently: valid `batch+apply`, reusable-plan
`prepare`, and `prepared-apply`. Typst participates only in `batch+apply`
because it has no reusable plan. The workload shapes are sparse mixed edits,
replacement-heavy edits, and same-offset insertion-heavy edits. The benchmark
deliberately covers exact byte-range valid inputs; rejection guarantees differ,
and rust-analyzer's trusted-range/assertion contract must not be described as
fail-closed validation.

Absolute timings are reported for every engine. Cross-engine time ratios are
reported only for prepared replay. `batch+apply` and `prepare` intentionally
have no ratio: Weavatrix verifies mandatory `before` evidence and hard budgets,
Mago has no per-range `before` proof, rust-analyzer does not validate the source,
and typst-edit has its own strict string-boundary checks. Output parity on valid
fixtures does not make those admission contracts equivalent.

Mago's prepared editor is consuming: `finish` takes ownership. Its clone is
therefore included in every prepared replay. Rust-analyzer's required source
String clone is also included; Weavatrix replays its borrowed prepared plan
directly. This preserves the actual reuse cost instead of moving required work
outside the timer.

The phase metric is also contract-specific. Output-producing `batch+apply`,
`prepared-apply`, and `write_to` rows report output MiB/s. `prepare` reports
edits/s because the engines do not all scan the complete source. The `chunks`
diagnostic only traverses borrowed chunks and observes their lengths, so it does
not report byte throughput.

The Weavatrix `prepare` row uses the consuming owned-edit API, so native
replacement strings are moved into the prepared plan just as Mago consumes its
native batch. Harness-only input clones happen before timing for both engines;
the borrowed Weavatrix prepare surface is a distinct caller-retains-input
contract.

No cross-engine sink/stream ratio is reported. The harness includes labelled
Weavatrix-only diagnostics for zero-copy `chunks` traversal and `write_to` into
a preallocated Vec. The timed competitors allocate a complete Vec/String or
mutate a String and expose no equivalent prepared batch-to-Write API. An
allocate-then-write adapter would measure a different contract. The harness
also does not represent UTF-16, JSON, multi-file, or secure worktree behavior.

Its terminal output is intentionally not checked in as a benchmark result.
Publication still requires the complete environment record and raw samples
defined above.

## Publishing results

Add measured tables only after committing:

- the harness source;
- its lockfile;
- fixture generator or immutable corpus identifiers;
- raw machine-readable samples;
- the summarized table;
- an exact command that reproduces the run.

The README should link here and summarize only the contracts actually measured.
