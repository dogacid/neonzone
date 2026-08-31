# neonzone — notes for Claude Code

A first-person neon wireframe tank simulator. BattleZone's geometry and pacing,
Tron's palette and glow, recoloured live from the machine's Omarchy theme.

## North star

Everything visible is a line. No textures, no meshes, no lighting model. If a
change would be easier with a filled polygon, it is the wrong change.

Two colour roles only: `palette.primary` for terrain, obstacles and own HUD,
`palette.hostile` for anything that wants shooting. This is a gameplay contract
as much as an aesthetic one — a player must be able to tell friend from threat
at a glance, in any theme.

## Layout

```
src/
  main.rs            winit ApplicationHandler, GPU init, frame order
  camera.rs          tank camera: yaw + drive only, deliberately not free-fly
  world.rs           rebuilds the whole line batch every frame
  theme.rs           Omarchy colors.toml -> OKLCH neon palette, + file watcher
  render/
    line.rs          instanced screen-space line quads
    line_glow.wgsl   the core/halo/dwell fragment shader
    post.rs          phosphor accumulation + tonemap
    post.wgsl
```

## Frame order — do not reorder

1. `post.decay_pass` — last frame at reduced energy into `accum[next]`
2. line pass — this frame's segments, additively, same attachment
3. `post.tonemap_pass` — to the swapchain
4. `post.swap`

Bloom, when it lands, goes between 2 and 3. It must read the *accumulated*
buffer, not the raw frame, or the trails will not bloom and the smear will read
as a separate effect pasted on top.

## Invariants worth protecting

- The line vertex shader emits `w = 1.0` on purpose. The quad is built in pixel
  space, so its varyings must interpolate linearly across the screen. Real
  clip-space `w` reintroduces perspective-correct interpolation and warps line
  thickness toward the vanishing point.
- The near-plane clip in `vs_main` is load-bearing. Without it, any segment
  straddling the camera plane whips across the whole screen.
- There is no depth buffer anywhere, by design. Pure additive blending, so
  overlapping strokes reinforce — which is how a vector monitor behaved. If
  hidden-line removal is ever wanted, do it as a CPU visibility pass over the
  segment list, not with depth.
- `world.rs` regenerates everything each frame. At a few thousand segments this
  is cheaper than managing a scene graph. Do not add one.
- `theme.rs` parses `colors.toml` permissively — it flattens the document and
  matches on key names with a chroma-ranked fallback. Omarchy has changed this
  schema before. Keep it tolerant; a renamed key should mean "different
  colours", never a crash.

## Known gaps

- Bloom is not implemented. `post.rs` has the seam marked.
- Theme changes snap rather than cross-fading.
- No HUD: no radar, no crosshair, no score.
- No shooting, no collision, no enemy AI.
- Distance fade is linear in view depth; may want a curve.

## Conventions

- `cargo fmt` before committing. No clippy allowances without a comment saying why.
- Comments explain *why*, never *what*. The code already says what.
- Tuning constants live next to their use with a note on what moving them does.
