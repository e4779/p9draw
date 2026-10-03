# p9draw — draw-protocol server for plan9port acme

A Rust implementation of the plan9port **devdraw** wire protocol: the classic
acme text editor (1993, C) draws into windows served by this server — on X11
and as a **native Wayland client**.

## Status — brick 1 complete

Verified live (2026-10-03/04): plan9port acme runs fully against this server —
windows, columns, tags with antialiased text, all three mouse buttons, drags,
execution (+Errors), resizes. Verified end-to-end including the per-window
H.264 stream of the [Greenfield-style browser compositor](https://github.com/udevbe/greenfield)
(decoded the actual stream: 1817 dark text pixels).

Current gaps (roadmap): HiDPI font sizes (fontsrv wired, size tuning pending),
screen-size propagation on first layout, wgpu backend horizon.

## Layout

| path | what |
|---|---|
| `crates/protocol` | drawfcall wire codec (33 wsysmsg types + inner draw commands, incl. plan9 compressed images `'Y'`) |
| `crates/render` | software raster: images, chans, fill/compose/tile, grey masks |
| `crates/host` | winit 0.30 + softbuffer 0.4 window host (X11 + native Wayland) |
| `crates/server` | the serve binary: wire dispatch, image store, screen composite, stats/trace/present-debug |
| `docs/SPEC.md` | the wire protocol specification (verified against real acme captures) |
| `docs/ARCHITECTURE.md` | crate map and data flow |
| `docs/DEBUG-NOTES.md` | the dark-window investigation record (evidence file) |
| `docs/*.md, docs/_sources/` | the OKF knowledge bundle: acme lineage research (ad, edward, wily, fleet, Greenfield, waypipe, ...) and the project diary (roadmap-devdraw.md) |

## Run

```sh
cargo test                      # 152+ tests, no display needed
cargo run -p p9draw-server -- serve   # with a Wayland/X display
```

### Live acme against this server (native Wayland, container)

See `scripts/launch-wayland-serve.sh` and the README section on the
Wayland-native serve window. The client-side acme needs `PLAN9` pointing at
the plan9port tree and `P9DRAW_SERVE=1 P9DRAW_SERVER_BIN=<p9draw-server>`
(the bin/devdraw wrapper routes it to us).

### E2E harness (synthetic client, spawns its own serve)

```sh
cargo build --release -p p9draw-server --examples
DISPLAY=:0 P9DRAW_SERVE=1 P9DRAW_SERVER_BIN=$PWD/target/release/p9draw-server \
  target/release/examples/e2e
```

Ten steps: init/alloc/fill/text/readback/mouse/window composite — PASSED/FAILED per step.

## Deploy checklist (containers)

1. `touch crates/*/src/*.rs` before release builds — cargo mtime cache has
   thrice served stale binaries after agent edits (verify: `strings bin | grep -c "serve stats"`).
2. Restart the RUNNING serve process after deploy — a replaced file does not
   restart anything (the "dark window" root cause).
3. `unset DISPLAY` for native Wayland (a leaked DISPLAY silently selects X11).
4. wayvnc/noVNC stack: see the tiling-browser notes in docs/.

## Docs map

Engineering docs live in this repo (`docs/`). The research knowledge base
(the acme lineage: ad, edward, wily, fleet, Greenfield, waypipe, the three
delivery schools) and the full project diary are in the companion OKF bundle
(`docs/` — concepts + `roadmap-devdraw.md`), maintained with the
[Open Knowledge Format](https://github.com/e4779/okf) tooling.
