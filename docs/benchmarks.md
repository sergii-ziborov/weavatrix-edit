# Benchmark methodology

The independent `tools/competitor-bench` harness compares `weavatrix-edit`,
Mago Text Edit 1.45.0, `ra_ap_text_edit` 0.0.241, and typst-edit 0.1.0. It
verifies every final output against an independent splicer before timing.
Results are machine-specific and contract-specific, not a universal library
ranking.

## Evidence status

Timing evidence is recorded only after the implementation has a clean immutable
commit. The source commit is benchmarked with `--raw` and the relevant release
gate; a following evidence-only commit records the exact commit hash,
environment, raw samples, summaries, command, and exit status. Dirty-tree
exploratory runs are deliberately not retained here.

The 0.1.2 implementation was frozen in clean commit
`228f95227c49f0c750bbecb3d02d267c0ab45cf0`. The complete matrix and a separate
predeclared default-ceiling raw gate were then run without source changes. The
following evidence-only commit records the commands, run windows, environment,
summaries, and every default-ceiling sample:

- [Windows clean-commit evidence](https://github.com/sergii-ziborov/weavatrix-edit/blob/main/benchmark-results/2026-08-02-windows-clean-228f952.md)

The default-ceiling caller-String median was 2.14 ms for Weavatrix and 5.44 ms
for Mago. The conservative fastest-competitor-p25 / Weavatrix-p75 gate passed
at 2.24x. The full matrix remains deliberately mixed and is not a universal 2x
claim: rust-analyzer was about 9% faster for equal-length caller-String replay,
while Weavatrix was 2.16x faster for the equivalent caller-Vec output.

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

The primary prepared and caller-buffer phases keep Weavatrix's optional
`rendered_text` cache uninitialized; the correctness gate asserts this invariant
before timing. A future warm-rendered-copy row must pre-render every engine's
verified output and label that distinct state explicitly. It must not compare a
Weavatrix cache hit against Mago `finish` or rust-analyzer edit application.

It reports valid `batch+apply`, reusable-plan `prepare`, allocating
`prepared-apply`, reusable caller-`String`, reusable caller-`Vec`, zero-copy
chunks, and `write_to` independently. Typst participates only in `batch+apply`
because it has no reusable plan. The workload shapes are sparse mixed edits,
replacement-heavy edits, and same-offset insertion-heavy edits. The benchmark
deliberately covers exact byte-range valid inputs; rejection guarantees differ,
and rust-analyzer's trusted-range/assertion contract must not be described as
fail-closed validation.

Absolute timings are reported for every engine. Cross-engine time ratios are
reported only for output-equivalent caller-buffer replay. Allocating prepared
replay retains native output ownership and is report-only.
`batch+apply` and `prepare` intentionally have no ratio: Weavatrix verifies
mandatory `before` evidence and hard budgets,
Mago has no per-range `before` proof, rust-analyzer does not validate the source,
and typst-edit has its own strict string-boundary checks. Output parity on valid
fixtures does not make those admission contracts equivalent.

Mago's prepared editor is consuming: `finish` takes ownership. Its clone is
therefore included in every prepared replay. Rust-analyzer's required source
String clone is also included; Weavatrix replays its borrowed prepared plan
directly. This preserves the actual reuse cost instead of moving required work
outside the timer.

The phase metric is also contract-specific. Output-producing `batch+apply`,
`prepared-apply`, caller-buffer, and `write_to` rows report output MiB/s.
`prepare` reports
edits/s because the engines do not all scan the complete source. The `chunks`
diagnostic only traverses borrowed chunks and observes their lengths, so it does
not report byte throughput.

The Weavatrix `prepare` row uses the consuming owned-edit API, so native
replacement strings are moved into the prepared plan just as Mago consumes its
native batch. Harness-only input clones happen before timing for both engines;
the borrowed Weavatrix prepare surface is a distinct caller-retains-input
contract.

No cross-engine generic-`Write` ratio is reported. The harness includes labelled
Weavatrix-only diagnostics for zero-copy `chunks` traversal and `write_to` into
a cleared, reused Vec. The caller-buffer phases are separate: all engines leave
the same complete reusable `String` or `Vec<u8>`, and every API-required reset,
clone, apply, and final copy remains inside the timer. The harness does not
represent UTF-16, JSON, multi-file, or secure worktree behavior.

The caller-`String` contract is a complete reusable Rust `String`. Mago's native
result is `Vec<u8>`, so its required full UTF-8 validation and String copy stay
inside that timer. The caller-`Vec` contract is a complete reusable byte vector;
rust-analyzer natively mutates a String, so its source restoration, edit apply,
and final byte copy all stay inside that timer. Allocating prepared rows retain
their native output ownership and are report-only rather than a strict
same-output-type ratio.

## Publishing results

Add measured tables only after committing:

- the harness source;
- its lockfile;
- fixture generator or immutable corpus identifiers;
- raw machine-readable samples;
- the summarized table;
- an exact command that reproduces the run.

The README should link here and summarize only the contracts actually measured.
