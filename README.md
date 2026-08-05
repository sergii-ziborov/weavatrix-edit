# Weavatrix Edit

[![CI](https://github.com/sergii-ziborov/weavatrix-edit/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/weavatrix-edit/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/weavatrix-edit.svg)](https://crates.io/crates/weavatrix-edit)
[![docs.rs](https://docs.rs/weavatrix-edit/badge.svg)](https://docs.rs/weavatrix-edit)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://github.com/sergii-ziborov/weavatrix-edit/blob/main/Cargo.toml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/sergii-ziborov/weavatrix-edit/blob/main/LICENSE)

`weavatrix-edit` is a deterministic, fail-closed Rust library for validating
and applying source-code edit plans. It provides strict LSP-compatible UTF-16
coordinates, exact `before` verification, evidence-labelled edits, bounded
application, prepared-plan composition, streaming without a final-result
allocation, and the extensible
`weavatrix.edit-plan.v1` JSON contract.

The crate does not open paths or own filesystem operations. Prepared output can
either be returned in memory or streamed to a caller-owned `std::io::Write`.

## Scope

`weavatrix-edit` owns:

- strict UTF-8, UTF-16, and UTF-32 position-to-byte conversion;
- the fixed 1-based line / 0-based UTF-16 position convention used by wire v1;
- validated insert, delete, and replacement operations over immutable source;
- exact `before` checks, Unicode-scalar boundaries, overlap detection, and
  stable same-position insertion order;
- bounded operation labels, source, edit-count, output-size, and multi-file plan validation;
- zero-copy validation views for higher-level planners that already own
  `FileEdit`-compatible fields and must not clone edit text;
- fallible reusable line indexing with explicit line-count and index-byte
  ceilings;
- bounded incremental admission of original-source byte edits with atomic
  `push_batch` rollback;
- portable repository-relative path validation;
- prepared edit union, source-offset invalidation, and forward offset mapping;
- allocation-free normalized change inspection with exact source/output ranges,
  retained provenance, and aggregate byte statistics;
- bounded multi-error diagnostics with structured, truncated mismatch evidence;
- allocation-reusing caller-owned `String` and `Vec<u8>` output, zero-copy
  chunks, and a caller-owned writer adapter;
- an optional caller-supplied validator over the complete result;
- Serde models for the extensible `weavatrix.edit-plan.v1` envelope.

## Non-scope

The crate deliberately does not own:

- filesystem access, repository-root containment, symlink checks, or encoding
  detection;
- SHA-256 calculation or verification against current file contents;
- locks, confirmation tokens, atomic writes, rollback, or recovery;
- symbol resolution, language parsing, rename planning, or LSP process
  management;
- mutable document sessions, revision rebasing, undo, or CRDT state;
- diffs, patches, formatting, undo history, or a persistent rope/editor buffer;
- atomic application across several files.

Those responsibilities belong to `weavatrix-worktree` and
`weavatrix-refactor-rust`. A `FileEdit.sha256` value is validated for wire
shape here; a transaction layer must compare it with the actual file.

## Installation

```toml
[dependencies]
weavatrix-edit = "0.1"
```

Or run `cargo add weavatrix-edit`. The minimum supported Rust version is 1.88.
Published versions are available on
[crates.io](https://crates.io/crates/weavatrix-edit), with API documentation on
[docs.rs](https://docs.rs/weavatrix-edit).

## Quick start

```rust
use weavatrix_edit::{
    Position, Provenance, TextEdit, TextRange, apply_edits,
};

let source = "const getUser = 1;\n";
let edit = TextEdit::replace(
    TextRange::new(Position::new(1, 6), Position::new(1, 13)),
    "getUser",
    "getCustomer",
    Provenance::EXACT_LSP,
);

let applied = apply_edits(source, &[edit])?;
assert_eq!(applied.text, "const getCustomer = 1;\n");
assert_eq!(applied.edits_applied, 1);
# Ok::<(), weavatrix_edit::EditError>(())
```

Every range refers to the original source. The engine resolves and verifies the
whole set before constructing output, so earlier edits never shift later ones.

## Public API

| Surface | Purpose |
| --- | --- |
| `TextEdit`, `ByteEdit` | Exact source edits in UTF-16 line/character or UTF-8 byte coordinates |
| `apply_edits`, `apply_edits_with_limits` | Validate and apply one complete edit set in memory |
| `prepare_edits`, `prepare_byte_edits` | Bind validated edits to an immutable source for reuse or composition |
| `prepare_byte_edits_owned` | Consume a native byte batch and move replacements into the prepared plan |
| `ByteEditBatch` | Incrementally admit bounded original-source edits; batch admission is transactional |
| `PreparedEdits` | Apply into a new or reusable output, opt into a retained rendered view, inspect changes/statistics, stream to `Write`, union plans, and map offsets |
| `ApplySummary` | Exact edit and byte totals returned by caller-buffer application |
| `PreparedChange`, `ChangeSummary` | Exact source/output preview ranges, before/after slices, provenance, and aggregate byte totals |
| `EditChunks` | Sink-independent zero-copy iteration for sync or async consumers |
| `WriteSummary` | Successful streamed-output counts without retaining the complete result |
| `LineIndex`, `LineIndexLimits` | Strict UTF-8/UTF-16/UTF-32 position conversion, with a fallible bounded constructor |
| `EditPlan`, `FileEdit` | Extensible multi-file `weavatrix.edit-plan.v1` wire model |
| `DeclaredEditPlan` | Decode the same envelope without materializing extension members |
| `validate_edit_plan` | Validate schema, paths, provenance, hashes, uniqueness, and budgets |
| `BorrowedFileEdit`, `validate_file_edits` | Validate arbitrary borrowed file/edit slices through the same internal engine, without cloning text |
| `EditValidationStats` | Owned edit-count and text-byte totals returned by borrowed validation |
| `PlanLimits`, `ApplyLimits`, `BatchLimits` | Explicit resource ceilings with conservative defaults |
| `diagnose_edits`, `diagnose_byte_edits` | Non-mutating multi-error preflight with bounded retained diagnostics |
| `DiagnosticLimits`, `ValidationReport` | Hard item/preview ceilings and structured exact-before mismatch evidence |
| `EditError`, `ErrorCode` | Stable machine-readable failures plus file/edit indices |

`PreparedEdits::bytes_before` and `bytes_after` expose exact preflight sizes so
a transaction layer can enforce an aggregate multi-file budget before writing.
Plan operation labels have the patch-compatible hard ceiling
`MAX_PLAN_OPERATION_BYTES` (4 KiB); text, file, and edit ceilings remain
caller-configurable through `PlanLimits`.

Higher-level plan crates can assemble a small slice of `BorrowedFileEdit`
views over their own operation model. `validate_file_edits` applies exactly the
same path, hash, provenance, extension-key, uniqueness, and aggregate-budget
checks used by `EditPlan::validate_with`; only the edit strings and extension
trees remain borrowed. Schema and completeness belong to the caller's envelope.
Each view declares the reserved member names of its source envelope through
`reserved_extension_keys`; `BorrowedFileEdit::from(&FileEdit)` supplies
`FILE_EDIT_RESERVED_EXTENSION_KEYS` automatically, while a different envelope
must name its own fields explicitly.

Hot replay loops can retain their output allocation. `apply_into` refills a
caller-owned `String`; `apply_into_bytes` writes the same guaranteed-valid UTF-8
bytes directly into a caller-owned `Vec<u8>` for files, hashes, or sockets:

```rust
use weavatrix_edit::{ByteEdit, Provenance, prepare_byte_edits};

let prepared = prepare_byte_edits(
    "let answer = 41;",
    &[ByteEdit::replace(13..15, "41", "42", Provenance::EXACT_LSP)],
)?;
let mut output = String::with_capacity(prepared.bytes_after());
let summary = prepared.apply_into(&mut output);
assert_eq!(output, "let answer = 42;");
assert_eq!(summary.edits_applied, 1);
# Ok::<(), weavatrix_edit::EditError>(())
```

Ordinary `apply`, `apply_into`, `apply_into_bytes`, `chunks`, and `write_to`
do not retain a second output-sized allocation. A hot read/replay loop can
explicitly call `rendered_text()` once to retain the complete result and borrow
it without copying; later `apply*` calls copy that contiguous materialization.
`has_rendered_text()` exposes the memory state and `clear_rendered_text()`
releases it. Cloning a plan does not clone the retained output, and `union`
always invalidates it. Prefer `chunks()` or `write_to()` when bounded streaming
memory matters more than repeated access latency.

Prepared execution metadata can additionally coalesce consecutive insertions
at one source offset while preserving their individual preview ranges, order,
and provenance. The duplicated insertion text has one hard 64 KiB ceiling per
plan; runs beyond that global budget use the ordinary non-coalesced path.

`PreparedEdits::apply_with_validator` is useful for callers that must reject
the complete output unless it remains parseable or satisfies a domain-specific
invariant:

```rust
use weavatrix_edit::{
    Position, Provenance, TextEdit, TextRange, prepare_edits,
};

let source = "value";
let edit = TextEdit::replace(
    TextRange::new(Position::new(1, 0), Position::new(1, 5)),
    "value",
    "result",
    Provenance::RESOLVED,
);

let prepared = prepare_edits(source, &[edit])?;
let applied = prepared.apply_with_validator(|text| text == "result")?;
assert_eq!(applied.text, "result");
# Ok::<(), weavatrix_edit::EditError>(())
```

## Staged batch admission

`ByteEditBatch` is the bounded counterpart to accumulating a `Vec<ByteEdit>`.
Every coordinate continues to address the immutable original source. A failed
`push` leaves prior edits intact; a failed `push_batch` admits none of that
batch. This is intentionally different from a live editor session where each
change addresses a new document revision.

```rust
use weavatrix_edit::{ByteEdit, ByteEditBatch, Provenance};

let source = "alpha beta";
let mut batch = ByteEditBatch::new(source)?;
batch.push(ByteEdit::insert(5, "-", Provenance::EXACT_LSP))?;
batch.push_batch(vec![ByteEdit::replace(
    6..10,
    "beta",
    "gamma",
    Provenance::EXACT_LSP,
)])?;

let applied = batch.finish()?.apply();
assert_eq!(applied.text, "alpha- gamma");
# Ok::<(), weavatrix_edit::EditError>(())
```

`BatchLimits` independently caps source bytes, edit count, accumulated
`before` bytes, accumulated replacement bytes, and final output bytes. The
fallible `finish` also enforces the final output ceiling for an empty batch;
an atomic batch may shrink a source that initially exceeds that ceiling.

## Structured preview and diagnostics

`PreparedEdits::changes()` exposes each normalized change without applying the
plan or running a second diff engine. Every item contains the exact source and
output byte ranges, borrowed `before`/`after` text, deterministic input order,
and all provenance labels retained when identical replacements are unioned.
`change_summary()` returns exact inserted, removed, input, and output byte
totals without allocating the final result.

`diagnose_edits` and `diagnose_byte_edits` validate the complete admitted set
without applying it. They continue across independent position, exact-before,
and overlap failures. `DiagnosticLimits` caps both retained item count and each
expected/actual preview. Error messages never interpolate complete source text;
full lengths, bounded UTF-8 previews, truncation state, and the exact source
range are available as structured mismatch evidence.

## Parallel multi-file use

`PreparedEdits` is immutable, `Send`, and `Sync`. Independent files can be
prepared and streamed concurrently by a caller, while every individual plan
retains deterministic original-source semantics. This crate intentionally does
not create a thread pool or depend on an async runtime: `weavatrix-worktree`
owns bounded scheduling for repository I/O, SHA-256 preflight, temporary-file
staging, and rollback. Its commit phase remains deterministic even when
preparation and staging run in parallel.

## Position contract

The wire v1 convention is intentionally fixed:

- lines are 1-based;
- characters are 0-based UTF-16 code units;
- ranges are half-open: `[start, end)`;
- a line feed (`\n`) is not part of the preceding line's addressable content;
- for compatibility with the shipping v1 contract, a carriage return in a
  CRLF pair remains part of that line;
- the empty line after a trailing `\n` is addressable;
- positions beyond a line or file fail instead of being clamped;
- a position between the two UTF-16 units of an astral Unicode scalar fails;
- resolved byte offsets must be UTF-8 scalar boundaries.

`LineIndex::byte_offset_with_encoding` also supports strict UTF-8 and UTF-32
coordinates for adapters, but serialized `TextEdit` values always use the UTF-16
wire convention. See [the complete v1 contract](docs/edit-plan-v1.md).

`LineIndex::new` remains the compatibility constructor for trusted, already
bounded text. Untrusted callers should use
`LineIndex::try_new(text, LineIndexLimits { max_lines, max_index_bytes })`.
That API counts and validates the complete line-start table before a fallible
`try_reserve_exact`, so a newline-heavy input returns `PLAN_TOO_LARGE` instead
of depending on an allocator panic. `LineIndexLimits::default()` permits
1,000,000 logical lines and 8 MiB of line-start offsets.

One-shot `apply_edits*` and `prepare_edits*` calls do not construct that full
table. They scan the bounded source once and retain at most two sparse line
records per admitted edit before deduplication. Their index memory is therefore
`O(max_edits)`, even when nearly every source byte is a line feed; existing
`ApplyLimits` signatures and filesystem/worktree boundaries are unchanged.

## Guarantees

For every successful single-source application:

1. every edit is structurally valid and carries applicable provenance;
2. every range resolves inside the original source on a Unicode-scalar boundary;
3. the original slice equals the mandatory `before` text;
4. non-empty ranges do not overlap;
5. same-position insertions preserve their input order;
6. output size is checked before construction;
7. no partial result is returned when any validation fails;
8. output is constructed deterministically from the original source;
9. the crate executes no unsafe Rust and opens no filesystem or network resources.

The default application limits are 16 MiB of source, 2,000 edits, and 64 MiB
of output per source. The default plan limits are 500 files, 2,000 edits per
file, 1,000,000 total edits, 4,096 bytes per path, and 64 MiB of combined
`before`/`after` text. The default staged-batch limits additionally cap
accumulated `before` text at 16 MiB and replacement text at 64 MiB. Callers can
provide lower limits. A separately constructed full `LineIndex` has the
independent `LineIndexLimits` described above.

## Wire contract

`EditPlan`, `FileEdit`, and `TextEdit` serialize with camel-case field names.
Unknown fields are retained at all three levels so a v1 consumer can round-trip
extensions it does not interpret. The frozen core fields and runtime invariants
are documented in:

- [Edit plan v1](docs/edit-plan-v1.md)
- [JSON Schema 2020-12](docs/schema/weavatrix.edit-plan.v1.schema.json)

A schema-valid document is not automatically safe to apply. The Rust validator
also checks cross-field ordering, `before != after`, portable path aliases,
resource budgets, and the applicable provenance set. A worktree consumer must
then verify each file hash against the current repository.

Retaining extensions means building one owned JSON value per undeclared member,
at every level. A consumer that only validates or applies a plan never reads
them and can decode through `DeclaredEditPlan`, which accepts and rejects
exactly the same documents with the same error messages, recovers identical
declared data, and skips undeclared members structurally:

```rust
use weavatrix_edit::DeclaredEditPlan;

let json = r#"{
    "schemaVersion": "weavatrix.edit-plan.v1",
    "operation": "rename_symbol",
    "createdAt": "2026-08-01T12:00:00Z",
    "files": [{
        "path": "src/user.ts",
        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "edits": [{
            "startLine": 10, "startChar": 8, "endLine": 10, "endChar": 15,
            "before": "getUser", "after": "getCustomer", "provenance": "EXACT_LSP"
        }]
    }]
}"#;

let plan = blazingly_json::from_str::<DeclaredEditPlan>(json)?.into_plan();
assert!(plan.validate().is_ok());
assert!(plan.extensions.is_empty());
# Ok::<(), blazingly_json::Error>(())
```

The recovered plan is not round-trippable: re-serializing it emits declared
members only. Decode through `EditPlan` whenever extensions must survive.
Either path is driver-independent; both are exercised against `blazingly-json`
and `serde_json`.

## Competitive position

There is no exact established equivalent combining strict LSP coordinates,
evidence-backed `before` verification, prepared edit composition, and an
extensible multi-file wire contract. The closest projects solve important
subsets:

| Project | Strongest adjacent capability | Different boundary |
| --- | --- | --- |
| [Mago Text Edit 1.45](https://crates.io/crates/mago-text-edit) | High-performance byte-edit batches, safety thresholds, and final checking | Byte buffers; no LSP coordinate or multi-file plan contract |
| [rust-analyzer text edit](https://github.com/rust-lang/rust-analyzer/blob/804ee7d794d4677ba17835653a6b032504839466/crates/ide-db/src/text_edit.rs) | Mature builder, union, coalescing, and offset mapping | Internal trusted byte ranges; not an untrusted wire-plan validator |
| [typst-edit 0.1](https://crates.io/crates/typst-edit) | Fail-closed byte-range application plus Typst-aware source locators | Typst-specific; no UTF-16 or exact-before plan protocol |
| [lsp-textdocument 0.5](https://crates.io/crates/lsp-textdocument) | UTF-8/UTF-16/UTF-32 LSP document mapping and updates | Document manager that clamps some invalid positions, not a strict edit-plan engine |
| [textum 0.4](https://docs.rs/textum/0.4.0/textum/) | Rope-backed snippet targeting and multi-file `PatchSet` | Includes target resolution and filesystem behavior; not an equivalent exact byte-batch primitive |
| Rope libraries such as [Ropey](https://github.com/cessen/ropey) | Persistent large-document storage and incremental editing | Editor data structure, not a proven refactoring plan contract |

This is a capability comparison, not a speed ranking. See
[the evidence and pinned sources](docs/competitors.md).

## Performance evidence

The pinned, output-gated harness includes Mago Text Edit 1.45.0,
`ra_ap_text_edit` 0.0.241, and typst-edit 0.1.0. Every final output is checked
byte-for-byte before timing, and the optional rendered-output cache is asserted
to remain cold for the primary rows. Caller-`String` and caller-`Vec` results
are reported separately because the adapters required by Mago and
rust-analyzer differ; allocating prepared and admission rows are report-only.

The clean `228f952` release run measured the default-limit 10 MiB / 2,000-edit
caller-`String` replay at 2.14 ms versus Mago's 5.44 ms (2.54x by median). The
predeclared conservative gate uses fastest-competitor p25 divided by Weavatrix
p75 and passed at 2.24x. In the complete matrix, Weavatrix beat Mago on every
caller-owned String and Vec workload.

No universal 2x claim is made. Equal-length caller-`String` replay shares a
full-copy memory floor with rust-analyzer, which was about 9% faster on that
one recorded row; Weavatrix's direct caller-Vec path was 2.16x faster there.
See [the benchmark contract](docs/benchmarks.md) and the
[exact clean-commit environment, matrix, and raw samples](https://github.com/sergii-ziborov/weavatrix-edit/blob/main/benchmark-results/2026-08-02-windows-clean-228f952.md).

Envelope decoding is measured separately. Retaining extensions is the dominant
cost of decoding a large multi-file plan, because it builds one owned JSON
value per undeclared member at every level. Against the published 0.1.5 derive,
linked into the same process so both arms share machine state,
`DeclaredEditPlan` decodes a 500-file Unicode plan carrying extensions 1.80x to
2.06x faster, and a 1 KiB message 1.83x to 2.17x faster — resolvable on
`blazingly-json` and `serde_json`, under both a stock release and a fat-LTO
profile. On plans with no undeclared members every cell overlaps: nothing to
skip, and no regression. Removing `#[serde(flatten)]` while still capturing
extensions is worth at most about 1.2x on its own. Full matrix, protocol, and
quartiles: [decoder and envelope decode costs](docs/decoder-comparison.md).

## Limitations

- Input is valid UTF-8 Rust `str`; arbitrary binary data and non-UTF-8 source
  decoding are outside the crate.
- Application produces contiguous UTF-8 in a new or caller-owned `String` or
  `Vec<u8>`; it is not a persistent rope and does not optimize a sequence of
  interactive editor keystrokes.
- Multi-file plans are validated here, but applying them transactionally is a
  worktree-layer responsibility.
- `PreparedEdits::write_to` validates the complete edit set before the first
  write, but an I/O failure can leave a non-transactional sink with a prefix;
  filesystem callers need a temporary file plus atomic rename. The method does
  not call `flush` or request durable storage synchronization.
- `sha256` is shape-checked but not calculated or compared with a file here.
- `completeness: "PARTIAL"` is valid; deciding whether incomplete evidence is
  acceptable belongs to the caller.
- Extension fields are preserved as JSON values but have no core semantics.
  `DeclaredEditPlan` drops them on decode; that is lossy by design, and a plan
  recovered through it must not be re-serialized as if it were the original.
- Provenance labels prove which planner supplied a range; this crate verifies
  the range and exact source text, not the planner's semantic reasoning.

## Development

```text
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo package --locked
cargo bench --locked --bench edit_engine
```

CI runs the tests on Linux, Windows, and macOS, repeats them on Rust 1.88, and
builds documentation and the publishable package with warnings denied. A
separate Linux gate requires at least 95% line coverage.

## License

MIT.
