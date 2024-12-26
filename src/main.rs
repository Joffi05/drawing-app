mod read_stylus;
mod utility;
mod command;

use command::{Command, CommandStack};
use macroquad::prelude::*;
use miniquad::window::set_mouse_cursor;
use miniquad::CursorIcon;
use read_stylus::{read_input, StylusEvent};
use rfd::FileDialog;
use serde::{Serialize, Deserialize};
use serde_json::{self};
use std::fs::File;
use std::io::{Write, Read};
use std::sync::mpsc::{self};
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
    offset: [f32;2],
    zoom: f32,
}

#[derive(PartialEq, Serialize, Deserialize, Clone)]
enum ToolMode {
    Pen,
    Eraser,
}

#[derive(Clone)]
struct Stroke {
    points: Vec<(Vec2,f32)>, // world coords
}

impl Stroke {
    fn new() -> Self {
        Self { points: Vec::new() }
    }

    fn add_point(&mut self, pos: Vec2, pressure: f32, zoom: f32) {
        let thickness = (pressure * (1.0 / zoom)).max(0.5);
        self.points.push((pos, thickness));
    }

    fn simplify(&mut self, epsilon: f32) {
        self.points = ramer_douglas_peucker(&self.points, epsilon);
    }
}

impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        self.points == other.points
    }
}

impl From<&Stroke> for StrokeData {
    fn from(stroke: &Stroke) -> Self {
        let points = stroke.points.iter()
            .map(|(pos,th)| ([pos.x,pos.y], *th))
            .collect();
        StrokeData { points }
    }
}

struct InfiniteCanvas {
    strokes: Vec<Stroke>,
    stroke_cache: Vec<Option<Vec<Mesh>>>,
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
        let c= Self {
            strokes:Vec::new(),
            stroke_cache:Vec::new(),
            current_stroke:None,
            command_stack: CommandStack::new(),
            offset:Vec2::ZERO,
            last_offset: Vec2::ZERO,
            zoom:1.0,
            last_zoom: 1.0,
            current_pressure:0.0,
            stylus_btn_1_pressed:false,
            last_btn_1_press:Instant::now()-Duration::from_secs(1),
            tool_mode:ToolMode::Pen,
            last_stylus_screen_pos:None,
        };
        c.update_cursor_icon();
        c
    }

    fn clear(&mut self) {
        self.stroke_cache.clear();
        self.strokes.clear();
    }

    fn toggle_eraser(&mut self) {
        self.tool_mode=if self.tool_mode==ToolMode::Pen {ToolMode::Eraser} else {ToolMode::Pen};
        self.update_cursor_icon();
    }

    fn erase_stroke_at(&mut self, pos: Vec2) {
        let radius=10.0*(1.0/self.zoom);
        let mut i=0;
        while i<self.strokes.len() {
            if stroke_intersect(&self.strokes[i], pos, radius) {
                self.command_stack.push_undo(command::Command::RemoveStroke(self.strokes[i].clone()));
                self.stroke_cache.remove(i);
                self.strokes.remove(i);
            } else {
                i+=1;
            }
        }
    }

    fn update_cursor_icon(&self) {
        match self.tool_mode {
            ToolMode::Pen=> set_mouse_cursor(CursorIcon::Crosshair),
            ToolMode::Eraser=> set_mouse_cursor(CursorIcon::NotAllowed),
        }
    }

    fn finalize_stroke(&mut self) {
        if let Some(mut stroke)=self.current_stroke.take() {
            stroke.simplify(0.5); // optional
            let segments = 10;
            let smoothed = catmull_rom_spline(&stroke.points, segments);
            stroke.points = smoothed;
            self.command_stack.push_undo(command::Command::AddStroke(stroke.clone()));
            self.strokes.push(stroke);
            self.stroke_cache.push(None);
        }
    }

    fn save_to_json(&mut self) {
        let data=CanvasData {
            strokes:self.strokes.iter().map(|s| s.into()).collect(),
            tool_mode:self.tool_mode.clone(),
            offset:[self.offset.x,self.offset.y],
            zoom:self.zoom,
        };

        if let Some(path)=FileDialog::new().add_filter("json",&["json"]).save_file() {
            let json=serde_json::to_string_pretty(&data).unwrap();
            let mut file=File::create(path).unwrap();
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


            // setup cache
            self.stroke_cache.clear();
            for _ in &self.strokes {
                self.stroke_cache.push(None);
            }

            // setup undo-redo stack
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

        let offset_changed = self.offset != self.last_offset;
        let zoom_changed = (self.zoom - self.last_zoom).abs() > f32::EPSILON;

        clear_background(WHITE);

        let a4_w = 595.0;
        let a4_h = 842.0;

        let visible_left = self.offset.x;
        let visible_top = self.offset.y;
        let visible_right = self.offset.x + screen_w/self.zoom;
        let visible_bottom = self.offset.y + screen_h/self.zoom;

        let start_x = (visible_left/a4_w).floor() as i32 - 1;
        let end_x = (visible_right/a4_w).ceil() as i32 + 1;
        let start_y = (visible_top/a4_h).floor() as i32 - 1;
        let end_y = (visible_bottom/a4_h).ceil() as i32 + 1;

        for x in start_x..=end_x {
            for y in start_y..=end_y {
                let page_topleft_world = vec2(x as f32 * a4_w, y as f32 * a4_h);
                let px0 = (page_topleft_world.x - self.offset.x)*self.zoom;
                let py0 = (page_topleft_world.y - self.offset.y)*self.zoom;
                let pw = a4_w * self.zoom;
                let ph = a4_h * self.zoom;

                draw_rectangle_lines(px0, py0, pw, ph, 1.0, Color::new(0.0,0.0,0.0,0.5));
            }
        }

        for (i, stroke) in self.strokes.iter().enumerate() {
            // draw wider
            if is_stroke_visible(stroke, self.offset, self.zoom, screen_w * 3.0, screen_h * 3.0) {
                if self.stroke_cache[i].is_none() {
                    // build submeshes
                    let submeshes = stroke_to_world_submeshes(&stroke.points, 800 /* random number that seems to work, dont want to think about it now */);
                    self.stroke_cache[i] = Some(submeshes);
                }

                if let Some(ref submeshes) = self.stroke_cache[i] {
                    // iter all subs
                    for mesh in submeshes.iter() {
                        let mut screen_mesh = transform_mesh_absolute(
                            mesh,
                            self.offset,
                            self.zoom,
                            vec2(0.0, 0.0),
                        );
                        draw_mesh(&mut screen_mesh);
                    }
                }
            } else {
                self.stroke_cache[i] = None;
            }
        }
        
        if let Some(stroke) = &self.current_stroke {
            for i in 0..stroke.points.len() {
                let (pos, radius) = stroke.points[i];
                let sx = (pos.x - self.offset.x)*self.zoom;
                let sy = (pos.y - self.offset.y)*self.zoom;
                draw_circle(sx, sy, radius*self.zoom, BLACK);

                if i + 1 < stroke.points.len() {
                    let (npos, nr) = stroke.points[i+1];
                    let nsx = (npos.x - self.offset.x)*self.zoom;
                    let nsy = (npos.y - self.offset.y)*self.zoom;
                    draw_filled_trapezoid(vec2(sx,sy), radius*self.zoom, vec2(nsx,nsy), nr*self.zoom);
                }
            }
        }

        if offset_changed || zoom_changed {
            self.last_offset = self.offset;
            self.last_zoom = self.zoom;
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


// building mesh (old)
pub(crate) fn stroke_to_world_mesh(points: &[(Vec2, f32)]) -> Option<Mesh> {
    if points.len() < 2 {
        return None;
    }

    let n = points.len();

    let mut vertices = Vec::with_capacity(n * 2);
    let mut indices = Vec::with_capacity((n - 1) * 6);

    let mut directions = Vec::with_capacity(n);
    for i in 0..n {
        // ? wtf 
        let dir = if i == n - 1 {
            let prev = points[i - 1].0;
            let curr = points[i].0;
            (curr - prev).normalize()
        } else {
            let curr = points[i].0;
            let nxt  = points[i + 1].0;
            (nxt - curr).normalize()
        };
        directions.push(dir);
    }

    let color = Color::new(0.0, 0.0, 0.0, 1.0);
    let c = color_u8(color);
    let normal = [0.0, 0.0, 1.0, 0.0];

    for i in 0..n {
        let (pos, radius) = points[i];
        let dir = directions[i];
        let perp = vec2(-dir.y, dir.x);

        let left_pos = pos + perp * radius;
        let right_pos= pos - perp * radius;

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

        indices.push(i0); indices.push(i1); indices.push(i2);
        indices.push(i2); indices.push(i1); indices.push(i3);
    }

    // draw caps
    {
        let start_left  = vertices[0].position.truncate();
        let start_right = vertices[1].position.truncate();
        draw_cap(&mut vertices, &mut indices, start_left, start_right, c, normal);
    }
    {
        let end_left_i  = 2 * (n - 1);
        let end_right_i = 2 * (n - 1) + 1;
        let end_left  = vertices[end_left_i as usize].position.truncate();
        let end_right = vertices[end_right_i as usize].position.truncate();
        draw_cap(&mut vertices, &mut indices, end_left, end_right, c, normal);
    }

    Some(Mesh {
        vertices,
        indices,
        texture: None,
    })
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
    let c      = color_u8(color);
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


#[macroquad::main("Drawing App")]
async fn main() {
    let (sender,receiver)=mpsc::channel();
    let stylus_device_path="/dev/input/event15".to_string();
    read_input(stylus_device_path, sender);

    let mut canvas=InfiniteCanvas::new();
    let pressure_max=60000.0;
    let double_click_threshold=Duration::from_millis(300);

    loop {
        let screen_pos=vec2(mouse_position().0, mouse_position().1);

        while let Ok(event)=receiver.try_recv() {
            match event {
                StylusEvent::Pressure{value}=>{
                    canvas.current_pressure=(value as f32 / pressure_max)*3.0;
                }
                StylusEvent::Key{key,value}=>{
                    if key==evdev::Key::BTN_STYLUS {
                        if value==1 {
                            let now=Instant::now();
                            if !canvas.stylus_btn_1_pressed {
                                if now.duration_since(canvas.last_btn_1_press)<double_click_threshold {
                                    canvas.toggle_eraser();
                                }
                                canvas.last_btn_1_press=now;
                                canvas.stylus_btn_1_pressed=true;
                                canvas.last_stylus_screen_pos=Some(screen_pos);
                            }
                        } else {
                            canvas.stylus_btn_1_pressed=false;
                            canvas.last_stylus_screen_pos=None;
                        }
                    }
                }
                _=>{}
            }
        }

        if canvas.stylus_btn_1_pressed {
            if let Some(last_pos)=canvas.last_stylus_screen_pos {
                let delta=screen_pos-last_pos;
                canvas.last_offset = canvas.offset;
                canvas.offset -= delta*(1.0/canvas.zoom);
                canvas.last_stylus_screen_pos=Some(screen_pos);
            } else {
                canvas.last_stylus_screen_pos=Some(screen_pos);
            }
        }

        let scroll=mouse_wheel().1;
        if scroll!=0.0 {
            let factor=if scroll>0.0 {1.1}else{0.9};
            canvas.last_zoom = canvas.zoom;
            canvas.zoom*=factor;
            canvas.zoom=canvas.zoom.clamp(0.1,10.0);
        }

        if canvas.current_pressure>0.1 {
            let world_pos=canvas.offset+(screen_pos*(1.0/canvas.zoom));
            match canvas.tool_mode {
                ToolMode::Pen=>{
                    if let Some(stroke)=&mut canvas.current_stroke {
                        stroke.add_point(world_pos, canvas.current_pressure, canvas.zoom);
                    } else {
                        let mut stroke=Stroke::new();
                        stroke.add_point(world_pos, canvas.current_pressure, canvas.zoom);
                        canvas.current_stroke=Some(stroke);
                    }
                }
                ToolMode::Eraser=>{
                    canvas.erase_stroke_at(world_pos);
                }
            }
        } else if canvas.current_pressure<0.1 {
            if canvas.tool_mode==ToolMode::Pen {
                canvas.finalize_stroke();
            }
        }

        if is_key_down(KeyCode::LeftControl)&&is_key_pressed(KeyCode::S) {
            canvas.save_to_json();
        }
        if is_key_down(KeyCode::LeftControl)&&is_key_pressed(KeyCode::O) {
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

