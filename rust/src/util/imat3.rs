use glam::{IVec3, DMat3, DVec3};

/// A 3x3 integer matrix for integer-based transformations.
/// Used primarily for rotating integer crystal lattice positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IMat3 {
    /// The matrix columns stored as three IVec3s
    pub cols: [IVec3; 3],
}

impl IMat3 {
    /// Creates a new IMat3 from three column vectors representing
    /// the transformed basis vectors.
    ///
    /// # Arguments
    ///
    /// * `x_axis` - The transformed x-axis (first column)
    /// * `y_axis` - The transformed y-axis (second column)
    /// * `z_axis` - The transformed z-axis (third column)
    pub fn new(x_axis: &IVec3, y_axis: &IVec3, z_axis: &IVec3) -> Self {
        Self {
            cols: [*x_axis, *y_axis, *z_axis],
        }
    }

    /// Creates an identity matrix.
    pub fn identity() -> Self {
        Self {
            cols: [
                IVec3::new(1, 0, 0),
                IVec3::new(0, 1, 0),
                IVec3::new(0, 0, 1),
            ],
        }
    }

    /// Performs matrix multiplication with a vector (M * v).
    ///
    /// # Arguments
    ///
    /// * `vec` - The vector to transform
    ///
    /// # Returns
    ///
    /// The transformed vector
    pub fn mul(&self, vec: &IVec3) -> IVec3 {
        let x = self.cols[0] * vec.x;
        let y = self.cols[1] * vec.y;
        let z = self.cols[2] * vec.z;
        x + y + z
    }

    /// Converts the integer matrix to a double-precision matrix.
    ///
    /// # Returns
    ///
    /// A `DMat3` with the same values as this matrix, converted to f64.
    pub fn as_dmat3(&self) -> DMat3 {
        DMat3::from_cols(
            self.cols[0].as_dvec3(),
            self.cols[1].as_dvec3(),
            self.cols[2].as_dvec3()
        )
    }
}
