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

// Tron (1982)'s tanks read nothing like a real-world turret-and-hull tank:
// low, wide, and near bilaterally symmetric front-to-back (the recognizer
// grid gave them no obvious front), a sloped wedge deck rather than a round
// turret, and one raised center cabin as the only tall feature. Flat facets
// throughout -- no cylinder ever reads honestly as a handful of line
// segments, so nothing here tries to be one.
fn tank(batch: &mut LineBatch, pos: Vec3, yaw: f32, color: [f32; 3]) {
    let (s, c) = yaw.sin_cos();
    let rot = |v: Vec3| Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c) + pos;
    let mut edge = |a: Vec3, b: Vec3| batch.segment(rot(a).into(), rot(b).into(), color, 2.2, 1.9);

    // Wireframe frustum: a rectangular ring at y0 sized (hw0, hl0) connected
    // to another at y1 sized (hw1, hl1). A box is just hw0==hw1, hl0==hl1.
    let mut frustum = |hw0: f32, hl0: f32, y0: f32, hw1: f32, hl1: f32, y1: f32| {
        let sx = [-1.0f32, 1.0, 1.0, -1.0];
        let sz = [-1.0f32, -1.0, 1.0, 1.0];
        let lo = |i: usize| Vec3::new(sx[i] * hw0, y0, sz[i] * hl0);
        let hi = |i: usize| Vec3::new(sx[i] * hw1, y1, sz[i] * hl1);
        for i in 0..4 {
            let j = (i + 1) % 4;
            edge(lo(i), lo(j));
            edge(hi(i), hi(j));
            edge(lo(i), hi(i));
        }
    };

    // Low, wide wedge hull: sides slope inward toward a flat deck instead of
    // sitting under a turret.
    let (hw, hl, hh) = (4.2, 6.5, 1.6);
    frustum(hw, hl, 0.0, hw * 0.72, hl * 0.9, hh);

    // Raised center cabin -- the one tall feature, sat mid-deck.
    let (cw, cl, ch) = (hw * 0.4, hl * 0.42, 1.5);
    frustum(cw, cl, hh, cw * 0.85, cl * 0.85, hh + ch);

    // Twin tread skirts along the flanks, long and low, same at both ends.
    let (tw, tl, th) = (0.9, hl * 0.95, hh * 0.55);
    for side in [-1.0f32, 1.0] {
        let cx = side * (hw - tw * 0.5 - 0.1);
        let sx = [-1.0f32, 1.0, 1.0, -1.0];
        let sz = [-1.0f32, -1.0, 1.0, 1.0];
        let lo = |i: usize| Vec3::new(cx + sx[i] * tw * 0.5, 0.0, sz[i] * tl);
        let hi = |i: usize| Vec3::new(cx + sx[i] * tw * 0.5, th, sz[i] * tl);
        for i in 0..4 {
            let j = (i + 1) % 4;
            edge(lo(i), lo(j));
            edge(hi(i), hi(j));
            edge(lo(i), hi(i));
        }
    }
}
