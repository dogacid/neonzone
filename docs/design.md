# Design notes

## Why a custom renderer instead of an engine

BattleZone is not a 3D-engine problem. Its world is line lists — a few hundred
vertices, no textures, no lighting. What is actually needed is a renderer that
draws glowing lines well, and every general-purpose engine makes that harder,
because GPU line primitives are 1px, aliased and thickness-locked. The custom
pipeline gets written either way; starting from wgpu means writing only it.

## The two effects that sell the look

**Beam persistence.** Real vector monitors had phosphor decay. That is what
people register as "arcade" without knowing why. Implemented as the ping-pong
accumulation in `post.rs` — fast rotation smears, slow drift does not.

**Beam dwell.** The electron beam decelerated at each vertex, so corners burned
hotter than the middle of a stroke. Implemented in `line_glow.wgsl` as an
endpoint-proximity term. Nearly free, and it is most of what separates
"wireframe" from "vector monitor".

## The two fragment lobes

The fragment shader sums a tight Gaussian (`exp(-t²·6)`) and a wide low-energy
skirt (`exp(-t²·1.3)` at 30%). The tight lobe survives the bloom downsample and
reads as a crisp line; the skirt is the beam glowing. Tuning these two against
each other is most of the art direction — raise the skirt for hazy Tron, drop
it for clean 1980 vector.

## Why themes get neonised rather than used directly

Omarchy palettes are tuned for eight hours of comfortable reading: mid contrast,
restrained chroma. Applied literally, every theme produces a washed-out grey
game. So each colour is converted to OKLCH, its hue is preserved, and lightness
and chroma are forced to neon targets. Hue is what makes Everforest recognisably
Everforest; the rest is what makes it readable prose rather than an arcade
cabinet. Keep the first, discard the second.

The void is handled separately: it keeps a trace of the theme background's hue
at a fraction of its chroma, crushed to near-black. Bloom needs somewhere
genuinely dark to bloom into.
