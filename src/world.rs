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

fn tank(batch: &mut LineBatch, pos: Vec3, yaw: f32, color: [f32; 3]) {
    let (s, c) = yaw.sin_cos();
    let rot = |v: Vec3| Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c) + pos;
    let mut edge = |a: Vec3, b: Vec3| batch.segment(rot(a).into(), rot(b).into(), color, 2.2, 1.9);

    // Hull.
    let (hw, hh, hl) = (3.0, 2.0, 5.0);
    let hull = |sx: f32, sy: f32, sz: f32| Vec3::new(sx * hw, sy * hh, sz * hl);
    for i in 0..4 {
        let (a, b) = ([-1.0, 1.0, 1.0, -1.0][i], [-1.0, -1.0, 1.0, 1.0][i]);
        let (c2, d) = ([-1.0, 1.0, 1.0, -1.0][(i + 1) % 4], [-1.0, -1.0, 1.0, 1.0][(i + 1) % 4]);
        edge(hull(a, 0.0, b), hull(c2, 0.0, d));
        edge(hull(a, 1.0, b), hull(c2, 1.0, d));
        edge(hull(a, 0.0, b), hull(a, 1.0, b));
    }

    // Turret and barrel.
    let t = 1.6;
    for i in 0..4 {
        let (a, b) = ([-1.0, 1.0, 1.0, -1.0][i], [-1.0, -1.0, 1.0, 1.0][i]);
        let (c2, d) = ([-1.0, 1.0, 1.0, -1.0][(i + 1) % 4], [-1.0, -1.0, 1.0, 1.0][(i + 1) % 4]);
        edge(Vec3::new(a * t, hh, b * t), Vec3::new(c2 * t, hh, d * t));
        edge(Vec3::new(a * t, hh + 1.4, b * t), Vec3::new(c2 * t, hh + 1.4, d * t));
        edge(Vec3::new(a * t, hh, b * t), Vec3::new(a * t, hh + 1.4, b * t));
    }
    edge(Vec3::new(0.0, hh + 0.8, -t), Vec3::new(0.0, hh + 0.8, -hl - 3.5));
}
