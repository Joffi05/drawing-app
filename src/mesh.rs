use macroquad::prelude::*;
use crate::Stroke; // Make sure the path to your Stroke struct is correct

/// A trait for types that can be turned into a single `Mesh` for drawing.
pub trait Meshable {
    /// Convert `self` into an Option<Mesh>, applying the given `offset` and `zoom`.
    /// Returns `None` if no valid mesh can be built (e.g., not enough points).
    fn to_mesh(&self, offset: Vec2, zoom: f32) -> Vec<Mesh>;
}

impl Meshable for Stroke {
    fn to_mesh(&self, offset: Vec2, zoom: f32) -> Vec<Mesh> {
        // Use your existing stroke_to_screen_mesh to build the mesh
        crate::stroke_to_screen_mesh(&self.points, offset, zoom)
    }
}

impl Meshable for Vec<Mesh> {
    fn to_mesh(&self, _offset: Vec2, _zoom: f32) -> Vec<Mesh> {
        // Combine all meshes in `self` into one single mesh
        // to minimize draw calls.
        if self.is_empty() {
            return vec![];
        }

        let mut combined_vertices = Vec::new();
        let mut combined_indices = Vec::new();
        let mut vertex_offset: u16 = 0;

        for sub_mesh in self {
            // Append vertices
            combined_vertices.extend_from_slice(&sub_mesh.vertices);

            // Append indices with a shift to account for all previously added vertices
            for &idx in &sub_mesh.indices {
                combined_indices.push(idx + vertex_offset);
            }

            // Increase the offset by the number of vertices just added
            vertex_offset += sub_mesh.vertices.len() as u16;
        }

        // If after combining we have an empty mesh, return None
        if combined_vertices.is_empty() || combined_indices.is_empty() {
            return vec![];
        }

        vec![Mesh {
            vertices: combined_vertices,
            indices: combined_indices,
            texture: None,
        }]
    }
}
