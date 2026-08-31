// Screen-space expanded line quads with a hot core, soft halo and
// beam-dwell brightening at endpoints. Draws into an HDR target.
//
// One instance = one segment. Six vertices per instance, no index buffer.

struct Globals {
    view:       mat4x4<f32>,
    proj:       mat4x4<f32>,
    viewport:   vec2<f32>,   // pixels
    near:       f32,         // positive distance to near plane
    fade_near:  f32,         // view distance where distance fade starts
    fade_far:   f32,         // view distance where lines reach fade_floor
    fade_floor: f32,         // intensity multiplier at fade_far (0.15 reads well)
    dwell_gain: f32,         // extra brightness at segment endpoints (0.6 default)
    _pad:       f32,
};

@group(0) @binding(0) var<uniform> g: Globals;

struct Instance {
    @location(0) a:      vec3<f32>,  // world-space start
    @location(1) b:      vec3<f32>,  // world-space end
    @location(2) color:  vec3<f32>,  // linear RGB, unclamped
    @location(3) params: vec2<f32>,  // x = width in px, y = intensity multiplier
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    // uv.x = distance along the spine in px (0..len is the segment, outside is a cap)
    // uv.y = perpendicular offset in px
    @location(0)                 uv:     vec2<f32>,
    @location(1) @interpolate(flat) geom:   vec2<f32>,  // x = len px, y = half width px
    @location(2) @interpolate(flat) color:  vec3<f32>,
    @location(3)                 shade:  vec2<f32>,     // x = distance fade, y = intensity
};

fn to_screen(clip: vec4<f32>) -> vec2<f32> {
    let ndc = clip.xy / clip.w;
    return (ndc * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5)) * g.viewport;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0,  1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    let q = corners[vi];

    var out: VsOut;
    out.color = inst.color;

    var av = (g.view * vec4<f32>(inst.a, 1.0)).xyz;
    var bv = (g.view * vec4<f32>(inst.b, 1.0)).xyz;

    // Right-handed view space looks down -z, so visible points have z < -near.
    // Without this clip, a segment straddling the camera plane whips across
    // the screen instead of disappearing off the edge of it.
    let nz = -g.near;
    if (av.z > nz && bv.z > nz) {
        out.pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    if (av.z > nz) { av = mix(av, bv, (nz - av.z) / (bv.z - av.z)); }
    if (bv.z > nz) { bv = mix(bv, av, (nz - bv.z) / (av.z - bv.z)); }

    let ca = g.proj * vec4<f32>(av, 1.0);
    let cb = g.proj * vec4<f32>(bv, 1.0);
    let sa = to_screen(ca);
    let sb = to_screen(cb);

    let delta = sb - sa;
    let len = length(delta);
    if (len < 1e-4) {
        out.pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    let dir = delta / len;
    let nrm = vec2<f32>(-dir.y, dir.x);

    let hw = max(inst.params.x, 0.5) * 0.5;
    let along = q.x;
    // Push the quad past both endpoints by hw so round caps have room to live.
    let cap = (along * 2.0 - 1.0) * hw;
    let p = mix(sa, sb, along) + dir * cap + nrm * (q.y * hw);

    let ndc = (p / g.viewport * 2.0 - vec2<f32>(1.0)) * vec2<f32>(1.0, -1.0);
    let depth = mix(ca.z / ca.w, cb.z / cb.w, along);

    // w = 1 so the varyings interpolate linearly in screen space, which is what
    // the px-space uv needs. Depth still works, it is just linear along the
    // segment rather than perspective-correct -- invisible on lines this thin.
    out.pos = vec4<f32>(ndc, depth, 1.0);
    out.uv = vec2<f32>(along * len + cap, q.y * hw);
    out.geom = vec2<f32>(len, hw);

    let dist = -mix(av.z, bv.z, along);
    let fade = mix(1.0, g.fade_floor, smoothstep(g.fade_near, g.fade_far, dist));
    out.shade = vec2<f32>(fade, inst.params.y);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let len = in.geom.x;
    let hw  = in.geom.y;

    // Distance to the capsule spine: clamping along the axis gives round caps
    // for free.
    let spine = clamp(in.uv.x, 0.0, len);
    let d = length(vec2<f32>(in.uv.x - spine, in.uv.y));
    let t = d / hw;

    // Two lobes: a tight core that survives the bloom downsample as a bright
    // line, and a narrow low-energy skirt that reads as the beam glowing
    // without softening the stroke itself. The skirt was originally wide and
    // heavy (1.3 / 0.3), which read as blur rather than glow.
    let core = exp(-t * t * 9.0);
    let halo = exp(-t * t * 2.2) * 0.18;

    // Beam dwell. A real XY monitor decelerates the beam at each vertex, so
    // corners burn hotter than the middle of a stroke.
    let end_d = min(in.uv.x, len - in.uv.x) / max(hw, 1.0);
    let dwell = exp(-end_d * end_d * 0.6) * g.dwell_gain;

    let e = (core + halo) * (1.0 + dwell) * in.shade.x * in.shade.y;
    if (e < 0.002) { discard; }

    return vec4<f32>(in.color * e, e);
}
