# Competitive evidence

Snapshot date: 2026-08-02.

`weavatrix-edit` combines two adjacent Rust problem areas: safe batch source
editing and strict LSP coordinate conversion. No comparison below treats a rope,
editor buffer, language server, or syntax-specific rewriter as though it had the
same complete contract.

## Sources and versions

The capability review uses public registry pages and immutable source revisions:

- [Mago Text Edit 1.45.0](https://docs.rs/crate/mago-text-edit/1.45.0),
  [reviewed source](https://github.com/carthage-software/mago/blob/8771af7d3cdac8d2934e8fd187563d3c0464ac9a/crates/text-edit/src/lib.rs)
- [rust-analyzer `text_edit.rs` at `804ee7d`](https://github.com/rust-lang/rust-analyzer/blob/804ee7d794d4677ba17835653a6b032504839466/crates/ide-db/src/text_edit.rs), and the distinct
  [published `ra_ap_text_edit` 0.0.241 snapshot](https://docs.rs/crate/ra_ap_text_edit/0.0.241)
- [typst-edit 0.1.0](https://docs.rs/crate/typst-edit/0.1.0),
  [reviewed source](https://docs.rs/typst-edit/0.1.0/src/typst_edit/lib.rs.html)
- [lsp-textdocument 0.5.0](https://docs.rs/crate/lsp-textdocument/0.5.0),
  [`FullTextDocument` documentation](https://docs.rs/lsp-textdocument/0.5.0/lsp_textdocument/struct.FullTextDocument.html), and
  [reviewed source](https://docs.rs/lsp-textdocument/0.5.0/src/lsp_textdocument/text_document.rs.html)
- [textum 0.4.0](https://docs.rs/textum/0.4.0/textum/), including the
  [`PatchSet` implementation](https://docs.rs/textum/0.4.0/src/textum/composer.rs.html) and
  [`Patch::apply` implementation](https://docs.rs/textum/0.4.0/src/textum/patch.rs.html)
- [Ropey 1.6.1](https://docs.rs/ropey/1.6.1/ropey/) and
  [ee-xi-rope 0.8.2](https://docs.rs/ee-xi-rope/0.8.2/xi_rope/) as
  persistent editor-buffer references rather than direct plan-engine competitors

Versions are pinned because these APIs can change. The statements here should
be rechecked before publishing a later comparison.

## Capability matrix

| Capability | Weavatrix Edit 0.1.0 | Mago Text Edit 1.45.0 | rust-analyzer `804ee7d` / `ra_ap` 0.0.241 | typst-edit 0.1.0 | lsp-textdocument 0.5.0 |
| --- | --- | --- | --- | --- | --- |
| Primary boundary | Untrusted, evidence-backed source plans | Fast byte-buffer edit accumulation | Trusted internal IDE edits | Typst-aware source rewriting | LSP document state and coordinate mapping |
| Native edit coordinates | Strict UTF-8/UTF-16/UTF-32 line/character plus byte API | `u32` byte ranges | `u32` byte ranges | `usize` byte ranges | UTF-8/UTF-16/UTF-32 LSP positions |
| Invalid coordinate policy | Typed error; no clamping | Bounds result; byte buffer permits arbitrary bytes | Builder assertions and `String::replace_range` assumptions | Typed bounds/char-boundary error | Some conversions clamp or round invalid positions |
| Whole-set overlap validation | Yes | Yes, including atomic batch admission | Builder assertion; `union` returns conflict | Yes | Not its primary contract; updates are document changes |
| Stable same-offset insertions | Yes | Yes | Yes | Input order after stable range sort | Sequential document-change semantics |
| Exact original `before` proof | Yes | No | No | No | No |
| Applicable evidence label | Provenance allowlist | Safety threshold | Current upstream: change annotation; `ra_ap` 0.0.241: none | No | No |
| Final caller validator | Yes | Yes | No | No | No |
| Source binding and replay | Source-bound; non-consuming replay | Source-bound; `finish` consumes, while `Clone` enables cloned replay | Reusable structural plan; not source-bound | No | Mutable document state instead |
| Prepared-plan union | Yes | No independent-plan union | Yes | No | No |
| Forward offset mapping | Yes, with boundary bias | No dedicated API | Yes | No | Position/offset conversion, not edit projection |
| Bounded original-source admission | Yes | Incremental and batch admission, but no hard budgets | Builder stages trusted edits without source validation | No | No |
| Sequential current-document session | No; separate future layer | No; every offset addresses the original source | No | No | Yes; later changes address already-modified text |
| Streaming result | Borrowed chunks and caller-owned `Write` | No | No | No | No |
| Arbitrary binary source | No; strict UTF-8 source | Yes | No | No | No |
| Hard source/edit/output budgets | Yes | No | No | No | No |
| Rejection diagnostics | File/edit/related-edit indices | Result category only; no edit index | Conflict returned or assertion; no edit index | Conflicting ranges/offset; no edit index | Not a plan-validation result |
| Versioned multi-file JSON plan | Extensible v1 envelope and schema | Optional per-type Serde, no equivalent envelope | No | Optional serialization, no equivalent envelope | LSP document types, no equivalent envelope |
| Filesystem transaction | No | No | No | No | No |

"No" means the reviewed public surface does not provide that contract. It does
not mean a consumer cannot build it around the project.

## Closest direct engine: Mago Text Edit

Mago is the strongest direct performance-oriented comparator. Its editor:

- accepts byte-buffer edits;
- validates bounds, overlaps, and safety thresholds;
- admits a batch atomically;
- preserves same-position insertion order;
- can run a checker on the simulated complete result;
- calculates final capacity and stitches in one pass.

That is a strong baseline for `weavatrix-edit`'s prepared byte-edit path. Mago's
use of arbitrary bytes is also the right design for its PHP tooling. The
different Weavatrix requirement is strict UTF-8 source plus v1 UTF-16/LSP
coordinates, exact `before` proof, detailed error location, prepared union and
offset projection, and a multi-file evidence envelope.

Neither project should be described as universally faster without an
output-equivalent benchmark.

## Two meanings of streaming

Streaming output and edits arriving over time have different correctness
contracts:

- `PreparedEdits::chunks` and `PreparedEdits::write_to` emit a fully validated
  original-source batch without allocating a final result string. Validation
  finishes before the first chunk, although an external writer error can leave
  a non-transactional sink with a valid prefix.
- A mutable editing session applies each new change to a new document revision.
  It needs revision identifiers, current/base coordinate semantics, transform
  or rebase rules, undo, and a conflict policy. `lsp-textdocument` is an example
  of sequential current-document changes, not atomic refactoring-plan apply.

Calling both APIs "stream edit" would hide that distinction. The first belongs
in this crate; the second belongs in a future revision/session component.

## Mature edit algebra: rust-analyzer

rust-analyzer's internal `TextEdit` is the strongest composition reference. It
offers a builder, sorted disjoint edits, adjacent edit coalescing, union,
same-position insertion handling, invalidated-offset checks, and
`apply_to_offset`.

The published `ra_ap_text_edit` 0.0.241 used by the benchmark has this core edit
algebra. The reviewed current upstream revision additionally carries change
annotations; the published benchmark snapshot does not.

It is intentionally an internal trusted-range abstraction. The reviewed builder
asserts disjointness, and application delegates to `String::replace_range`,
which expects valid bounds. That is suitable inside rust-analyzer's controlled
pipeline but is not a fail-closed parser for untrusted JSON plans. It also does
not carry `before`, hashes, provenance, LSP coordinates, or a multi-file wire
schema.

`weavatrix-edit` should retain rust-analyzer's useful composition semantics
while returning typed errors at the untrusted boundary.

## Strict byte validation: typst-edit

typst-edit validates all edit ranges before constructing output and returns
specific overlap, out-of-bounds, and non-character-boundary errors. It also
provides Typst syntax-aware call and link locators that are intentionally
outside a general edit engine.

Its apply core is a good correctness comparator for byte-range edits. Its
Typst-specific parsing, absence of strict UTF-16 mapping, and lack of `before`,
provenance, hash, composition, and multi-file plan contracts make it a partial
rather than direct substitute.

## Coordinate reference: lsp-textdocument

lsp-textdocument supports all LSP 3.17 position encodings and manages live
document updates. It is an important interoperability reference, not a strict
plan validator. The reviewed implementation deliberately clamps positions past
line or file ends and rounds some non-boundary offsets; its update path also has
assertive invalid-range behavior.

Weavatrix chooses the opposite policy for refactoring plans: an invalid or stale
coordinate fails the whole prepared set. This prevents a malformed position
from silently editing a nearby valid location.

## Adjacent patch engine: textum

textum combines snippet targeting, Rope storage, and multi-file `PatchSet`
operations, so it is useful product research but not an equivalent low-level
batch benchmark. Its direct range primitive applies one edit at a time; its
batch path includes snippet resolution and filesystem reads.

The reviewed `PatchSet` resolves ranges for preflight and then each `Patch`
resolves its snippet again while applying to an already-changing Rope. Its
overlap rejection is also limited to pairs whose replacements are both
non-empty. Finally, `write_to_files` writes materialized results one file at a
time and explicitly permits partial filesystem output after an I/O failure.
Weavatrix keeps targeting/planning and transactional worktree mutation outside
the edit splice core so those states remain explicit.

## Ropes and editor buffers

Ropey, xi-rope, and similar libraries optimize persistent large-document
storage and repeated interactive changes. `weavatrix-edit` accepts an immutable
`str`; it can emit borrowed result chunks or stream them to a writer, but it
does not retain a mutable document revision graph. A future rope adapter could
translate validated byte edits, while persistent storage remains outside the
core wire and validation model.

## Remaining boundaries

The initial crate does not yet claim:

- a universal performance win over Mago, rust-analyzer, typst-edit, or ropes;
- arbitrary binary-source support;
- persistent incremental editing;
- filesystem atomicity or rollback;
- semantic correctness of a planner's provenance claim;
- LSP `WorkspaceEdit` conversion;
- cross-file application atomicity.

These are explicit boundaries, not silent fallbacks.

## Benchmark requirement

Performance claims must follow [the checked-in methodology](benchmarks.md).
In particular, a UTF-16 plan plus exact-before validation cannot be timed
against a raw byte splice and labelled equivalent. Coordinate mapping,
validation, and apply-only workloads must be reported separately.
