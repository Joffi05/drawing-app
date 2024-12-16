use macroquad::{
    color::{Color, BLACK},
    math::{vec2, Vec2, Vec3},
    models::Mesh,
    ui::Vertex,
};
use crate::Stroke;

pub(crate) fn perpendicular_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ap = p - a;
    let ab = b - a;
    let ab_length = ab.length();
    if ab_length == 0.0 {
        return ap.length();
    }
    let proj = ap.dot(ab) / (ab_length * ab_length);
    let closest = a + ab * proj;
    (p - closest).length()
}

pub(crate) fn ramer_douglas_peucker(points: &[(Vec2, f32)], epsilon: f32) -> Vec<(Vec2, f32)> {
    if points.len() < 3 {
        return points.to_vec();
    }

    let (first, last) = (points.first().unwrap().0, points.last().unwrap().0);
    let mut max_dist = 0.0;
    let mut index = 0;

    for (i, &(p, _)) in points.iter().enumerate().skip(1).take(points.len()-2) {
        let dist = perpendicular_distance(p, first, last);
        if dist > max_dist {
            max_dist = dist;
            index = i;
        }
    }

    if max_dist > epsilon {
        let left = ramer_douglas_peucker(&points[..=index], epsilon);
        let right = ramer_douglas_peucker(&points[index..], epsilon);

        let mut result = left;
        result.pop();
        result.extend(right);
        result
    } else {
        vec![points[0], points[points.len()-1]]
    }
}

pub(crate) fn interpolate_pressure(r0: f32, r1: f32, r2: f32, r3: f32, t: f32) -> f32 {
    let t2 = t*t;
    let t3 = t2*t;
    0.5 * ((2.0*r1) + (-r0 + r2)*t + (2.0*r0 - 5.0*r1 +4.0*r2 - r3)*t2 + (-r0 +3.0*r1 -3.0*r2 + r3)*t3)
}

pub(crate) fn catmull_rom_spline(points: &[(Vec2, f32)], segments: usize) -> Vec<(Vec2, f32)> {
    if points.len() < 4 {
        return points.to_vec();
    }

    let mut result = Vec::new();
    let mut extended = Vec::new();
    extended.push(points[0]);
    extended.extend_from_slice(points);
    extended.push(*points.last().unwrap());

    for i in 1..(extended.len()-2) {
        let p0 = extended[i-1].0;
        let p1 = extended[i].0;
        let p2 = extended[i+1].0;
        let p3 = extended[i+2].0;

        let r0 = extended[i-1].1;
        let r1 = extended[i].1;
        let r2 = extended[i+1].1;
        let r3 = extended[i+2].1;

        for s in 0..segments {
            let t = s as f32 / (segments as f32);
            let t2 = t*t;
            let t3 = t2*t;

            let px = 0.5 * ((2.0*p1.x) + (-p0.x + p2.x)*t + (2.0*p0.x - 5.0*p1.x +4.0*p2.x - p3.x)*t2 + (-p0.x +3.0*p1.x -3.0*p2.x + p3.x)*t3);
            let py = 0.5 * ((2.0*p1.y) + (-p0.y + p2.y)*t + (2.0*p0.y -5.0*p1.y +4.0*p2.y - p3.y)*t2 + (-p0.y +3.0*p1.y -3.0*p2.y + p3.y)*t3);

            let pos = Vec2::new(px, py);
            let pressure = interpolate_pressure(r0, r1, r2, r3, t);

            result.push((pos, pressure));
        }
    }

    result.push(*points.last().unwrap());

    result
}

pub(crate) fn draw_filled_trapezoid(start: Vec2, start_radius: f32, end: Vec2, end_radius: f32) {
    let direction = (end - start).normalize();
    let perpendicular = Vec2::new(-direction.y, direction.x);

    let start_left = start + perpendicular * start_radius;
    let start_right = start - perpendicular * start_radius;
    let end_left = end + perpendicular * end_radius;
    let end_right = end - perpendicular * end_radius;

    draw_triangle(start_left, end_left, end_right, BLACK);
    draw_triangle(start_left, end_right, start_right, BLACK);
}

fn draw_triangle(p1: Vec2, p2: Vec2, p3: Vec2, color: Color) {
    macroquad::shapes::draw_triangle(p1, p2, p3, color);
}


pub(crate) fn color_u8(color:Color)->[u8;4] {
    [(color.r*255.0)as u8,
     (color.g*255.0)as u8,
     (color.b*255.0)as u8,
     (color.a*255.0)as u8]
}

pub(crate) fn stroke_intersect(stroke: &Stroke, pos: Vec2, radius: f32) -> bool {
    for &(p,_) in &stroke.points {
        if p.distance(pos) <= radius {
            return true;
        }
    }
    false
}

pub(crate) fn stroke_to_screen_mesh_old(points: &[(Vec2, f32)], offset: Vec2, zoom: f32) -> Option<Mesh> {
    if points.len()<2 {
        return None;
    }

    let n=points.len();
    let mut vertices=Vec::with_capacity(n*2);
    let mut indices=Vec::with_capacity((n-1)*6);

    let mut directions=Vec::with_capacity(n);
    for i in 0..n {
        let dir=if i==n-1 {
            let prev=points[i-1].0;
            let curr=points[i].0;
            (curr - prev).normalize()
        } else {
            let curr=points[i].0;
            let next=points[i+1].0;
            (next - curr).normalize()
        };
        directions.push(dir);
    }

    let color=Color::new(0.0,0.0,0.0,1.0);
    let c=color_u8(color);
    let normal=[0.0,0.0,1.0,0.0];

    for i in 0..n {
        let (pos,radius)=points[i];
        // convert to screen coords:
        let sx=(pos.x - offset.x)*zoom;
        let sy=(pos.y - offset.y)*zoom;

        let dir=directions[i];
        let perp=vec2(-dir.y,dir.x);

        let screen_radius = radius * zoom;

        let left_pos=vec2(sx,sy) + perp * screen_radius;
        let right_pos=vec2(sx,sy) - perp * screen_radius;

        vertices.push(Vertex {
            position: Vec3::new(left_pos.x,left_pos.y,0.0),
            uv: Vec2::new(0.0,0.0),
            color:c,
            normal: normal.into(),
        });

        vertices.push(Vertex {
            position: Vec3::new(right_pos.x,right_pos.y,0.0),
            uv: Vec2::new(0.0,0.0),
            color:c,
            normal: normal.into(),
        });
    }

    for i in 0..(n-1) {
        let i0=(i*2)as u16;
        let i1=(i*2+1)as u16;
        let i2=((i+1)*2)as u16;
        let i3=((i+1)*2+1)as u16;

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
        texture:None,
    })
}
