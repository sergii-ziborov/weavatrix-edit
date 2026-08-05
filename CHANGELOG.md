# Changelog

## 0.1.7 — 2026-08-05

- Corrected the `DeclaredEditPlan` documentation. 0.1.6 claimed it accepts and
  rejects exactly the documents `EditPlan` does; that is false for content
  inside undeclared members. Capturing decode materializes each undeclared
  member into a `Value` and fails on anything the value model cannot represent
  (out-of-range numbers, lone surrogate escapes, nesting past the driver's
  depth limit), while the declared-only path consumes that member without
  inspecting it and accepts the document. Declared members are unaffected.
  A declared-only decode is therefore safe for a terminal consumer of declared
  data and unsafe as an input-rejection gate. `tests/declared_divergence.rs`
  pins the divergence so it cannot be re-asserted away.
- No code change; behavior is identical to 0.1.6.

## 0.1.6 — 2026-08-05

- Added `DeclaredEditPlan`, an additive decode path that recovers an `EditPlan`
  without materializing any extension member. It yields identical declared data
  with empty extension maps, and reports declared-member errors identically.
  Dropping extensions is lossy by design: a plan recovered through it must not
  be re-serialized as if it were the original. See 0.1.7 for the corrected
  statement of how the two decodes differ on undeclared content.
- Replaced the derived `#[serde(flatten)]` codecs on `TextEdit`, `FileEdit`,
  and `EditPlan` with hand-written ones. Serialized bytes, accepted documents,
  and duplicate/missing/invalid-type messages are unchanged and pinned against
  an independent restatement of the original derives on two serde drivers.
- Tied the reserved extension-key lists to the wire field names themselves, so
  a future member rename cannot silently open a shadowing collision.
- Measured, in one process against the published 0.1.5 tree: decoding a plan
  that carries extensions through `DeclaredEditPlan` is 1.80x-2.17x faster on
  both decoders under both build profiles. Plans without undeclared members are
  unchanged in either direction.
- No public API was removed or changed.

## 0.1.2 — 2026-08-02

- Added allocation-reusing `PreparedEdits::apply_into` and
  `PreparedEdits::apply_into_bytes` output paths with exact application totals.
- Added an allocation-free sorted preflight and no-intermediate-candidate apply
  path while preserving exact-before, Unicode, overlap, limit, and
  error-precedence gates.
- Added explicit opt-in `rendered_text` replay caching with observable and
  releasable output-sized memory; ordinary application remains uncached.
- Added bulk equal-byte-length patching and stable same-offset execution-run
  coalescing with one hard 64 KiB retained-text ceiling per prepared plan.
- Fused native byte structural admission with deferred exact-before checking
  while retaining deterministic failure precedence.
- Reduced common prepared metadata size with lazily allocated merged provenance.
- Expanded the pinned competitor harness with correctness-gated caller-buffer,
  reversed-input, default-limit, and custom-limit stress workloads.
- Recorded clean-commit competitor evidence: the conservative default-ceiling
  caller-String gate passed at 2.24x versus the fastest competitor tail.
- Corrected installation instructions to use the published crates.io package.
- Added crates.io and docs.rs release links and badges.
- Added a secret-backed, manually dispatched GitHub publication workflow.

## 0.1.1 — 2026-08-02

- Added fallible full line indexing with explicit line and byte ceilings.
- Changed one-shot position resolution to use an `O(edits)` sparse index.
- Added bounded multi-error diagnostics with structured mismatch evidence.
- Added allocation-free normalized change previews and exact byte statistics.
- Preserved provenance labels and original order across prepared-plan unions.
- Removed complete source text from exact-before error messages.

## 0.1.0 — 2026-08-01

- Initial public release.
