// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use ultraviolet::Vec3;

/// An axis-aligned bounding box defined by two opposite corners (`min` and `max`).
/// `min.x <= max.x`, `min.y <= max.y`, `min.z <= max.z`.
#[derive(Copy, Clone, Debug, Default)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl BoundingBox {
    /// Returns the smallest `BoundingBox` that would contain both `self` and `other`.
    #[allow(dead_code)]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: Vec3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Vec3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }

    /// Returns true if the provided `point` is inside this `BoundingBox`.
    /// Otherwise returns false.
    #[allow(dead_code)]
    pub fn contains(&self, point: Vec3) -> bool {
        self.min.x <= point.x
            && point.x <= self.max.x
            && self.min.y <= point.y
            && point.y <= self.max.y
            && self.min.z <= point.z
            && point.z <= self.max.z
    }

    /// Grows this `BoundingBox` in-place to ensure that it will contain a given `point`.
    #[allow(dead_code)]
    pub fn enclose_point(&mut self, point: Vec3) {
        self.min.x = f32::min(self.min.x, point.x);
        self.min.y = f32::min(self.min.y, point.y);
        self.min.z = f32::min(self.min.z, point.z);

        self.max.x = f32::max(self.max.x, point.x);
        self.max.y = f32::max(self.max.y, point.y);
        self.max.z = f32::max(self.max.z, point.z);
    }

    /// Grows this `BoundingBox` in-place to ensure that it will contain a
    /// sphere with a specified `center` position and `radius`.
    pub fn enclose_sphere(&mut self, center: Vec3, radius: f32) {
        self.min.x = f32::min(self.min.x, center.x - radius);
        self.min.y = f32::min(self.min.y, center.y - radius);
        self.min.z = f32::min(self.min.z, center.z - radius);

        self.max.x = f32::max(self.max.x, center.x + radius);
        self.max.y = f32::max(self.max.y, center.y + radius);
        self.max.z = f32::max(self.max.z, center.z + radius);
    }
}

// End of File
