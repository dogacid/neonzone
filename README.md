# neonzone

A first-person neon wireframe tank simulator for the terminal-shaped soul.
BattleZone's geometry and pacing, Tron's glow, recoloured live from whatever
Omarchy theme you happen to be running.

Switch themes with `Super + Ctrl + Shift + Space` and the game follows.

## Status

Early scaffold. It builds a world, draws it, and lets you drive around it.
Bloom, HUD and anything resembling a game are not there yet — see `CLAUDE.md`
for the gap list.

## Running

```
cargo run --release
```

`W`/`S` drive, `A`/`D` rotate, `Esc` quits. Arrow keys work too.

With no Omarchy install detected it falls back to a classic green-on-black
arcade palette, so it runs anywhere.

## Requirements

Arch (or anything with a Vulkan or GL driver), a Rust toolchain, and the usual
Wayland client headers. On the Quadro/Pascal path it will take Vulkan; wgpu
falls back to GL on its own if the driver is uncooperative.

## A note on API drift

This scaffold was written against `wgpu 25` and `winit 0.30`, and it has not
been compiled. Both crates churn their APIs between minor versions. If the first
`cargo build` throws errors, they will almost certainly be in these three
places, and all three are mechanical fixes:

- `wgpu::Instance::new` — took an owned descriptor before 25, a reference after.
- `adapter.request_device` — the trailing trace-path argument moved into
  `DeviceDescriptor` as the `trace` field.
- `RenderPipelineDescriptor` — `cache` and `multiview` have appeared and moved
  around across versions.

Pinning to the exact versions in `Cargo.toml` is the fastest path. Everything
in the shaders is version-independent.

## Layout

See `CLAUDE.md` for the module map, the frame ordering contract, and the
invariants that are easy to break by accident.

## Licence

MIT.
