# Changelog

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
