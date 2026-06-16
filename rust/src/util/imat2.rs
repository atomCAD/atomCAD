use glam::{DMat2, IVec2};

/// A 2x2 integer matrix for integer-based transformations.
/// Used primarily for expressing 2D superlattices over crystal planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IMat2 {
    /// The matrix columns stored as two IVec2s
    pub cols: [IVec2; 2],
}

impl IMat2 {
    /// Creates a new IMat2 from two column vectors representing
    /// the transformed basis vectors.
    ///
    /// # Arguments
    ///
    /// * `x_axis` - The transformed x-axis (first column)
    /// * `y_axis` - The transformed y-axis (second column)
    pub fn new(x_axis: &IVec2, y_axis: &IVec2) -> Self {
        Self {
            cols: [*x_axis, *y_axis],
        }
    }

    /// Creates an identity matrix.
    pub fn identity() -> Self {
        Self {
            cols: [IVec2::new(1, 0), IVec2::new(0, 1)],
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
    pub fn mul(&self, vec: &IVec2) -> IVec2 {
        let x = self.cols[0] * vec.x;
        let y = self.cols[1] * vec.y;
        x + y
    }

    /// Performs matrix multiplication with another IMat2 matrix (self * other).
    ///
    /// # Arguments
    ///
    /// * `other` - The matrix to multiply with
    ///
    /// # Returns
    ///
    /// A new IMat2 that is the result of the matrix multiplication
    pub fn mul_imat2(&self, other: &IMat2) -> IMat2 {
        // For each column in the result matrix, transform the corresponding
        // column from the other matrix
        let x_col = self.mul(&other.cols[0]);
        let y_col = self.mul(&other.cols[1]);

        IMat2::new(&x_col, &y_col)
    }

    /// Converts the integer matrix to a double-precision matrix.
    ///
    /// # Returns
    ///
    /// A `DMat2` with the same values as this matrix, converted to f64.
    pub fn as_dmat2(&self) -> DMat2 {
        DMat2::from_cols(self.cols[0].as_dvec2(), self.cols[1].as_dvec2())
    }
}
