use glam::f64::DVec3;

/// Double precision Axis Aligned Bounding Box
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DAABox {
    pub min: DVec3,
    pub max: DVec3,
}

impl DAABox {
    /// Creates a new DAABox from two corner points.
    /// Automatically determines which point is min and which is max for each axis.
    pub fn new(corner1: DVec3, corner2: DVec3) -> Self {
        DAABox {
            min: DVec3::new(
                corner1.x.min(corner2.x),
                corner1.y.min(corner2.y),
                corner1.z.min(corner2.z),
            ),
            max: DVec3::new(
                corner1.x.max(corner2.x),
                corner1.y.max(corner2.y),
                corner1.z.max(corner2.z),
            ),
        }
    }

    /// Creates a new DAABox from a starting point and size vector.
    /// The size vector can have negative components.
    pub fn from_start_and_size(start: DVec3, size: DVec3) -> Self {
        Self::new(start, start + size)
    }

    /// Creates a new DAABox with explicitly specified min and max corners.
    /// Does not validate that min <= max, use this only when you're certain of the order.
    pub fn from_min_max(min: DVec3, max: DVec3) -> Self {
        DAABox { min, max }
    }

    /// Returns the size of the box in each dimension.
    pub fn size(&self) -> DVec3 {
        self.max - self.min
    }

    /// Returns the center point of the box.
    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    /// Expands the box by the given epsilon in all directions.
    /// This makes the box larger by 2*epsilon in each dimension.
    pub fn expand(&self, epsilon: f64) -> Self {
        DAABox {
            min: self.min - DVec3::splat(epsilon),
            max: self.max + DVec3::splat(epsilon),
        }
    }

    /// Checks if this box overlaps with another box.
    /// Returns true if the boxes overlap or touch.
    pub fn overlaps(&self, other: &DAABox) -> bool {
        self.min.x <= other.max.x && self.max.x >= other.min.x &&
        self.min.y <= other.max.y && self.max.y >= other.min.y &&
        self.min.z <= other.max.z && self.max.z >= other.min.z
    }

    /// Conservative overlap check that expands this box by epsilon before checking overlap.
    /// This ensures we don't miss overlaps due to numerical precision issues.
    /// It's okay to report false positives (non-overlapping boxes as overlapping),
    /// but we must never miss a true overlap.
    pub fn conservative_overlap(&self, other: &DAABox, epsilon: f64) -> bool {
        let expanded_self = self.expand(epsilon);
        expanded_self.overlaps(other)
    }

    /// Checks if a point is inside this box (inclusive of boundaries).
    pub fn contains_point(&self, point: DVec3) -> bool {
        point.x >= self.min.x && point.x <= self.max.x &&
        point.y >= self.min.y && point.y <= self.max.y &&
        point.z >= self.min.z && point.z <= self.max.z
    }

    /// Returns true if this box is valid (min <= max in all dimensions).
    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x &&
        self.min.y <= self.max.y &&
        self.min.z <= self.max.z
    }
}

















