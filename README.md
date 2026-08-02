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
- bounded source, edit-count, output-size, and multi-file plan validation;
- fallible reusable line indexing with explicit line-count and index-byte
  ceilings;
- bounded incremental admission of original-source byte edits with atomic
  `push_batch` rollback;
- portable repository-relative path validation;
- prepared edit union, source-offset invalidation, and forward offset mapping;
- allocation-free normalized change inspection with exact source/output ranges,
  retained provenance, and aggregate byte statistics;
- bounded multi-error diagnostics with structured, truncated mismatch evidence;
- zero-copy output chunks and a caller-owned writer adapter without allocating
  a final `String`;
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
| `PreparedEdits` | Apply, inspect normalized changes and statistics, stream to `Write`, union plans, and map offsets |
| `PreparedChange`, `ChangeSummary` | Exact source/output preview ranges, before/after slices, provenance, and aggregate byte totals |
| `EditChunks` | Sink-independent zero-copy iteration for sync or async consumers |
| `WriteSummary` | Successful streamed-output counts without retaining the complete result |
| `LineIndex`, `LineIndexLimits` | Strict UTF-8/UTF-16/UTF-32 position conversion, with a fallible bounded constructor |
| `EditPlan`, `FileEdit` | Extensible multi-file `weavatrix.edit-plan.v1` wire model |
| `validate_edit_plan` | Validate schema, paths, provenance, hashes, uniqueness, and budgets |
| `PlanLimits`, `ApplyLimits`, `BatchLimits` | Explicit resource ceilings with conservative defaults |
| `diagnose_edits`, `diagnose_byte_edits` | Non-mutating multi-error preflight with bounded retained diagnostics |
| `DiagnosticLimits`, `ValidationReport` | Hard item/preview ceilings and structured exact-before mismatch evidence |
| `EditError`, `ErrorCode` | Stable machine-readable failures plus file/edit indices |

`PreparedEdits::bytes_before` and `bytes_after` expose exact preflight sizes so
a transaction layer can enforce an aggregate multi-file budget before writing.

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

No public competitor performance result is claimed for version 0.1.2. The
repository contains a native smoke benchmark and a separate output-equivalent
byte-edit harness for Mago, rust-analyzer, and typst-edit. A ranking will be
published only after the harness, environment, and raw samples are recorded and
the compared operations are labelled by their actual validation contract.

The required method and benchmark matrix are in
[docs/benchmarks.md](docs/benchmarks.md).

## Limitations

- Input is valid UTF-8 Rust `str`; arbitrary binary data and non-UTF-8 source
  decoding are outside the crate.
- Application produces a contiguous `String`; it is not a persistent rope and
  does not optimize a sequence of interactive editor keystrokes.
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
