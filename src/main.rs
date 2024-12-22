mod read_stylus;
mod mesh;
mod utility;
mod command;

// Bring the trait into scope so you can call `.to_mesh(...)`
use mesh::Meshable;

use command::{Command, CommandStack};
use macroquad::{math, prelude::*};
use miniquad::window::set_mouse_cursor;
use miniquad::CursorIcon;
use read_stylus::{read_input, StylusEvent};
use rfd::FileDialog;
use serde::{Serialize, Deserialize};
use serde_json::{self};
use std::fs::File;
use std::io::{Write, Read};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};
use utility::*;

#[derive(Serialize, Deserialize)]
struct StrokeData {
    points: Vec<([f32;2], f32)>,
}

#[derive(Serialize, Deserialize)]
struct CanvasData {
    strokes: Vec<StrokeData>,
    tool_mode: ToolMode,
    offset: [f32; 2],
    zoom: f32,
}

#[derive(PartialEq, Serialize, Deserialize, Clone)]
enum ToolMode {
    Pen,
    Eraser,
}

#[derive(Clone)]
pub struct Stroke {
    pub points: Vec<(Vec2, f32)>, // world coords
}

impl Stroke {
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }

    pub fn add_point(&mut self, pos: Vec2, pressure: f32, zoom: f32) {
        // Scale thickness by (1.0 / zoom) so lines remain visually consistent
        let thickness = pressure * (1.0 / zoom);
        //let thickness = (pressure).max(0.5);
        self.points.push((pos, thickness));
    }

    pub fn simplify(&mut self, epsilon: f32) {
        self.points = ramer_douglas_peucker(&self.points, epsilon);
    }
}

// For undo/redo comparisons
impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        self.points == other.points
    }
}

impl From<&Stroke> for StrokeData {
    fn from(stroke: &Stroke) -> Self {
        let points = stroke.points.iter()
            .map(|(pos, th)| ([pos.x, pos.y], *th))
            .collect();
        StrokeData { points }
    }
}

struct InfiniteCanvas {
    strokes: Vec<Stroke>,
    stroke_cache: Vec<Option<Mesh>>,
    current_stroke: Option<Stroke>,
    command_stack: CommandStack,

    offset: Vec2,
    last_offset: Vec2,
    zoom: f32,
    last_zoom: f32,

    current_pressure: f32,
    stylus_btn_1_pressed: bool,
    last_btn_1_press: Instant,
    tool_mode: ToolMode,
    last_stylus_screen_pos: Option<Vec2>,
}

impl InfiniteCanvas {
    fn new() -> Self {
        let mut c = Self {
            strokes: Vec::new(),
            stroke_cache: Vec::new(),
            current_stroke: None,
            command_stack: CommandStack::new(),

            offset: Vec2::ZERO,
            last_offset: Vec2::ZERO,
            zoom: 1.0,
            last_zoom: 1.0,

            current_pressure: 0.0,
            stylus_btn_1_pressed: false,
            last_btn_1_press: Instant::now() - Duration::from_secs(1),
            tool_mode: ToolMode::Pen,
            last_stylus_screen_pos: None,
        };
        c.update_cursor_icon();
        c
    }

    fn clear(&mut self) {
        self.stroke_cache.clear();
        self.strokes.clear();
    }

    fn toggle_eraser(&mut self) {
        self.tool_mode = if self.tool_mode == ToolMode::Pen {
            ToolMode::Eraser
        } else {
            ToolMode::Pen
        };
        self.update_cursor_icon();
    }

    fn erase_stroke_at(&mut self, pos: Vec2) {
        let radius = 10.0 * (1.0 / self.zoom);
        let mut i = 0;
        while i < self.strokes.len() {
            if stroke_intersect(&self.strokes[i], pos, radius) {
                // For undo:
                self.command_stack.push_undo(Command::RemoveStroke(self.strokes[i].clone()));
                // Remove from data + cache
                self.stroke_cache.remove(i);
                self.strokes.remove(i);
            } else {
                i += 1;
            }
        }
    }

    fn update_cursor_icon(&self) {
        match self.tool_mode {
            ToolMode::Pen => set_mouse_cursor(CursorIcon::Crosshair),
            ToolMode::Eraser => set_mouse_cursor(CursorIcon::NotAllowed),
        }
    }

    fn finalize_stroke(&mut self) {
        if let Some(mut stroke) = self.current_stroke.take() {
            // Optional: smoothing or simplifying
            // stroke.simplify(0.4);

            // Example catmull smoothing
            let segments = 10;
            let smoothed = catmull_rom_spline(&stroke.points, segments);
            stroke.points = smoothed;

            self.command_stack.push_undo(Command::AddStroke(stroke.clone()));
            self.strokes.push(stroke);
            self.stroke_cache.push(None);
        }
    }

    fn save_to_json(&mut self) {
        let data = CanvasData {
            strokes: self.strokes.iter().map(|s| s.into()).collect(),
            tool_mode: self.tool_mode.clone(),
            offset: [self.offset.x, self.offset.y],
            zoom: self.zoom,
        };

        if let Some(path) = FileDialog::new().add_filter("json", &["json"]).save_file() {
            let json = serde_json::to_string_pretty(&data).unwrap();
            let mut file = File::create(path).unwrap();
            file.write_all(json.as_bytes()).unwrap();
        }
    }

    fn load_from_json(&mut self) {
        if let Some(path) = FileDialog::new().add_filter("json", &["json"]).pick_file() {
            let mut file = File::open(path).unwrap();
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            let data: CanvasData = serde_json::from_str(&contents).unwrap();

            self.strokes.clear();
            for sd in data.strokes {
                let mut stroke = Stroke::new();
                for (p, press) in sd.points {
                    stroke.points.push((vec2(p[0], p[1]), press));
                }
                self.strokes.push(stroke);
            }

            self.tool_mode = data.tool_mode;
            self.offset = vec2(data.offset[0], data.offset[1]);
            self.zoom = data.zoom;
            self.update_cursor_icon();

            // Rebuild stroke_cache for the new strokes
            self.stroke_cache.clear();
            for _ in &self.strokes {
                self.stroke_cache.push(None);
            }

            // Reset undo/redo
            self.command_stack.clear();
        }
    }

    fn undo(&mut self) {
        if let Some(comm) = self.command_stack.pop_undo() {
            match comm {
                Command::AddStroke(stroke) => {
                    if let Some(idx) = self.strokes.iter().position(|s| *s == stroke) {
                        self.strokes.remove(idx);
                        self.stroke_cache.remove(idx);
                        self.command_stack.push_redo(Command::AddStroke(stroke));
                    }
                }
                Command::RemoveStroke(stroke) => {
                    self.strokes.push(stroke.clone());
                    self.stroke_cache.push(None);
                    self.command_stack.push_redo(Command::RemoveStroke(stroke));
                }
            }
        }
    }

    fn redo(&mut self) {
        if let Some(comm) = self.command_stack.pop_redo() {
            match comm {
                Command::AddStroke(stroke) => {
                    self.strokes.push(stroke.clone());
                    self.stroke_cache.push(None);
                    self.command_stack.push_undo(Command::AddStroke(stroke));
                }
                Command::RemoveStroke(stroke) => {
                    if let Some(idx) = self.strokes.iter().position(|s| *s == stroke) {
                        self.strokes.remove(idx);
                        self.stroke_cache.remove(idx);
                        self.command_stack.push_undo(Command::RemoveStroke(stroke));
                    }
                }
            }
        }
    }

    fn draw(&mut self) {
        let screen_w = screen_width();
        let screen_h = screen_height();

        // Check if offset or zoom changed
        let offset_changed = self.offset != self.last_offset;
        let zoom_changed = (self.zoom - self.last_zoom).abs() > f32::EPSILON;

        // If offset or zoom changed, recache
        if offset_changed || zoom_changed {
            for mesh_opt in &mut self.stroke_cache {
                *mesh_opt = None;
            }
            self.last_offset = self.offset;
            self.last_zoom = self.zoom;
        }

        clear_background(WHITE);

        // Example infinite grid (A4 pages)
        let a4_w = 595.0;
        let a4_h = 842.0;

        let visible_left = self.offset.x;
        let visible_top = self.offset.y;
        let visible_right = self.offset.x + screen_w / self.zoom;
        let visible_bottom = self.offset.y + screen_h / self.zoom;

        let start_x = (visible_left / a4_w).floor() as i32 - 1;
        let end_x   = (visible_right / a4_w).ceil() as i32 + 1;
        let start_y = (visible_top / a4_h).floor() as i32 - 1;
        let end_y   = (visible_bottom / a4_h).ceil() as i32 + 1;

        for x in start_x..=end_x {
            for y in start_y..=end_y {
                let page_topleft_world = vec2(x as f32 * a4_w, y as f32 * a4_h);
                let px0 = (page_topleft_world.x - self.offset.x) * self.zoom;
                let py0 = (page_topleft_world.y - self.offset.y) * self.zoom;
                let pw = a4_w * self.zoom;
                let ph = a4_h * self.zoom;

                draw_rectangle_lines(px0, py0, pw, ph, 1.0, Color::new(0.0, 0.0, 0.0, 0.5));
            }
        }

        // Render visible strokes
        for (i, stroke) in self.strokes.iter().enumerate() {
            if is_stroke_visible(stroke, self.offset, self.zoom, screen_w, screen_h) {
                // If no cached mesh, build one now
                if self.stroke_cache[i].is_none() {
                    // Using the Meshable trait:
                    // stroke.to_mesh(offset, zoom)
                    let built_mesh = stroke.to_mesh(self.offset, self.zoom);
                    //self.stroke_cache[i] = built_mesh;
                    for i in built_mesh {
                        draw_mesh(&i);
                    }
                }

                if let Some(ref mesh) = self.stroke_cache[i] {
                    draw_mesh(mesh);
                } 
            }
        }

        //draw_mesh(self.stroke_cache.iter().filter_map(|x| x.is_some().));

        // Draw current (in-progress) stroke
        if let Some(stroke) = &self.current_stroke {
            for i in 0..stroke.points.len() {
                let (pos, radius) = stroke.points[i];
                let sx = (pos.x - self.offset.x) * self.zoom;
                let sy = (pos.y - self.offset.y) * self.zoom;
                draw_circle(sx, sy, radius * self.zoom, BLACK);

                if i + 1 < stroke.points.len() {
                    let (npos, nr) = stroke.points[i + 1];
                    let nsx = (npos.x - self.offset.x) * self.zoom;
                    let nsy = (npos.y - self.offset.y) * self.zoom;
                    draw_filled_trapezoid(vec2(sx, sy), radius * self.zoom, vec2(nsx, nsy), nr * self.zoom);
                }
            }
        }
    }
}


// kp
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


// create the mesh from the curent stroke
pub(crate) fn stroke_to_screen_mesh(
    points: &[(Vec2, f32)],
    offset: Vec2,
    zoom: f32,
) -> Vec<Mesh> {
    const MAX_POINTS_PER_MESH: usize = 100; // Maximale Anzahl Punkte pro Mesh
    let mut meshes = Vec::new();

    if points.len() < 2 {
        return meshes;
    }

    for chunk in points.chunks(MAX_POINTS_PER_MESH) {
        if chunk.len() < 2 {
            continue;
        }

        let mut vertices = Vec::with_capacity(chunk.len() * 2);
        let mut indices = Vec::with_capacity((chunk.len() - 1) * 6);

        let mut directions = Vec::with_capacity(chunk.len());
        for i in 0..chunk.len() {
            let dir = if i == chunk.len() - 1 {
                let prev = chunk[i - 1].0;
                let curr = chunk[i].0;
                (curr - prev).normalize()
            } else {
                let curr = chunk[i].0;
                let nxt = chunk[i + 1].0;
                (nxt - curr).normalize()
            };
            directions.push(dir);
        }

        let color = Color::new(0.0, 0.0, 0.0, 1.0);
        let c = color_u8(color);
        let normal = [0.0, 0.0, 1.0, 0.0];

        for i in 0..chunk.len() {
            let (pos, radius) = chunk[i];
            let sx = (pos.x - offset.x) * zoom;
            let sy = (pos.y - offset.y) * zoom;

            let dir = directions[i];
            let perp = vec2(-dir.y, dir.x);
            let screen_radius = radius * zoom;

            let left_pos = vec2(sx, sy) + perp * screen_radius;
            let right_pos = vec2(sx, sy) - perp * screen_radius;

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

        for i in 0..(chunk.len() - 1) {
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

        meshes.push(Mesh {
            vertices,
            indices,
            texture: None,
        });
    }

    meshes
}



// The rest of your code, including the macroquad main loop, is unchanged:
#[macroquad::main("Drawing App")]
async fn main() {
    let (sender, receiver) = mpsc::channel();
    let stylus_device_path = "/dev/input/event15".to_string();
    read_input(stylus_device_path, sender);

    let mut canvas = InfiniteCanvas::new();
    let pressure_max = 60000.0;
    let double_click_threshold = Duration::from_millis(300);

    loop {
        let screen_pos = vec2(mouse_position().0, mouse_position().1);

        while let Ok(event) = receiver.try_recv() {
            match event {
                StylusEvent::Pressure { value } => {
                    // example custom pressure curve
                    let max_thickness = 3.5;
                    let min_thickness = 0.0;

                    let alpha = 1.2; // tweak
                    let normalized_pressure = value as f32 / pressure_max;
                    let curve_pressure = normalized_pressure.powf(alpha); 
                    let thickness = curve_pressure * (max_thickness - min_thickness) + min_thickness;
                    canvas.current_pressure = thickness;
                }
                StylusEvent::Key { key, value } => {
                    if key == evdev::Key::BTN_STYLUS {
                        if value == 1 {
                            let now = Instant::now();
                            if !canvas.stylus_btn_1_pressed {
                                if now.duration_since(canvas.last_btn_1_press) < double_click_threshold {
                                    canvas.toggle_eraser();
                                }
                                canvas.last_btn_1_press = now;
                                canvas.stylus_btn_1_pressed = true;
                                canvas.last_stylus_screen_pos = Some(screen_pos);
                            }
                        } else {
                            canvas.stylus_btn_1_pressed = false;
                            canvas.last_stylus_screen_pos = None;
                        }
                    }
                }
                _ => {}
            }
        }

        if canvas.stylus_btn_1_pressed {
            if let Some(last_pos) = canvas.last_stylus_screen_pos {
                let delta = screen_pos - last_pos;
                canvas.last_offset = canvas.offset;
                canvas.offset -= delta * (1.0 / canvas.zoom);
                canvas.last_stylus_screen_pos = Some(screen_pos);
            } else {
                canvas.last_stylus_screen_pos = Some(screen_pos);
            }
        }

        let scroll = mouse_wheel().1;
        if scroll != 0.0 {
            let factor = if scroll > 0.0 { 1.1 } else { 0.9 };
            canvas.last_zoom = canvas.zoom;
            canvas.zoom *= factor;
            canvas.zoom = canvas.zoom.clamp(0.1, 10.0);
        }

        if canvas.current_pressure > 0.1 {
            let world_pos = canvas.offset + (screen_pos * (1.0 / canvas.zoom));
            match canvas.tool_mode {
                ToolMode::Pen => {
                    if let Some(stroke) = &mut canvas.current_stroke {
                        stroke.add_point(world_pos, canvas.current_pressure, canvas.zoom);
                    } else {
                        let mut stroke = Stroke::new();
                        stroke.add_point(world_pos, canvas.current_pressure, canvas.zoom);
                        canvas.current_stroke = Some(stroke);
                    }
                }
                ToolMode::Eraser => {
                    canvas.erase_stroke_at(world_pos);
                }
            }
        } else if canvas.current_pressure < 0.1 {
            if canvas.tool_mode == ToolMode::Pen {
                canvas.finalize_stroke();
            }
        }

        if is_key_down(KeyCode::LeftControl) && is_key_pressed(KeyCode::S) {
            canvas.save_to_json();
        }
        if is_key_down(KeyCode::LeftControl) && is_key_pressed(KeyCode::O) {
            canvas.load_from_json();
        }
        if is_key_down(KeyCode::LeftControl) && is_key_pressed(KeyCode::Z) {
            canvas.undo();
        }
        if is_key_down(KeyCode::LeftControl) && is_key_pressed(KeyCode::R) {
            canvas.redo();
        }

        canvas.draw();

        if is_key_pressed(KeyCode::C) {
            canvas.clear();
        }

        next_frame().await;
    }
}
