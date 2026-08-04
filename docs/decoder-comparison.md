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
unknown members decodes a 500-file unicode plan in 300 µs, while the flatten
model that materializes every extension member takes 1,284 µs. The gap is not
`flatten`'s machinery so much as building roughly one `BTreeMap` plus its
`Value` tree per file and per edit, for members that most consumers never read.

This is the live optimization target for multi-file plans: capture extensions
lazily instead of eagerly, so callers that only need declared fields do not pay
for the rest. The wire contract in `tests/envelope_wire.rs` pins the observable
behavior any such change must preserve.

## Reproducing

`tests/decode_bench.rs` gives a coarse in-crate signal only. Conclusions
require the standalone matrix described above: a fair model whose extension
value type is generic over each decoder's own `Value`, a build with the
consumer's optimization profile, IQR reporting so unresolvable cells are
visible, and CPU pinning.
