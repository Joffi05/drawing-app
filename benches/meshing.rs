use criterion::{black_box, criterion_group, criterion_main, Criterion};
use macroquad::{color::{Color, BLACK}, color_u8, math::{vec2, Vec2, Vec3}, models::Mesh, ui::Vertex};


/// A helper to create some dummy data for transform_mesh_absolute
fn setup_mesh() -> Mesh {
    // You can adapt this to your real usage.
    let vertices = (0..10_000).map(|i| {
        // Example vertex
        let x = i as f32;
        let y = (i as f32).sin();
        Vertex {
            position: Vec3::new(x, y, 0.0),
            uv:       Vec2::ZERO,
            color:    BLACK.into(),
            normal: [0.0,0.0,0.0,0.0].into(),
        }
    }).collect();

    Mesh {
        vertices,
        indices: Vec::new(),
        texture: None, // or some texture handle
    }
}

pub fn transform_mesh_absolute(
    original_mesh: &Mesh,
    offset: Vec2,
    zoom: f32,
    pivot: Vec2,
) -> Mesh {
    let mut new_vertices = Vec::with_capacity(original_mesh.vertices.len());

    for v in &original_mesh.vertices {
        let mut pos = v.position.truncate();

        pos -= offset;

        pos = pivot + (pos - pivot) * zoom;

        let mut new_vertex = *v;
        new_vertex.position.x = pos.x;
        new_vertex.position.y = pos.y;
        new_vertex.position.z = 0.0;

        new_vertices.push(new_vertex);
    }

    Mesh {
        vertices: new_vertices,
        indices: original_mesh.indices.clone(),
        texture: original_mesh.texture.clone(),
    }
}



const CAP_SEGMENTS: usize = 8; 
fn draw_cap(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    start_left: Vec2,
    start_right: Vec2,
    color: [u8;4],
    normal: [f32;4],
) {
    let center = (start_left + start_right)*0.5;
    let cap_radius = start_left.distance(start_right)*0.5;

    let angle_left = (start_left - center).angle_between(vec2(1.0,0.0));
    let angle_right = (start_right - center).angle_between(vec2(1.0,0.0));

    let mut a0 = angle_left;
    let mut a1 = angle_right;
    if a1 < a0 {
        a1 += std::f32::consts::TAU;
    }

    let arc = a1 - a0;

    if arc > std::f32::consts::PI {
        let temp = a0;
        a0 = a1;
        a1 = temp + std::f32::consts::TAU; 
        let arc2 = a1 - a0;
        if arc2 > std::f32::consts::PI {
            a1 = a0 + std::f32::consts::PI;
        }
    } else {
        a1 = a0 + std::f32::consts::PI;
    }

    let first_cap_index = vertices.len() as u16;
    vertices.push(Vertex {
        position: Vec3::new(center.x, center.y, 0.0),
        uv: Vec2::new(0.0,0.0),
        color,
        normal: normal.into(),
    });

    for j in 0..=CAP_SEGMENTS {
        let t = j as f32 / CAP_SEGMENTS as f32;
        let angle = a0 + t*(a1 - a0);
        let vx = center.x + angle.cos()*cap_radius;
        let vy = center.y + angle.sin()*cap_radius;
        vertices.push(Vertex {
            position: Vec3::new(vx,vy,0.0),
            uv: Vec2::new(0.0,0.0),
            color,
            normal: normal.into(),
        });
    }

    for j in 0..CAP_SEGMENTS {
        let center_i = first_cap_index;
        let v1 = first_cap_index + 1 + j as u16;
        let v2 = first_cap_index + 2 + j as u16;
        indices.push(center_i);
        indices.push(v1);
        indices.push(v2);
    }
}


// ? kp was hier abgeht
pub fn stroke_to_world_submeshes(
    points: &[(Vec2, f32)],
    max_chunk_points: usize
) -> Vec<Mesh> {
    if points.len() < 2 {
        return Vec::new();
    }

    let mut result = Vec::new();
    let n = points.len();
    let mut start = 0;

    while start < n {
        let mut end = (start + max_chunk_points).min(n - 1);
        let is_last_chunk = end == n - 1;

        if !is_last_chunk {
            end += 1;
        }

        let sub_points = &points[start..=end];

        let draw_start_cap = start == 0;
        let draw_end_cap   = end == n - 1;

        let mesh = build_stroke_mesh_chunk(sub_points, draw_start_cap, draw_end_cap);
        result.push(mesh);

        if !is_last_chunk {
            start = end - 1;
        } else {
            // done
            start = end + 1;
        }
    }

    result
}



fn build_stroke_mesh_chunk(
    points: &[(Vec2, f32)],
    draw_start_cap: bool,
    draw_end_cap: bool,
) -> Mesh {
    if points.len() < 2 {
        return Mesh {
            vertices: Vec::new(),
            indices:  Vec::new(),
            texture:  None,
        };
    }

    let n = points.len();
    let mut vertices = Vec::with_capacity(n * 2);
    let mut indices  = Vec::with_capacity((n - 1) * 6);

    // direction array
    let mut directions = Vec::with_capacity(n);
    for i in 0..n {
        let dir = if i == n - 1 {
            // last point
            let prev = points[i - 1].0;
            let curr = points[i].0;
            (curr - prev).normalize()
        } else {
            // any other point
            let curr = points[i].0;
            let nxt  = points[i + 1].0;
            (nxt - curr).normalize()
        };
        directions.push(dir);
    }

    let color  = Color::new(0.0, 0.0, 0.0, 1.0);
    let c      = color.into();
    let normal = [0.0, 0.0, 1.0, 0.0];

    // 2 vertices per stroke point
    for i in 0..n {
        let (pos, radius) = points[i];
        let dir  = directions[i];
        let perp = vec2(-dir.y, dir.x);

        let left_pos  = pos + perp * radius;
        let right_pos = pos - perp * radius;

        vertices.push(Vertex {
            position: Vec3::new(left_pos.x, left_pos.y, 0.0),
            uv:       Vec2::new(0.0, 0.0),
            color:    c,
            normal:   normal.into(),
        });

        vertices.push(Vertex {
            position: Vec3::new(right_pos.x, right_pos.y, 0.0),
            uv:       Vec2::new(0.0, 0.0),
            color:    c,
            normal:   normal.into(),
        });
    }

    // indices 2 triangles per segment
    for i in 0..(n - 1) {
        let i0 = (i * 2) as u16;
        let i1 = (i * 2 + 1) as u16;
        let i2 = ((i + 1) * 2) as u16;
        let i3 = ((i + 1) * 2 + 1) as u16;

        indices.push(i0); indices.push(i1); indices.push(i2);
        indices.push(i2); indices.push(i1); indices.push(i3);
    }

    if draw_start_cap {
        // draw cap at the first segment start
        let start_left  = vertices[0].position.truncate();
        let start_right = vertices[1].position.truncate();
        draw_cap(&mut vertices, &mut indices, start_left, start_right, c, normal);
    }
    if draw_end_cap {
        // draw cap at the final segment end
        let end_left_i  = 2 * (n - 1);
        let end_right_i = 2 * (n - 1) + 1;
        let end_left  = vertices[end_left_i as usize].position.truncate();
        let end_right = vertices[end_right_i as usize].position.truncate();
        draw_cap(&mut vertices, &mut indices, end_left, end_right, c, normal);
    }

    Mesh {
        vertices,
        indices,
        texture: None,
    }
}

/// A helper to create some dummy stroke points for stroke_to_world_submeshes
fn setup_stroke_points() -> Vec<(Vec2, f32)> {
    (0..10_000).map(|i| {
        let x = i as f32;
        let y = (i as f32).cos();
        // (position, thickness/pressure):
        (Vec2::new(x, y), 1.0)
    }).collect()
}

fn bench_transform_mesh_absolute(c: &mut Criterion) {
    // Create some test data
    let mesh = setup_mesh();
    let offset = Vec2::new(100.0, 200.0);
    let zoom   = 2.0;
    let pivot  = Vec2::new(50.0, 50.0);

    // Create a benchmark group
    c.bench_function("transform_mesh_absolute", |b| {
        b.iter(|| {
            // black_box to prevent compiler optimizations removing dead code
            let _res = transform_mesh_absolute(
                black_box(&mesh),
                black_box(offset),
                black_box(zoom),
                black_box(pivot),
            );
        });
    });
}

fn bench_stroke_to_world_submeshes(c: &mut Criterion) {
    // Create some test data
    let points = setup_stroke_points();
    let max_chunk_points = 800;

    c.bench_function("stroke_to_world_submeshes", |b| {
        b.iter(|| {
            let _res = stroke_to_world_submeshes(
                black_box(&points),
                black_box(max_chunk_points),
            );
        });
    });
}


// Combine benchmarks into a group:
criterion_group!(
    name = benches;
    config = Criterion::default();
    targets = bench_transform_mesh_absolute, bench_stroke_to_world_submeshes
);

// Tell Criterion to run the group:
criterion_main!(benches);
