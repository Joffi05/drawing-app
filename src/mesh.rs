use macroquad::prelude::*;

fn color_u8(color: Color) -> [u8;4] {
    let r = (color.r * 255.0) as u8;
    let g = (color.g * 255.0) as u8;
    let b = (color.b * 255.0) as u8;
    let a = (color.a * 255.0) as u8;
    [r,g,b,a]
}

pub fn stroke_to_mesh(points: &[(Vec2, f32)]) -> Option<Mesh> {
    if points.len() < 2 {
        return None;
    }

    let n = points.len();
    let mut vertices = Vec::with_capacity(n * 2);
    let mut indices = Vec::with_capacity((n - 1) * 6);

    // Directions
    let mut directions = Vec::with_capacity(n);
    for i in 0..n {
        let dir = if i == n - 1 {
            let prev = points[i - 1].0;
            let curr = points[i].0;
            (curr - prev).normalize()
        } else {
            let curr = points[i].0;
            let next = points[i + 1].0;
            (next - curr).normalize()
        };
        directions.push(dir);
    }

    let color = Color::new(0.0, 0.0, 0.0, 1.0);
    let c = color_u8(color);
    let normal = [0.0, 0.0, 0.0, 0.0]; // normal facing out of screen

    for i in 0..n {
        let pos = points[i].0;
        let radius = points[i].1;
        let dir = directions[i];
        let perp = vec2(-dir.y, dir.x);

        let left_pos = pos + perp * radius;
        let right_pos = pos - perp * radius;

        vertices.push(Vertex {
            position: Vec3::new(left_pos.x, left_pos.y, 0.0),
            uv: Vec2::new(0.0, 0.0),
            color: c,
            normal: normal.into(),
        });

        vertices.push(Vertex {
            position: Vec3::new(right_pos.x, right_pos.y, 0.0),
            uv: Vec2::new(0.0, 0.0),
            color: c,
            normal: normal.into(),
        });
    }

    for i in 0..(n - 1) {
        let i0 = (i * 2) as u16;
        let i1 = (i * 2 + 1) as u16;
        let i2 = ((i + 1) * 2) as u16;
        let i3 = ((i + 1) * 2 + 1) as u16;

        indices.push(i0);
        indices.push(i1);
        indices.push(i2);

        indices.push(i2);
        indices.push(i1);
        indices.push(i3);
    }

    Some(Mesh {
        vertices,
        indices,
        texture: None,
    })
}
