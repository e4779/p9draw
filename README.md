# p9draw-protocol

Pure-Rust codec (encode/decode) for the plan9port devdraw wire protocol
(drawfcall / `Wsysmsg`): frames `size[4 BE] tag[1] type[1] payload`, all 33
message types per `SPEC.md` §4, big-endian primitives, `n[4]+bytes` strings.
Status v0.1, protocol-only: no IO, no external crates — the inner draw
stream (`draw.rs`) and the server live in later crates (ARCHITECTURE.md).

Test: `cargo test -p p9draw-protocol` — offline unit tests: golden bytes
for key frames plus encode→decode roundtrips for all variants.
