//! Tank camera. Deliberately not a free-fly camera: BattleZone's feel comes
//! from being welded to a slow chassis that can only rotate and drive.

use glam::{Mat4, Vec3};

pub struct Camera {
    pub pos: Vec3,
    pub yaw: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { pos: Vec3::new(0.0, 1.6, 0.0), yaw: 0.0, fov_y: 1.05, near: 0.1, far: 900.0 }
    }
}

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub forward: f32,
    pub turn: f32,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    pub fn update(&mut self, input: Input, dt: f32) {
        const DRIVE: f32 = 26.0;
        const TURN: f32 = 1.4;
        self.yaw += input.turn * TURN * dt;
        self.pos += self.forward() * input.forward * DRIVE * dt;
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_to_rh(self.pos, self.forward(), Vec3::Y)
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect, self.near, self.far)
    }
}
