# Decoder and envelope decode costs

This records what actually drives decode time for plan-sized `EditPlan`
payloads, and corrects two earlier claims that were made from a confounded
measurement.

## Corrected record

An earlier measurement, taken with `tests/decode_bench.rs`, was reported as:
`serde_json` and `blazingly-json` are within noise, and replacing the envelope
`#[serde(flatten)]` with hand-written codecs does not help. **Both statements
were unsupported.** Three confounds produced them:

1. The two "different" models compared were the shipped derives and
   `tests/flatten_reference`, which restates the same derives. That ratio is a
   tautology.
2. The shipped model hard-codes `blazingly_json::Value` as its extension value
   type, so the `serde_json` arm was forced to construct another crate's
   `Value`. That is workload, not decoder speed.
3. `#[serde(flatten)]` routes every driver through `deserialize_map` plus
   `Content` buffering and a second `FlatMapDeserializer` pass. This additive
   constant is driver-independent and mathematically compresses any ratio
   toward 1.0. The test also builds without cross-crate LTO, unlike the
   profile a production consumer ships.

## What a fair measurement shows

Matrix over two decoders, four models, four corpora, and two build profiles.
Correctness gated before timing; 31 samples per cell with alternating arm
order, adaptive inner loops, medians with p25/p75, run pinned to one logical
processor at high priority. Ratios below are `serde_json / blazingly-json`, so
above 1.0 means blazingly-json is faster.

Production profile (fat LTO, codegen-units = 1):

| Corpus | Plain struct | Borrowed `Cow` | Flatten |
| --- | ---: | ---: | ---: |
| ASCII, 500 files | 1.435x | 1.676x | 1.374x |
| Unicode + extensions, 500 files | 1.258x | 1.475x | 1.048x |
| Single 1 KiB message | 1.316x | 1.447x | 1.159x |
| Escape-heavy | 1.126x | 1.117x | 1.100x |

Stock release (no cross-crate LTO) narrows this to roughly 1.03x-1.19x, and on
the escape-heavy corpus the arms overlap, so no winner is resolvable there.
`serde_json` leads only in the raw borrowed-envelope rows, which compare
`RawJson` against `&RawValue` — different APIs, labelled as not comparable.

This is consistent with blazingly-json's own published numbers: its headline
7.35x-7.62x measures a hand-written canonical scanner against serde typed
deserialization, a different API pair, while its serde path is documented at
+1.70% to +12.13%.

## The dominant cost is materializing extensions

Same corpus, same decoder, production profile: a plain struct that drops
unknown members decodes a 500-file unicode plan in about 300 µs, while a
flatten model that materializes every extension member takes several times
that. The gap is dominated by building roughly one `BTreeMap` plus its `Value`
tree per file and per edit, for members that most consumers never read.

One correction to how that gap was first stated. The "1,284 µs" figure quoted
earlier was the `typed-flatten-native` row — a harness-local generic model —
not the shipped envelope, which measured about 680 µs in the same run. The
conclusion (extensions dominate) survives; the specific pair of numbers was
mismatched, and the ratio was overstated.

## What changed in 0.1.6

Two things, and only one of them mattered.

1. `TextEdit`, `FileEdit`, and `EditPlan` now implement serde by hand instead
   of deriving it with `#[serde(flatten)]`. Every member is read once, with no
   `Content` buffering and no `FlatMapDeserializer` second pass.
2. `DeclaredEditPlan` was added: the same envelope, the same acceptance and
   error messages, but undeclared members are skipped structurally through
   `IgnoredAny` instead of being materialized.

Change 1 alone is close to a wash. Change 2 is the win. The hand-written codec
is what makes change 2 expressible at all — `flatten` cannot say "decode this
envelope and skip the undeclared members" — and it lets both decode paths share
one implementation of field matching, duplicate detection, and missing-field
reporting, so they cannot drift apart.

### Measuring a before/after honestly on this machine

Cross-process comparison was not usable here. An unchanged control model moved
by up to 40% between two runs of the same matrix, because the pinned logical
processor shares a physical core with whatever else the scheduler puts there.

So the before/after is measured **inside one process**. The published 0.1.5
tree is linked a second time under the package name `weavatrix-edit-legacy`, so
both arms cross an identical crate boundary and differ only in the envelope
codec. The two arms are then interleaved sample by sample under the protocol
above: adaptive inner loop, at least 10 warmups, 31 recorded samples,
alternating arm order, decode plus recursive drop inside the timer, and a
correctness gate that requires both arms to re-encode to the same bytes before
either is timed. A row counts only when the two interquartile ranges are
disjoint.

Ratios below are `0.1.5 median / 0.1.6 median`, so above 1.00 means 0.1.6 is
faster. `res` marks a resolvable row; `-` marks overlapping IQRs, where the
honest reading is "no change measurable at this sample size".

Published 0.1.5 derive → 0.1.6 `DeclaredEditPlan` (the end-to-end result):

| Corpus | release, blazingly | release, serde_json | fatlto, blazingly | fatlto, serde_json |
| --- | --- | --- | --- | --- |
| Unicode + extensions, 500 files | 2.057x res | 1.993x res | 1.973x res | 1.797x res |
| Single 1 KiB message | 2.166x res | 2.048x res | 1.901x res | 1.831x res |
| ASCII, 500 files, no extensions | 1.052x - | 1.058x - | 1.017x - | 1.053x - |
| Escape-heavy, no extensions | 1.011x - | 0.978x - | 0.994x - | 0.989x - |

Published 0.1.5 derive → 0.1.6 capturing decode (extensions still retained):

| Corpus | release, blazingly | release, serde_json | fatlto, blazingly | fatlto, serde_json |
| --- | --- | --- | --- | --- |
| Unicode + extensions, 500 files | 1.109x res | 1.110x - | 1.180x res | 1.124x - |
| Single 1 KiB message | 1.217x res | 1.223x res | 1.208x res | 1.188x res |
| ASCII, 500 files, no extensions | 0.980x - | 1.024x - | 1.004x - | 1.013x - |
| Escape-heavy, no extensions | 0.985x - | 1.017x - | 1.005x - | 0.959x - |

Reading these together:

- On plans that actually carry extensions, dropping them is worth **1.80x to
  2.17x**, resolvable on both decoders under both profiles.
- Keeping them and only removing `flatten` is worth at most about 1.2x, and
  half those cells do not resolve. This is why the hand-written codec is
  justified by what it enables, not by its own speed.
- On corpora with no undeclared members, every single cell overlaps. The
  declared-only path is neither faster nor slower there, which is the expected
  result: there is nothing to skip.
- No row anywhere shows a resolvable regression.

Absolute medians for the headline row (Unicode + extensions, 500 files, fat
LTO, blazingly-json): 0.1.5 derive 708,611 ns [p25 647,416 / p75 776,778];
0.1.6 declared-only 359,221 ns [327,562 / 394,802].

## Reproducing

`tests/decode_bench.rs` gives a coarse in-crate signal only. Conclusions
require the standalone matrix described above: a fair model whose extension
value type is generic over each decoder's own `Value`, a build with the
consumer's optimization profile, IQR reporting so unresolvable cells are
visible, CPU pinning, and — for any before/after claim — both versions linked
into one process.
