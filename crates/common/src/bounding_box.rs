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

#[cfg(test)]
mod tests {
    use super::*;

    /// Creation and basic properties of a bounding box.
    ///
    /// This test demonstrates creating a bounding box and verifying its basic properties.
    #[test]
    fn test_basic_creation_and_properties() {
        // Create a basic bounding box from (1,1,1) to (4,5,6)
        let bbox = BoundingBox {
            min: Vec3::new(1.0, 1.0, 1.0),
            max: Vec3::new(4.0, 5.0, 6.0),
        };

        // Verify min/max properties
        assert_eq!(bbox.min.x, 1.0);
        assert_eq!(bbox.min.y, 1.0);
        assert_eq!(bbox.min.z, 1.0);

        assert_eq!(bbox.max.x, 4.0);
        assert_eq!(bbox.max.y, 5.0);
        assert_eq!(bbox.max.z, 6.0);

        // Create a bounding box using Default
        let default_bbox = BoundingBox::default();
        assert_eq!(default_bbox.min.x, 0.0);
        assert_eq!(default_bbox.min.y, 0.0);
        assert_eq!(default_bbox.min.z, 0.0);
        assert_eq!(default_bbox.max.x, 0.0);
        assert_eq!(default_bbox.max.y, 0.0);
        assert_eq!(default_bbox.max.z, 0.0);
    }

    /// Point containment tests.
    ///
    /// This test demonstrates checking if points are inside or outside a bounding box.
    #[test]
    fn test_contains_point() {
        let bbox = BoundingBox {
            min: Vec3::new(1.0, 1.0, 1.0),
            max: Vec3::new(4.0, 5.0, 6.0),
        };

        // Test points inside the box
        assert!(bbox.contains(Vec3::new(2.0, 3.0, 4.0)));
        assert!(bbox.contains(Vec3::new(1.0, 1.0, 1.0))); // Exactly on min corner
        assert!(bbox.contains(Vec3::new(4.0, 5.0, 6.0))); // Exactly on max corner

        // Test boundary points (should be included)
        assert!(bbox.contains(Vec3::new(1.0, 3.0, 5.0))); // On min x face
        assert!(bbox.contains(Vec3::new(4.0, 3.0, 5.0))); // On max x face

        // Test points outside the box
        assert!(!bbox.contains(Vec3::new(0.0, 3.0, 4.0))); // Outside min x
        assert!(!bbox.contains(Vec3::new(5.0, 3.0, 4.0))); // Outside max x
        assert!(!bbox.contains(Vec3::new(2.0, 0.0, 4.0))); // Outside min y
        assert!(!bbox.contains(Vec3::new(2.0, 6.0, 4.0))); // Outside max y
        assert!(!bbox.contains(Vec3::new(2.0, 3.0, 0.0))); // Outside min z
        assert!(!bbox.contains(Vec3::new(2.0, 3.0, 7.0))); // Outside max z

        // Test points far outside
        assert!(!bbox.contains(Vec3::new(-10.0, -10.0, -10.0)));
    }

    /// Union of two bounding boxes.
    ///
    /// This test shows how to combine two bounding boxes to get a larger one that contains both.
    #[test]
    fn test_union() {
        let box1 = BoundingBox {
            min: Vec3::new(1.0, 1.0, 1.0),
            max: Vec3::new(3.0, 3.0, 3.0),
        };

        let box2 = BoundingBox {
            min: Vec3::new(2.0, 2.0, 2.0),
            max: Vec3::new(5.0, 5.0, 5.0),
        };

        // Create union of the two boxes
        let union_box = box1.union(&box2);

        // The union should extend from the min of both boxes to the max of both boxes
        assert_eq!(union_box.min.x, 1.0);
        assert_eq!(union_box.min.y, 1.0);
        assert_eq!(union_box.min.z, 1.0);

        assert_eq!(union_box.max.x, 5.0);
        assert_eq!(union_box.max.y, 5.0);
        assert_eq!(union_box.max.z, 5.0);

        // Test with non-overlapping boxes
        let box3 = BoundingBox {
            min: Vec3::new(-3.0, -3.0, -3.0),
            max: Vec3::new(-1.0, -1.0, -1.0),
        };

        let union_box2 = box1.union(&box3);

        assert_eq!(union_box2.min.x, -3.0);
        assert_eq!(union_box2.min.y, -3.0);
        assert_eq!(union_box2.min.z, -3.0);

        assert_eq!(union_box2.max.x, 3.0);
        assert_eq!(union_box2.max.y, 3.0);
        assert_eq!(union_box2.max.z, 3.0);
    }

    /// Enclosing points to grow a bounding box.
    ///
    /// This test demonstrates how to grow a bounding box to include additional points.
    #[test]
    fn test_enclose_point() {
        // Start with a small bounding box
        let mut bbox = BoundingBox {
            min: Vec3::new(1.0, 1.0, 1.0),
            max: Vec3::new(2.0, 2.0, 2.0),
        };

        // Enclose a point that's outside in the positive direction
        bbox.enclose_point(Vec3::new(3.0, 2.5, 4.0));

        // Box should grow only in the directions where the point exceeds the current bounds
        assert_eq!(bbox.min.x, 1.0); // Unchanged
        assert_eq!(bbox.min.y, 1.0); // Unchanged
        assert_eq!(bbox.min.z, 1.0); // Unchanged

        assert_eq!(bbox.max.x, 3.0); // Grew to include x=3
        assert_eq!(bbox.max.y, 2.5); // Grew to include y=2.5
        assert_eq!(bbox.max.z, 4.0); // Grew to include z=4

        // Enclose a point that's outside in the negative direction
        bbox.enclose_point(Vec3::new(0.0, 0.5, -1.0));

        // Box should grow in the negative directions where needed
        assert_eq!(bbox.min.x, 0.0); // Grew to include x=0
        assert_eq!(bbox.min.y, 0.5); // Grew to include y=0.5
        assert_eq!(bbox.min.z, -1.0); // Grew to include z=-1

        // Max values remain unchanged
        assert_eq!(bbox.max.x, 3.0);
        assert_eq!(bbox.max.y, 2.5);
        assert_eq!(bbox.max.z, 4.0);

        // Enclose a point already inside the box - should not change dimensions
        let before = bbox;
        bbox.enclose_point(Vec3::new(1.5, 1.5, 1.5));
        assert_eq!(bbox.min, before.min);
        assert_eq!(bbox.max, before.max);
    }

    /// Enclosing spheres to grow a bounding box.
    ///
    /// This test shows how to expand a bounding box to contain spheres.
    #[test]
    fn test_enclose_sphere() {
        // Start with a small bounding box
        let mut bbox = BoundingBox {
            min: Vec3::new(1.0, 1.0, 1.0),
            max: Vec3::new(2.0, 2.0, 2.0),
        };

        // Enclose a sphere at (3,3,3) with radius 1.0
        bbox.enclose_sphere(Vec3::new(3.0, 3.0, 3.0), 1.0);

        // Box should extend to contain the entire sphere
        // Sphere at (3,3,3) with radius 1 extends from (2,2,2) to (4,4,4)
        assert_eq!(bbox.min.x, 1.0); // Original min x is smaller than sphere's min x
        assert_eq!(bbox.min.y, 1.0); // Original min y is smaller than sphere's min y
        assert_eq!(bbox.min.z, 1.0); // Original min z is smaller than sphere's min z

        assert_eq!(bbox.max.x, 4.0); // Grew to include sphere's max x
        assert_eq!(bbox.max.y, 4.0); // Grew to include sphere's max y
        assert_eq!(bbox.max.z, 4.0); // Grew to include sphere's max z

        // Enclose a sphere at (0,0,0) with radius 2.0
        bbox.enclose_sphere(Vec3::new(0.0, 0.0, 0.0), 2.0);

        // Box should grow to contain this sphere too
        // Sphere at (0,0,0) with radius 2 extends from (-2,-2,-2) to (2,2,2)
        assert_eq!(bbox.min.x, -2.0);
        assert_eq!(bbox.min.y, -2.0);
        assert_eq!(bbox.min.z, -2.0);

        // Max values remain the same since the sphere doesn't extend beyond the current max
        assert_eq!(bbox.max.x, 4.0);
        assert_eq!(bbox.max.y, 4.0);
        assert_eq!(bbox.max.z, 4.0);
    }
}

// End of File
