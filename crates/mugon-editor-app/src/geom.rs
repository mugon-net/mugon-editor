//! Bounding-box math: ray picking and neighbour snapping.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) / 2.0
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn translated(&self, d: Vec3) -> Self {
        Self { min: self.min + d, max: self.max + d }
    }

    /// Bounds of this box after transforming its 8 corners.
    pub fn transformed(&self, m: &Mat4) -> Self {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { self.min.x } else { self.max.x },
                if i & 2 == 0 { self.min.y } else { self.max.y },
                if i & 4 == 0 { self.min.z } else { self.max.z },
            );
            let p = m.transform_point3(c);
            min = min.min(p);
            max = max.max(p);
        }
        Self { min, max }
    }
}

/// Distance along the ray to the box, or `None`.
pub fn ray_aabb(origin: Vec3, dir: Vec3, b: &Aabb) -> Option<f32> {
    let inv = Vec3::ONE / dir;
    let t1 = (b.min - origin) * inv;
    let t2 = (b.max - origin) * inv;
    let tmin = t1.min(t2).max_element();
    let tmax = t1.max(t2).min_element();
    if tmax >= tmin.max(0.0) {
        Some(tmin.max(0.0))
    } else {
        None
    }
}

/// Ray against a box given in local space and placed by `world` (so rotated entities pick correctly).
pub fn ray_obb(origin: Vec3, dir: Vec3, local: &Aabb, world: &Mat4) -> Option<f32> {
    let inv = world.inverse();
    let o = inv.transform_point3(origin);
    let d = inv.transform_vector3(dir);
    let t = ray_aabb(o, d, local)?;
    // distance parameter is preserved by the affine transform (dir is not renormalised)
    Some(t)
}

/// Intersection of a ray with the plane y = `height`.
pub fn ray_ground(origin: Vec3, dir: Vec3, height: f32) -> Option<Vec3> {
    if dir.y.abs() < 1e-6 {
        return None;
    }
    let t = (height - origin.y) / dir.y;
    (t > 0.0).then(|| origin + dir * t)
}

/// Translation that makes `moving` touch or align with nearby boxes in `others`.
///
/// Per axis it considers placing a face against the neighbour's opposite face (touching) and aligning faces
/// (flush), and applies the smallest correction within `threshold`. Neighbours must be close on the other axes.
pub fn snap_translation(moving: &Aabb, others: &[Aabb], threshold: f32) -> Vec3 {
    let mut delta = Vec3::ZERO;
    for axis in 0..3 {
        let (a1, a2) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut best: Option<f32> = None;
        for o in others {
            let near = |k: usize| moving.min[k] <= o.max[k] + threshold && moving.max[k] >= o.min[k] - threshold;
            if !(near(a1) && near(a2)) {
                continue;
            }
            for d in [
                o.max[axis] - moving.min[axis],
                o.min[axis] - moving.max[axis],
                o.min[axis] - moving.min[axis],
                o.max[axis] - moving.max[axis],
            ] {
                if d.abs() <= threshold && best.is_none_or(|b| d.abs() < b.abs()) {
                    best = Some(d);
                }
            }
        }
        delta[axis] = best.unwrap_or(0.0);
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_at(x: f32, y: f32, z: f32) -> Aabb {
        Aabb::new(Vec3::new(x, y, z), Vec3::new(x + 1.0, y + 1.0, z + 1.0))
    }

    #[test]
    fn snaps_face_to_face() {
        let wall = unit_at(0.0, 0.0, 0.0);
        let moving = unit_at(1.15, 0.05, 0.0); // 0.15 gap in x, 0.05 off in y
        let d = snap_translation(&moving, &[wall], 0.3);
        assert!((d.x - -0.15).abs() < 1e-5, "{d:?}");
        assert!((d.y - -0.05).abs() < 1e-5, "{d:?}");
        assert_eq!(d.z, 0.0);
    }

    #[test]
    fn ignores_far_boxes() {
        let d = snap_translation(&unit_at(5.0, 0.0, 0.0), &[unit_at(0.0, 0.0, 0.0)], 0.3);
        assert_eq!(d, Vec3::ZERO);
    }

    #[test]
    fn picks_rotated_box() {
        let b = Aabb::new(Vec3::new(-1.0, -0.1, -0.1), Vec3::new(1.0, 0.1, 0.1)); // long along x
        let world = Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2); // now long along y
        assert!(ray_obb(Vec3::new(0.0, 0.8, 5.0), Vec3::NEG_Z, &b, &world).is_some());
        assert!(ray_obb(Vec3::new(0.8, 0.0, 5.0), Vec3::NEG_Z, &b, &world).is_none());
    }

    #[test]
    fn ground_hit() {
        let p = ray_ground(Vec3::new(0.0, 2.0, 0.0), Vec3::new(1.0, -1.0, 0.0), 0.0).unwrap();
        assert!((p - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
    }
}
