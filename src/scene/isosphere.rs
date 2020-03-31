use ultraviolet::Vec3;

use super::Vertex;

// https://schneide.blog/2016/07/15/generating-an-icosphere-in-c/
const Z: f32 = 0.850650808352039932;
const X: f32 = 0.525731112119133606;

const VERTICES: [[f32; 3]; 12] = [
    [-X, Z, 0.0],
    [X, Z, 0.0],
    [-X, -Z, 0.0],
    [X, -Z, 0.0],
    [0.0, -X, Z],
    [0.0, X, Z],
    [0.0, -X, -Z],
    [0.0, X, -Z],
    [Z, 0.0, -X],
    [Z, 0.0, X],
    [-Z, 0.0, -X],
    [-Z, 0.0, X],
];

const FACES: [[usize; 3]; 20] = [
    [0, 11, 5],
    [0, 5, 1],
    [0, 1, 7],
    [0, 7, 10],
    [0, 10, 11],
    [1, 5, 9],
    [5, 11, 4],
    [11, 10, 2],
    [10, 7, 6],
    [7, 1, 8],
    [3, 9, 4],
    [3, 4, 2],
    [3, 2, 6],
    [3, 6, 8],
    [3, 8, 9],
    [4, 9, 5],
    [2, 4, 11],
    [6, 2, 10],
    [8, 6, 7],
    [9, 8, 1],
];

const fn vertex(position: [f32; 3], normal: [f32; 3]) -> Vertex {
    Vertex {
        position,
        normal,
    }
}

pub struct IsoSphere {
    vertices: Vec<Vertex>,
}

impl IsoSphere {
    pub fn new() -> Self {
        let vertices = FACES.into_iter()
            .fold(Vec::with_capacity(FACES.len() * 3), |mut vec, &[i0, i1, i2]| {
                let v0 = VERTICES[i0];
                let v1 = VERTICES[i1];
                let v2 = VERTICES[i2];

                let face_normal = Vec3::cross(&v0.into(), v1.into());
                
                vec.extend_from_slice(&[
                    vertex(v0, face_normal.into()),
                    vertex(v1, face_normal.into()),
                    vertex(v2, face_normal.into()),
                ]);

                vec
            });

        Self {
            vertices,
        }
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }
}