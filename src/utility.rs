use macroquad::{
    color::{Color, BLACK},
    math::{vec2, vec3, Vec2, Vec3, Vec3Swizzles},
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

pub(crate) fn stroke_bounding_box(points: &[(Vec2, f32)]) -> (f32, f32, f32, f32) {
    let mut min_x = std::f32::MAX;
    let mut max_x = std::f32::MIN;
    let mut min_y = std::f32::MAX;
    let mut max_y = std::f32::MIN;
    for (pos, _) in points {
        if pos.x < min_x { min_x = pos.x; }
        if pos.x > max_x { max_x = pos.x; }
        if pos.y < min_y { min_y = pos.y; }
        if pos.y > max_y { max_y = pos.y; }
    }
    (min_x, max_x, min_y, max_y)
}


pub(crate) fn is_stroke_visible(stroke: &Stroke, offset: Vec2, zoom: f32, screen_w: f32, screen_h: f32) -> bool {
    let (min_x, max_x, min_y, max_y) = stroke_bounding_box(&stroke.points);
    let visible_left = offset.x;
    let visible_top = offset.y;
    let visible_right = offset.x + screen_w/zoom;
    let visible_bottom = offset.y + screen_h/zoom;

    // aabb
    !(max_x < visible_left || min_x > visible_right || max_y < visible_top || min_y > visible_bottom)
}


pub(crate) fn transform_mesh_o(
    original_mesh: &Mesh,
    transformable: &mut Mesh,
    offset_delta: Vec2,
    zoom_delta: f32,
    zoom_center: Vec2,
    current_zoom: f32,
) {
    for (original_vertex, transformable_vertex) in original_mesh.vertices.iter().zip(&mut transformable.vertices) {
        // Reset transformable vertex to the original vertex position
        transformable_vertex.position = original_vertex.position;

        // Apply translation (offset) scaled by the current zoom
        transformable_vertex.position.x -= offset_delta.x * current_zoom;
        transformable_vertex.position.y -= offset_delta.y * current_zoom;

        // Apply scaling relative to the zoom center
        transformable_vertex.position.x = zoom_center.x
            + (transformable_vertex.position.x - zoom_center.x) * zoom_delta;
        transformable_vertex.position.y = zoom_center.y
            + (transformable_vertex.position.y - zoom_center.y) * zoom_delta;
    }
}


/// Takes a world-space mesh and returns a *new* mesh in screen-space
/// by applying (offset, zoom, pivot) as an *absolute* transform.
pub fn transform_mesh_absolute(
    original_mesh: &Mesh,
    offset: Vec2,
    zoom: f32,
    pivot: Vec2,
) -> Mesh {
    let mut new_vertices = Vec::with_capacity(original_mesh.vertices.len());

    for v in &original_mesh.vertices {
        let mut pos = v.position.truncate(); // world coords (x,y)

        // Step 1: subtract "camera" offset in world space
        pos -= offset;

        // Step 2: apply pivot if desired
        // For example, if pivot != (0, 0):
        //   pos = pivot + (pos - pivot) * zoom;
        //
        // For simplicity (pivot = 0,0) you could do just `pos *= zoom;`.
        // Below we show the pivot-based approach:
        pos = pivot + (pos - pivot) * zoom;

        // Build a new vertex with the transformed position.
        let mut new_vertex = *v;
        new_vertex.position.x = pos.x;
        new_vertex.position.y = pos.y;
        new_vertex.position.z = 0.0; // (assuming 2D)

        new_vertices.push(new_vertex);
    }

    Mesh {
        vertices: new_vertices,
        indices: original_mesh.indices.clone(),
        texture: original_mesh.texture.clone(),
    }
}



pub(crate) fn transform_mesh(
    mesh: &mut Mesh,
    offset_delta: Vec2,
    zoom_delta: f32,
    zoom_center: Vec2,
    current_zoom: f32,
) {
    for vertex in &mut mesh.vertices {
        // Translate positions by the scaled offset delta
        vertex.position.x -= offset_delta.x * current_zoom;
        vertex.position.y -= offset_delta.y * current_zoom;

        // Scale positions relative to the zoom center
        vertex.position.x = zoom_center.x + (vertex.position.x - zoom_center.x) * zoom_delta;
        vertex.position.y = zoom_center.y + (vertex.position.y - zoom_center.y) * zoom_delta;
    }
}