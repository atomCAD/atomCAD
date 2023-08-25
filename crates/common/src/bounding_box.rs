// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

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

    /// Computes the 1D intersection times for a directed line segment. Imagine a point moving the
    /// number line, starting at `origin` (when t=0) and moving at some `speed`.  If the point ever
    /// crosses the value `min` or `max`, then this function will return Some((t_min, t_max)), such
    /// that `origin + speed * t_min = min` adn `origin + speed * t_max = max`. These time values
    /// may be negative if needed to satisfy those relations.
    ///
    /// If `speed` is zero, but the `origin` is between `min` and `max`, then this function returns
    /// `Some((f32::NEG_INFINITY, f32::INFINITY))`.  This signifies that, although intersection will
    /// not happen, the point *is* between `min` and `max` for all times.
    ///
    /// If `speed` is zero and `origin` is outside of `[min, max]`, then this function returns
    /// `None` to indicate that intersection is impossible and the point will never be in the
    /// provided range.
    fn intersection_times(origin: f32, speed: f32, min: f32, max: f32) -> Option<(f32, f32)> {
        // If the speed is non-zero, we can compute the times normally.
        if speed != 0.0 {
            let t1 = (min - origin) / speed;
            let t2 = (max - origin) / speed;
            Some((f32::min(t1, t2), f32::max(t1, t2)))
        }
        // If the speed is zero and the origin is within the bounding slab along that axis, the ray
        // runs parallel to the slab.  We represent this as "intersecting" the slab from negative
        // infinity to positive infinity.
        else if origin >= min && origin <= max {
            Some((f32::NEG_INFINITY, f32::INFINITY))
        }
        // If the direction component is zero and the origin is outside the bounding slab along that
        // axis, the ray will never intersect the slab.
        else {
            None
        }
    }

    // Given a raycast defined by target_point = origin + velocity * t, returns the values of t
    // where the ray enters and exits the box (in that order).  If the ray fails to hit this box,
    // returns None. Note that the time values may be negative, but tmin will always be <= tmax.
    pub fn ray_hit_times(&self, origin: Vec3, velocity: Vec3) -> Option<(f32, f32)> {
        // Calculate intersection times using the function for each axis
        let (tx1, tx2) = Self::intersection_times(origin.x, velocity.x, self.min.x, self.max.x)?;
        let (ty1, ty2) = Self::intersection_times(origin.y, velocity.y, self.min.y, self.max.y)?;
        let (tz1, tz2) = Self::intersection_times(origin.z, velocity.z, self.min.z, self.max.z)?;

        // Find the largest tmin and smallest tmax across all axes
        let tmin = tx1.max(ty1).max(tz1);
        let tmax = tx2.min(ty2).min(tz2);

        // If the ray exits the box before it enters it, then our assumption
        // that the ray intersects with the box is wrong and we return None.
        if tmin > tmax {
            return None;
        }

        Some((tmin, tmax))
    }
}

// End of File
