//! Fly camera (RMB look + WASD/QE, MMB pan, wheel dolly) and the ground grid.

use crate::model::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

#[derive(Component)]
pub struct FlyCam {
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

impl FlyCam {
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }
}

pub fn cursor_in_viewport(window: &Window, ui: &UiState) -> bool {
    window.cursor_position().is_some_and(|p| ui.viewport.contains(p)) && !ui.pointer_over_ui
}

pub fn camera_controls(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    ui: Res<UiState>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cam: Single<(&mut Transform, &mut FlyCam)>,
    mut grabbed: Local<Option<MouseButton>>,
) {
    let (transform, fly) = &mut *cam;
    let inside = cursor_in_viewport(&window, &ui);

    if inside {
        for b in [MouseButton::Right, MouseButton::Middle] {
            if mouse.just_pressed(b) {
                *grabbed = Some(b);
            }
        }
    }
    if let Some(b) = *grabbed {
        if !mouse.pressed(b) {
            *grabbed = None;
        }
    }

    let dt = time.delta_secs();
    match *grabbed {
        Some(MouseButton::Right) => {
            fly.yaw -= motion.delta.x * 0.003;
            fly.pitch = (fly.pitch - motion.delta.y * 0.003).clamp(-1.54, 1.54);
            let rot = fly.rotation();
            let mut dir = Vec3::ZERO;
            if !ui.wants_keyboard {
                if keys.pressed(KeyCode::KeyW) { dir += rot * Vec3::NEG_Z; }
                if keys.pressed(KeyCode::KeyS) { dir += rot * Vec3::Z; }
                if keys.pressed(KeyCode::KeyA) { dir += rot * Vec3::NEG_X; }
                if keys.pressed(KeyCode::KeyD) { dir += rot * Vec3::X; }
                if keys.pressed(KeyCode::KeyE) { dir += Vec3::Y; }
                if keys.pressed(KeyCode::KeyQ) { dir += Vec3::NEG_Y; }
            }
            let boost = if keys.pressed(KeyCode::ShiftLeft) { 4.0 } else { 1.0 };
            transform.translation += dir.normalize_or_zero() * fly.speed * boost * dt;
        }
        Some(MouseButton::Middle) => {
            let rot = fly.rotation();
            let k = fly.speed * 0.0025;
            transform.translation += rot * Vec3::new(-motion.delta.x * k, motion.delta.y * k, 0.0);
        }
        _ => {}
    }

    if inside && scroll.delta.y != 0.0 {
        let rot = fly.rotation();
        transform.translation += rot * Vec3::NEG_Z * scroll.delta.y * fly.speed * 0.15;
        // wheel while flying adjusts the speed instead
        if matches!(*grabbed, Some(MouseButton::Right)) {
            fly.speed = (fly.speed * (1.0 + scroll.delta.y * 0.1)).clamp(0.5, 500.0);
        }
    }
    transform.rotation = fly.rotation();
}

pub fn draw_grid(mut gizmos: Gizmos) {
    gizmos.grid(
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        UVec2::splat(80),
        Vec2::splat(1.0),
        Color::srgba(0.5, 0.55, 0.65, 0.25),
    );
    gizmos.line(Vec3::ZERO, Vec3::X * 2.0, Color::srgb(0.9, 0.2, 0.2));
    gizmos.line(Vec3::ZERO, Vec3::Y * 2.0, Color::srgb(0.2, 0.9, 0.2));
    gizmos.line(Vec3::ZERO, Vec3::Z * 2.0, Color::srgb(0.3, 0.4, 0.95));
}
