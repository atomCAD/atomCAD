use glam::DVec3;
use std::collections::HashMap;

/// A data structure for efficiently storing and retrieving unique 3D points within a given epsilon tolerance.
/// Uses a grid-based spatial hashing approach for O(1) lookups.
pub struct Unique3DPoints<T> {
    /// The epsilon value determines the grid cell size and the maximum distance for considering points as identical
    epsilon: f64,
    /// Maps grid cell coordinates to a list of contained points and their associated values
    cells: HashMap<(i64, i64, i64), Vec<(DVec3, T)>>,
}

impl<T: Clone> Unique3DPoints<T> {
    /// Creates a new instance with the specified epsilon tolerance
    pub fn new(epsilon: f64) -> Self {
        assert!(epsilon > 0.0, "Epsilon must be positive");
        Self {
            epsilon,
            cells: HashMap::new(),
        }
    }

    /// Converts a point to its grid cell coordinates
    fn point_to_cell(&self, point: &DVec3) -> (i64, i64, i64) {
        (
            (point.x / self.epsilon).floor() as i64,
            (point.y / self.epsilon).floor() as i64,
            (point.z / self.epsilon).floor() as i64,
        )
    }

    /// Adds a point with associated value if it doesn't exist already within epsilon tolerance.
    /// Returns true if the point was added as new, false if a similar point already existed.
    pub fn add_point(&mut self, point: DVec3, value: T) -> bool {
        // First check if the point already exists (or a similar one)
        if self.get_point(&point).is_some() {
            return false;
        }

        // If not, add it to the appropriate cell
        let cell = self.point_to_cell(&point);
        self.cells.entry(cell).or_insert_with(Vec::new).push((point, value));
        true
    }

    /// Retrieves the value associated with a point that's within epsilon distance of the given point.
    /// Checks the cell containing the point and all 26 neighboring cells.
    pub fn get_point(&self, point: &DVec3) -> Option<&T> {
        let cell = self.point_to_cell(point);
        
        // Check all 27 neighboring cells (including the cell itself)
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let neighbor_cell = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                    
                    if let Some(points) = self.cells.get(&neighbor_cell) {
                        // Check each point in this cell
                        for (existing_point, value) in points {
                            let distance = (*existing_point - *point).length();
                            if distance < self.epsilon {
                                return Some(value);
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
    
    /// Retrieves the point and value that's within epsilon distance of the given point.
    /// Useful when you need both the actual stored point and its associated value.
    pub fn get_point_and_value(&self, point: &DVec3) -> Option<(&DVec3, &T)> {
        let cell = self.point_to_cell(point);
        
        // Check all 27 neighboring cells (including the cell itself)
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let neighbor_cell = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                    
                    if let Some(points) = self.cells.get(&neighbor_cell) {
                        // Check each point in this cell
                        for (existing_point, value) in points {
                            let distance = (*existing_point - *point).length();
                            if distance < self.epsilon {
                                return Some((existing_point, value));
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
    
    /// Gets or inserts a point. If a similar point exists within epsilon, returns its value.
    /// Otherwise, adds the new point with the given value and returns the value.
    pub fn get_or_insert(&mut self, point: DVec3, value: T) -> T 
    where T: Clone {
        if let Some(existing) = self.get_point(&point).cloned() {
            existing
        } else {
            self.add_point(point, value.clone());
            value
        }
    }
    
    /// Returns the number of unique points stored
    pub fn len(&self) -> usize {
        self.cells.values().map(|v| v.len()).sum()
    }
    
    /// Returns true if the collection is empty
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() || self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_add_and_get_point() {
        let mut points = Unique3DPoints::new(0.001);
        
        // Add some points
        points.add_point(DVec3::new(1.0, 2.0, 3.0), 1);
        points.add_point(DVec3::new(4.0, 5.0, 6.0), 2);
        
        // Should find exact matches
        assert_eq!(points.get_point(&DVec3::new(1.0, 2.0, 3.0)), Some(&1));
        assert_eq!(points.get_point(&DVec3::new(4.0, 5.0, 6.0)), Some(&2));
        
        // Should find close matches
        assert_eq!(points.get_point(&DVec3::new(1.0005, 2.0, 3.0)), Some(&1));
        assert_eq!(points.get_point(&DVec3::new(4.0, 5.0004, 6.0)), Some(&2));
        
        // Should not find points that are too far away
        assert_eq!(points.get_point(&DVec3::new(1.002, 2.0, 3.0)), None);
        assert_eq!(points.get_point(&DVec3::new(7.0, 8.0, 9.0)), None);
    }
    
    #[test]
    fn test_add_duplicate_points() {
        let mut points = Unique3DPoints::new(0.001);
        
        // Add a point
        assert!(points.add_point(DVec3::new(1.0, 2.0, 3.0), 1));
        
        // Adding the same point should return false
        assert!(!points.add_point(DVec3::new(1.0, 2.0, 3.0), 2));
        
        // Adding a very close point should also return false
        assert!(!points.add_point(DVec3::new(1.0005, 2.0, 3.0), 3));
        
        // The original value should remain
        assert_eq!(points.get_point(&DVec3::new(1.0, 2.0, 3.0)), Some(&1));
    }
    
    #[test]
    fn test_get_or_insert() {
        let mut points = Unique3DPoints::new(0.001);
        
        // First insertion should use the provided value
        let val1 = points.get_or_insert(DVec3::new(1.0, 2.0, 3.0), 1);
        assert_eq!(val1, 1);
        
        // Second insertion of the same point should return the existing value
        let val2 = points.get_or_insert(DVec3::new(1.0005, 2.0, 3.0), 2);
        assert_eq!(val2, 1);
        
        // Verify the collection has only one point
        assert_eq!(points.len(), 1);
    }
}
















