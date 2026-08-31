//! Everything you can see, rebuilt into a line batch each frame.
//!
//! There is no scene graph and no mesh format on purpose. At a few thousand
//! segments the whole world costs less to regenerate than to manage.

use crate::render::line::LineBatch;
use crate::theme::Palette;
use glam::Vec3;

pub struct Obstacle {
    pub pos: Vec3,
    pub kind: Kind,
}

pub enum Kind {
    Cube(f32),
    Pyramid(f32),
    Tank { yaw: f32 },
}

pub struct World {
    pub obstacles: Vec<Obstacle>,
}

impl Default for World {
    fn default() -> Self {
        let mut obstacles = Vec::new();
        // Deterministic scatter -- good enough until there is real spawn logic.
        let mut seed: u32 = 0x9e3779b9;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 10_000) as f32 / 10_000.0
        };
        for _ in 0..40 {
            let x = (next() - 0.5) * 700.0;
            let z = (next() - 0.5) * 700.0;
            let kind = if next() < 0.5 { Kind::Cube(6.0) } else { Kind::Pyramid(9.0) };
            obstacles.push(Obstacle { pos: Vec3::new(x, 0.0, z), kind });
        }
        obstacles.push(Obstacle { pos: Vec3::new(24.0, 0.0, -60.0), kind: Kind::Tank { yaw: 0.6 } });
        obstacles.push(Obstacle { pos: Vec3::new(-90.0, 0.0, -140.0), kind: Kind::Tank { yaw: -1.2 } });
        Self { obstacles }
    }
}

impl World {
    pub fn build(&self, batch: &mut LineBatch, palette: &Palette, eye: Vec3) {
        batch.clear();

        // The grid re-snaps to the player, so the floor is infinite for free
        // and never accumulates float error.
        batch.ground_grid(eye.into(), 12.0, 46, palette.primary, 1.6, 0.9);

        for o in &self.obstacles {
            match o.kind {
                Kind::Cube(s) => batch.cuboid(
                    (o.pos - Vec3::new(s * 0.5, 0.0, s * 0.5)).into(),
                    (o.pos + Vec3::new(s * 0.5, s, s * 0.5)).into(),
                    palette.primary,
                    1.8,
                    1.3,
                ),
                Kind::Pyramid(s) => pyramid(batch, o.pos, s, palette.primary),
                Kind::Tank { yaw } => tank(batch, o.pos, yaw, palette.hostile),
            }
        }
    }
}

fn pyramid(batch: &mut LineBatch, base: Vec3, size: f32, color: [f32; 3]) {
    let h = size * 1.1;
    let r = size * 0.5;
    let apex = base + Vec3::new(0.0, h, 0.0);
    let corners = [
        base + Vec3::new(-r, 0.0, -r),
        base + Vec3::new(r, 0.0, -r),
        base + Vec3::new(r, 0.0, r),
        base + Vec3::new(-r, 0.0, r),
    ];
    for i in 0..4 {
        batch.segment(corners[i].into(), corners[(i + 1) % 4].into(), color, 1.8, 1.3);
        batch.segment(corners[i].into(), apex.into(), color, 1.8, 1.3);
    }
}

// Modeled after stevetalkowski's "TRON Tank" fan reconstruction (Sketchfab),
// cross-checked against a film still: a wide flat wedge -- pointed nose,
// flat rear, wider than it is deep -- carrying a genuinely round stepped
// dome (a small sensor knob on its front shoulder), a mount block tapering
// into a long thin cannon well past the nose, and a hatch disc between the
// dome and the nose. The screen-used red/orange edge trim collapses to the
// single `palette.hostile` colour on purpose: two colour roles only, so a
// player can tell friend from threat at a glance.
fn tank(batch: &mut LineBatch, pos: Vec3, yaw: f32, color: [f32; 3]) {
    let (s, c) = yaw.sin_cos();
    let rot = |v: Vec3| Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c) + pos;
    const W: f32 = 2.0;
    const I: f32 = 1.6;

    // Wireframe frustum: an (x, z) ring `lo` at y0 connected to a same-length
    // ring `hi` at y1. Equal rings give a box or a barrel; a shrinking ring
    // tapers -- this one primitive is what the whole tank is built from.
    // Free function (not a closure) so calls can interleave freely without
    // fighting the borrow checker over a shared capture of `batch`.
    fn frustum(
        batch: &mut LineBatch,
        rot: &impl Fn(Vec3) -> Vec3,
        lo: &[(f32, f32)],
        y0: f32,
        hi: &[(f32, f32)],
        y1: f32,
        color: [f32; 3],
    ) {
        let n = lo.len();
        for i in 0..n {
            let j = (i + 1) % n;
            batch.segment(rot(Vec3::new(lo[i].0, y0, lo[i].1)).into(), rot(Vec3::new(lo[j].0, y0, lo[j].1)).into(), color, W, I);
            batch.segment(rot(Vec3::new(hi[i].0, y1, hi[i].1)).into(), rot(Vec3::new(hi[j].0, y1, hi[j].1)).into(), color, W, I);
            batch.segment(rot(Vec3::new(lo[i].0, y0, lo[i].1)).into(), rot(Vec3::new(hi[i].0, y1, hi[i].1)).into(), color, W, I);
        }
    }
    let rect = |hw: f32, z0: f32, z1: f32| -> [(f32, f32); 4] { [(-hw, z0), (hw, z0), (hw, z1), (-hw, z1)] };
    let octagon = |r: f32, cz: f32| -> [(f32, f32); 8] {
        std::array::from_fn(|i| {
            let a = i as f32 / 8.0 * std::f32::consts::TAU;
            (a.cos() * r, a.sin() * r + cz)
        })
    };

    // Body: a wide flat wedge, nose at -z, flat rear -- a plain triangle
    // reads closer to the reference than any notch or taper does.
    let (half_w, nose_z, rear_z) = (6.4, -6.2, 3.6);
    let body = [(0.0, nose_z), (half_w, rear_z), (-half_w, rear_z)];
    let deck = 0.55;
    frustum(batch, &rot, &body, 0.0, &body, deck, color);

    // Turret: a stepped hemisphere -- three shrinking octagons -- reads as
    // round at this line count where a single cone-to-a-point never did.
    let dome_cz = -1.6;
    let dome_r = 2.1;
    let dome = [(deck, dome_r), (deck + 0.35, dome_r * 0.88), (deck + 0.62, dome_r * 0.55), (deck + 0.85, dome_r * 0.12)];
    for w in dome.windows(2) {
        frustum(batch, &rot, &octagon(w[0].1, dome_cz), w[0].0, &octagon(w[1].1, dome_cz), w[1].0, color);
    }

    // Secondary sensor ball riding the dome's front shoulder.
    let knob_cz = dome_cz - dome_r * 0.7;
    let knob = [(deck + 0.5, 0.55), (deck + 0.78, 0.32), (deck + 0.95, 0.08)];
    for w in knob.windows(2) {
        frustum(batch, &rot, &octagon(w[0].1, knob_cz), w[0].0, &octagon(w[1].1, knob_cz), w[1].0, color);
    }

    // Cannon: a stubby mount block narrowing into a long thin barrel that
    // projects well past the nose.
    let mount_cz = dome_cz - dome_r * 0.9;
    frustum(batch, &rot, &rect(0.55, mount_cz - 0.7, mount_cz + 0.7), deck + 0.15, &rect(0.22, mount_cz - 0.7, mount_cz + 0.7), deck + 0.55, color);
    let barrel_y = deck + 0.35;
    let barrel_front = nose_z - 5.2;
    let barrel = rect(0.16, barrel_front, mount_cz);
    frustum(batch, &rot, &barrel, barrel_y - 0.16, &barrel, barrel_y + 0.16, color);

    // Hatch: a low disc on the deck between the dome and the nose, with a
    // chevron scored into it.
    let hatch_cz = (dome_cz + nose_z) * 0.5;
    let hatch = octagon(1.0, hatch_cz);
    frustum(batch, &rot, &hatch, deck, &hatch, deck + 0.1, color);
    let chev_y = deck + 0.1;
    batch.segment(rot(Vec3::new(-0.5, chev_y, hatch_cz - 0.3)).into(), rot(Vec3::new(0.0, chev_y, hatch_cz + 0.35)).into(), color, W, I);
    batch.segment(rot(Vec3::new(0.5, chev_y, hatch_cz - 0.3)).into(), rot(Vec3::new(0.0, chev_y, hatch_cz + 0.35)).into(), color, W, I);
}
