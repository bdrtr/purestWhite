# Hypher Binary Tries

CrossPoint embeds the exact binary automata produced by
[Typst's `hypher`](https://github.com/typst/hypher). 

## File layout

Each `.bin` blob is a single self-contained automaton:

```
uint32_t root_addr_be;  // big-endian offset of the root node
uint8_t  levels[];      // shared "levels" tape (dist/score pairs)
uint8_t  nodes[];       // node records packed back-to-back
```

The size of the `levels` tape is implicit. Individual nodes reference slices
inside that tape via 12-bit offsets, so no additional pointers are required.

### Node encoding

Every node starts with a single control byte:

- Bit 7 – set when the node stores scores (`levels`).
- Bits 5-6 – stride of the target deltas (1, 2, or 3 bytes, big-endian).
- Bits 0-4 – transition count (values ≥ 31 spill into an extra byte).

If the `levels` flag is set, two more bytes follow. Together they encode a
12-bit offset into the global `levels` tape and a 4-bit length. Each byte in the
levels tape packs a distance/score pair as `dist * 10 + score`, where `dist`
counts how many UTF-8 bytes we advanced since the previous digit.

After the optional levels header come the transition labels (one byte per edge)
followed by the signed target deltas. Targets are stored as relative offsets
from the current node address. Deltas up to ±128 fit in a single byte, larger
distances grow to 2 or 3 bytes. The runtime walks the transitions with a simple
linear scan and materializes the absolute address by adding the decoded delta
to the current node’s base.

## Embedding blobs into the firmware

The `.bin` files live unchanged in `rust/crates/bembeyaz-hyphenation/tries/`,
and the Rust crate embeds them with `include_bytes!`, so the automata stay in
flash. Node addresses in `liang.rs` count from the byte after `root_addr_be`;
levels offsets count from the start of the file.

The files in the repository are the ones the C++ hyphenator embedded until
46ebf6e. On 2026-10-19 `en` and `de` matched hypher's main branch byte for
byte and `fi` did not: hypher has updated some tries since.

The script `update_hyphenation.sh` downloads the current tries for all
languages. A new trie changes where words break, so check
`test/hyphenation_eval` after an update and re-record `HyphenatorCorpusTest`:
set every `kHashes` entry to 0, run the test, paste the hashes it prints, and
take the language-tag test's new hash from its failure message. The hashes then
pin the new tries instead of the C++ code.
To use it, run:

```sh
./scripts/update_hyphenation.sh
```
