# p9draw

Pure-Rust implementation of the plan9port devdraw wire protocol
(drawfcall / `Wsysmsg`, `SPEC.md`) and its observation tooling:

- `crates/protocol` — codec: all 33 message types, the inner little-endian
  draw stream, golden-byte tests. No IO, no dependencies.
- `crates/render` — memdraw-semantics raster (images, compose, fill) over
  byte planes.
- `crates/host` — real window (winit + softbuffer).
- `crates/server` — `serve` (devdraw replacement on the legacy pipe,
  SPEC.md §2.1: drawfcall in on stdin, replies on stdout, acme renders
  into a real window), plus `observe` / `capture` / `capture-pipe` and
  `scripts/devdraw-tee.sh`.

Test: `cargo test --workspace` — offline unit tests (golden bytes,
roundtrips, render semantics) plus fixture tests over live captures
(`docs/fixtures-analysis.md`).

## End-to-end test

`crates/server/examples/e2e.rs` is a synthetic drawfcall client speaking
the real wire protocol over the stdin/stdout of the `serve` subcommand —
the exact fd pair a plan9port client dup2()s its pipe onto. Steps, each
printed as PASSED / SKIP / FAILED (exit 0 unless a step FAILED):

1. `init` — `Tinit` → `Rinit`;
2. `alloc` — Twrdraw `'b'`: image 1, whole-screen rect, XRGB32, value =
   DPaleyellow `0xFFFFAAFF` (the image is born filled — allocimage fill
   semantics);
3. `fill` — Twrdraw `'d'` src=1 over image 0 across the whole screen plus
   `'v'` flush in one stream (count = 45+1 = 46, like the live capture);
4. `readback` — Twrdraw `'r'` + `Trddraw`: the screen bytes must equal the
   render-semantics expectation `[B,G,R,X]` = `AA FF FF 00` per pixel;
5. `mouse` — `Tbouncemouse` then `Trdmouse` → `Rrdmouse` within 5 s;
   **SKIP without an X display**, so the harness stays CI-safe.

Build and run (headless: every step honestly SKIPs, exit 0):

    cargo build --bins --examples
    cargo run --example e2e -p p9draw-server

The serve binary is searched in `target/{release,debug}`; point
`P9DRAW_SERVER_BIN` at it explicitly when it lives elsewhere.

### Full run in the acme-web container

The full PASSED path needs the container's X server (`DISPLAY=:1`). The
deploy (main agent) puts the binaries at `/config/plan9port/bin/`; copy
the freshly built example next to them and run it there:

    cargo build --release --bins --examples
    doas podman cp target/release/p9draw-server acme-web:/config/plan9port/bin/p9draw-server
    doas podman cp target/release/examples/e2e acme-web:/config/plan9port/bin/e2e
    doas podman exec acme-web bash -c 'chmod 755 /config/plan9port/bin/p9draw-server /config/plan9port/bin/e2e && \
      DISPLAY=:1 P9DRAW_SERVE=1 NAMESPACE=/tmp/ns.abc.serve \
      P9DRAW_SERVER_BIN=/config/plan9port/bin/p9draw-server \
      /config/plan9port/bin/e2e'

`P9DRAW_SERVE=1` and `NAMESPACE` mirror the live acme environment
(`devdraw-tee.sh` serve branch); the harness itself talks to `serve`
over stdio and needs neither socket nor namespace.

The example sets `P9DRAW_STATS=1` for the child, so its stderr ends with
the serve stats line — per-type frame counters (`type: count, bytes`),
printed every 30 s and once more at exit (`crates/server/src/stats.rs`).
