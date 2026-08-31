// Fullscreen passes: phosphor decay and tonemap.

struct Params {
    decay:    f32,
    exposure: f32,
    _pad0:    f32,
    _pad1:    f32,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> p: Params;

@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var v = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(v[i], 0.0, 1.0);
}

// Lays last frame down at reduced energy before this frame's lines are drawn
// additively on top. Beam persistence -- the thing that makes fast rotation
// smear the way a real XY monitor did.
@fragment
fn fs_decay(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(src, vec2<i32>(pos.xy), 0) * p.decay;
}

@fragment
fn fs_tonemap(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let hdr = textureLoad(src, vec2<i32>(pos.xy), 0).rgb * p.exposure;
    // Simple exponential rolloff. Keeps hot cores white-hot without clipping
    // the surrounding halo into a flat disc.
    return vec4<f32>(vec3<f32>(1.0) - exp(-hdr), 1.0);
}
