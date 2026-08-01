# `weavatrix.edit-plan.v1`

Status: frozen core contract for Weavatrix Refactor.

This document defines the serialized plan model accepted by `weavatrix-edit`.
The [JSON Schema](schema/weavatrix.edit-plan.v1.schema.json) describes its wire
shape. The Rust validator remains authoritative for invariants that JSON Schema
2020-12 cannot express.

## Envelope

```json
{
  "schemaVersion": "weavatrix.edit-plan.v1",
  "operation": "rename_symbol",
  "completeness": "COMPLETE",
  "files": [
    {
      "path": "src/user.ts",
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "edits": [
        {
          "startLine": 10,
          "startChar": 8,
          "endLine": 10,
          "endChar": 15,
          "before": "getUser",
          "after": "getCustomer",
          "provenance": "EXACT_LSP"
        }
      ]
    }
  ],
  "createdAt": "2026-08-01T12:00:00Z"
}
```

`createdAt` is an example extension. Unknown fields are retained at the plan,
file, and edit levels and serialized again. Extensions must not shadow a core
field.

## Core fields

### Plan

| Field | Required | Contract |
| --- | :---: | --- |
| `schemaVersion` | yes | Exactly `weavatrix.edit-plan.v1` |
| `operation` | yes | Non-empty producer-defined operation name |
| `files` | yes | Non-empty array of unique file entries |
| `completeness` | no | `COMPLETE` or `PARTIAL` |

### File entry

| Field | Required | Contract |
| --- | :---: | --- |
| `path` | yes | Portable repository-relative path using `/` |
| `sha256` | yes | 64 lowercase hexadecimal characters |
| `edits` | yes | Non-empty array of edits over the hashed source |

The library validates only the SHA-256 representation. A filesystem transaction
consumer must calculate the current content hash and compare it immediately
before applying the file's edits.

### Edit entry

| Field | Required | Contract |
| --- | :---: | --- |
| `startLine` | yes | 1-based line number |
| `startChar` | yes | 0-based UTF-16 code-unit position |
| `endLine` | yes | 1-based line number |
| `endChar` | yes | 0-based UTF-16 code-unit position |
| `before` | yes | Exact original text in the half-open range |
| `after` | yes | Replacement text; must differ from `before` |
| `provenance` | yes | One of the applicable evidence labels below |

## Position rules

1. A range is half-open: its start is included and its end is excluded.
2. Lines are counted from one; characters are counted from zero.
3. Character values count UTF-16 code units, matching the Weavatrix v1 and LSP
   compatibility surface.
4. `\n` terminates a line and is not addressable from the preceding line.
5. A `\r` immediately before `\n` remains part of the preceding line for v1
   compatibility.
6. A trailing `\n` creates a final empty line whose position `0` is valid.
7. A position beyond a line or file is an error; it is never clamped.
8. A position that splits an astral Unicode scalar's UTF-16 surrogate pair is
   an error.
9. Resolved ranges must begin and end on UTF-8 scalar boundaries.
10. The end position must not precede the start position.

All edit positions in one file refer to the same original source. Edits are not
interpreted sequentially.

## Applicable provenance

| Value | Meaning |
| --- | --- |
| `EXACT_LSP` | Exact range supplied by a language-server operation |
| `RESOLVED` | Range resolved to a canonical symbol or reference |
| `EXTRACTED` | Exact declaration or structural range owned by an extractor |
| `LEXICAL_EXACT` | Exact lexical match whose `before` text is reverified |

Inferred, conflicting, or otherwise unproven findings must not appear as edits.
They can be carried in plan extensions such as `uncertainReferences`.

The engine does not independently prove the semantic claim behind a provenance
label. It verifies that the label is applicable, the range is valid, and the
source slice exactly matches `before`.

## Path rules

Paths must:

- be non-empty and repository-relative;
- use forward slashes;
- contain no drive prefix, colon, backslash, control character, empty segment,
  `.` segment, or `..` segment;
- contain no case-insensitive `.git` segment;
- contain no segment ending in a dot or space;
- avoid Windows device names including `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`,
  `CONOUT$`, `COM1` through `COM9`, and `LPT1` through `LPT9`.

Within one plan, exact duplicate paths and conservative Windows-portable aliases
are rejected. These lexical checks do not replace real-path containment and
link checks in the filesystem transaction layer.

## Default budgets

| Budget | Default |
| --- | ---: |
| Files | 500 |
| Edits per file | 2,000 |
| Total edits | 1,000,000 |
| UTF-8 path bytes | 4,096 |
| Combined `before` and `after` bytes | 64 MiB |

Consumers can choose stricter [`PlanLimits`](https://docs.rs/weavatrix-edit/latest/weavatrix_edit/struct.PlanLimits.html).

## Application invariants

Before producing output for one source, the engine validates every edit,
resolves every position, compares every `before` slice, checks the complete set
for overlap, and calculates the output size. Failure returns one typed error and
no partial output.

Empty edits at the same position are allowed and preserve plan-array order.
An insertion at the boundary of a replacement is allowed. An insertion strictly
inside a non-empty replaced range conflicts with that replacement.

## Error contract

`EditError::code()` returns one of these stable categories:

- `SCHEMA_MISMATCH`
- `INVALID_PLAN`
- `INVALID_FILE`
- `INVALID_EDIT`
- `INVALID_PATH`
- `UNPROVEN_EDIT`
- `PLAN_TOO_LARGE`
- `POSITION_OUT_OF_RANGE`
- `BEFORE_MISMATCH`
- `OVERLAPPING_EDITS`
- `OUTPUT_TOO_LARGE`
- `VALIDATION_REJECTED`

When applicable, `file_index`, `edit_index`, and `related_edit_index` identify
the exact rejected entries. Consumers should branch on the code rather than the
human-readable message.

## Versioning

The v1 core fields and position convention are frozen. Additive producer
metadata belongs in extension fields. Any incompatible field meaning, position
convention, or validation rule that changes which existing core plans are
accepted requires a new `schemaVersion` and an explicit compatibility path.
