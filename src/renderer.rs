//! Рендерер: один шейдер, одна пачка треугольников, три текстуры.
//!
//! Всё, что видно — панели, текст, шахматка прозрачности и сам холст —
//! это однотипные квады с uv, цветом и номером текстуры (0 — холст,
//! 1 — узор, 2 — атлас глифов), поэтому кадр рисуется одним вызовом.

use ::gl::types::{GLint, GLuint};

/// Квад в пиксельных координатах окна (левый верхний — 0,0).
#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub color: [f32; 4],
    pub unit: u8,
    pub clip: [f32; 4],
}

impl Item {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4], unit: u8, clip: [f32; 4]) -> Self {
        Self { x0, y0, x1, y1, u0: 0.0, v0: 0.0, u1: 1.0, v1: 1.0, color, unit, clip }
    }
}

/// Треугольник (для линий, окружностей и прочих наклонных фигур интерфейса).
#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub p: [[f32; 2]; 3],
    pub color: [f32; 4],
    pub clip: [f32; 4],
}

/// Слои отрисовки. Порядок важен: фон → шахматка → холст → плавающий фрагмент
/// → панели → миниатюры слоёв → штрихи → текст.
pub const UNIT_BG: u8 = 0;
pub const UNIT_CHECKER: u8 = 1;
pub const UNIT_CANVAS: u8 = 2;
pub const UNIT_SOLID: u8 = 3;
pub const UNIT_ATLAS: u8 = 4;
pub const UNIT_THUMBS: u8 = 5;
pub const UNIT_FLOAT: u8 = 6;
pub const PASSES: [u8; 7] = [
    UNIT_BG, UNIT_CHECKER, UNIT_CANVAS, UNIT_FLOAT, UNIT_SOLID, UNIT_THUMBS, UNIT_ATLAS,
];

/// Атлас миниатюр: сетка 8×8 ячеек по 64 px (до 64 слоёв).
pub const THUMB_CELL: usize = 64;
pub const THUMB_COLS: usize = 8;
pub const THUMB_ATLAS: usize = THUMB_CELL * THUMB_COLS;

const VS: &str = r#"#version 330 core
layout(location=0) in vec2 a_pos;
layout(location=1) in vec2 a_uv;
layout(location=2) in vec4 a_color;
out vec2 v_uv;
out vec4 v_color;
void main() {
    v_uv = a_uv;
    v_color = a_color;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const FS: &str = r#"#version 330 core
in vec2 v_uv;
in vec4 v_color;
uniform sampler2D u_tex;
uniform float u_alpha_mask;
out vec4 frag;
void main() {
    vec4 t = texture(u_tex, v_uv);
    if (u_alpha_mask > 0.5) {
        // Атлас глифов: одноканальное покрытие умножаем на прозрачность цвета.
        if (t.r < 0.004) { discard; }
        frag = vec4(v_color.rgb, v_color.a * t.r);
    } else {
        frag = t * v_color;
    }
}
"#;

pub struct Renderer {
    prog: GLuint,
    vao: GLuint,
    vbo: GLuint,
    tex_canvas: GLuint,
    tex_pattern: GLuint,
    tex_atlas: GLuint,
    tex_white: GLuint,
    tex_thumbs: GLuint,
    tex_float: GLuint,
    u_tex: GLint,
    u_alpha_mask: GLint,
    verts: Vec<f32>,
    pub width: i32,
    pub height: i32,
}

impl Renderer {
    pub fn new(width: i32, height: i32) -> Self {
        let (vs, fs) = (compile(::gl::VERTEX_SHADER, VS), compile(::gl::FRAGMENT_SHADER, FS));
        unsafe {
            let prog = ::gl::CreateProgram();
            ::gl::AttachShader(prog, vs);
            ::gl::AttachShader(prog, fs);
            ::gl::LinkProgram(prog);
            let mut ok = 0;
            ::gl::GetProgramiv(prog, ::gl::LINK_STATUS, &mut ok);
            assert!(ok == 1, "шейдер не слинковался");

            let mut vao = 0;
            ::gl::GenVertexArrays(1, &mut vao);
            ::gl::BindVertexArray(vao);
            let mut vbo = 0;
            ::gl::GenBuffers(1, &mut vbo);
            ::gl::BindBuffer(::gl::ARRAY_BUFFER, vbo);
            let stride = 8 * 4;
            ::gl::EnableVertexAttribArray(0);
            ::gl::VertexAttribPointer(0, 2, ::gl::FLOAT, ::gl::FALSE, stride, std::ptr::null());
            ::gl::EnableVertexAttribArray(1);
            ::gl::VertexAttribPointer(1, 2, ::gl::FLOAT, ::gl::FALSE, stride, 8usize as *const _);
            ::gl::EnableVertexAttribArray(2);
            ::gl::VertexAttribPointer(2, 4, ::gl::FLOAT, ::gl::FALSE, stride, 16usize as *const _);

            let mut tex_canvas = 0;
            ::gl::GenTextures(1, &mut tex_canvas);
            let mut tex_pattern = 0;
            ::gl::GenTextures(1, &mut tex_pattern);
            let mut tex_atlas = 0;
            ::gl::GenTextures(1, &mut tex_atlas);
            let mut tex_white = 0;
            ::gl::GenTextures(1, &mut tex_white);
            let mut tex_thumbs = 0;
            ::gl::GenTextures(1, &mut tex_thumbs);
            let mut tex_float = 0;
            ::gl::GenTextures(1, &mut tex_float);

            let r = Self {
                prog,
                vao,
                vbo,
                tex_canvas,
                tex_pattern,
                tex_atlas,
                tex_white,
                tex_thumbs,
                tex_float,
                u_tex: get_uniform(prog, "u_tex"),
                u_alpha_mask: get_uniform(prog, "u_alpha_mask"),
                verts: Vec::with_capacity(64 * 1024),
                width,
                height,
            };
            r.init_canvas();
            r.init_pattern();
            r.init_atlas();
            r.init_white();
            r.init_thumbs();
            r.init_state();
            r
        }
    }

    fn init_state(&self) {
        unsafe {
            ::gl::Disable(::gl::DEPTH_TEST);
            ::gl::Disable(::gl::CULL_FACE);
            ::gl::Enable(::gl::BLEND);
            ::gl::BlendFuncSeparate(::gl::SRC_ALPHA, ::gl::ONE_MINUS_SRC_ALPHA, ::gl::ONE, ::gl::ONE_MINUS_SRC_ALPHA);
            ::gl::PixelStorei(::gl::UNPACK_ALIGNMENT, 1);
        }
    }

    fn init_canvas(&self) {
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_canvas);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MIN_FILTER, ::gl::LINEAR as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MAG_FILTER, ::gl::LINEAR as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_WRAP_S, ::gl::CLAMP_TO_EDGE as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_WRAP_T, ::gl::CLAMP_TO_EDGE as i32);
            // Заглушка 1×1, чтобы первый кадр ничего не читал из пустой памяти.
            let white: [u8; 4] = [255, 255, 255, 255];
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::RGBA8 as i32, 1, 1, 0,
                ::gl::RGBA, ::gl::UNSIGNED_BYTE, white.as_ptr() as *const _,
            );
        }
    }

    fn init_pattern(&self) {
        // Шахматка прозрачности: 16×16, две клетки по 8 px, режим REPEAT.
        let mut px = vec![0u8; 16 * 16 * 4];
        for y in 0..16 {
            for x in 0..16 {
                let light = ((x / 8) + (y / 8)) % 2 == 0;
                let v = if light { 0xE6u8 } else { 0xC8u8 };
                let i = (y * 16 + x) * 4;
                px[i] = v;
                px[i + 1] = v;
                px[i + 2] = v;
                px[i + 3] = 255;
            }
        }
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_pattern);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MIN_FILTER, ::gl::NEAREST as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MAG_FILTER, ::gl::NEAREST as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_WRAP_S, ::gl::REPEAT as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_WRAP_T, ::gl::REPEAT as i32);
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::RGBA8 as i32, 16, 16, 0,
                ::gl::RGBA, ::gl::UNSIGNED_BYTE, px.as_ptr() as *const _,
            );
        }
    }

    fn init_atlas(&self) {
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_atlas);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MIN_FILTER, ::gl::LINEAR as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MAG_FILTER, ::gl::LINEAR as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_WRAP_S, ::gl::CLAMP_TO_EDGE as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_WRAP_T, ::gl::CLAMP_TO_EDGE as i32);
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::R8 as i32, 2048, 2048, 0,
                ::gl::RED, ::gl::UNSIGNED_BYTE, std::ptr::null(),
            );
        }
    }

    /// Белая текстура 1×1: заливка панелей и кнопок идёт через неё,
    /// поэтому цвет квада попадает на экран без искажений.
    fn init_white(&self) {
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_white);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MIN_FILTER, ::gl::NEAREST as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MAG_FILTER, ::gl::NEAREST as i32);
            let white: [u8; 4] = [255, 255, 255, 255];
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::RGBA8 as i32, 1, 1, 0,
                ::gl::RGBA, ::gl::UNSIGNED_BYTE, white.as_ptr() as *const _,
            );
        }
    }

    /// Текстура миниатюр слоёв: сетка ячеек THUMB_CELL×THUMB_CELL.
    fn init_thumbs(&self) {
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_thumbs);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MIN_FILTER, ::gl::LINEAR as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MAG_FILTER, ::gl::LINEAR as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_WRAP_S, ::gl::CLAMP_TO_EDGE as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_WRAP_T, ::gl::CLAMP_TO_EDGE as i32);
            let blank = vec![0u8; THUMB_ATLAS * THUMB_ATLAS * 4];
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::RGBA8 as i32, THUMB_ATLAS as i32, THUMB_ATLAS as i32, 0,
                ::gl::RGBA, ::gl::UNSIGNED_BYTE, blank.as_ptr() as *const _,
            );
        }
    }

    pub fn update_thumbs(&mut self, rgba: &[u8]) {
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_thumbs);
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::RGBA8 as i32, THUMB_ATLAS as i32, THUMB_ATLAS as i32, 0,
                ::gl::RGBA, ::gl::UNSIGNED_BYTE, rgba.as_ptr() as *const _,
            );
        }
    }

    /// Загружает «плавающий» фрагмент (буфер вставки) в свою текстуру.
    pub fn update_float(&mut self, w: usize, h: usize, rgba: &[u8]) {
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_float);
            // NEAREST сохраняет резкие пиксели фрагмента при масштабе холста,
            // LINEAR мыльно бы штрихи при увеличении.
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MIN_FILTER, ::gl::NEAREST as i32);
            ::gl::TexParameteri(::gl::TEXTURE_2D, ::gl::TEXTURE_MAG_FILTER, ::gl::NEAREST as i32);
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::RGBA8 as i32, w as i32, h as i32, 0,
                ::gl::RGBA, ::gl::UNSIGNED_BYTE, rgba.as_ptr() as *const _,
            );
        }
    }

    pub fn update_canvas(&mut self, w: usize, h: usize, rgba: &[u8]) {
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_canvas);
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::RGBA8 as i32, w as i32, h as i32, 0,
                ::gl::RGBA, ::gl::UNSIGNED_BYTE, rgba.as_ptr() as *const _,
            );
        }
    }

    pub fn update_atlas(&mut self, data: &[u8], w: usize, h: usize) {
        unsafe {
            ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_atlas);
            ::gl::TexImage2D(
                ::gl::TEXTURE_2D, 0, ::gl::R8 as i32, w as i32, h as i32, 0,
                ::gl::RED, ::gl::UNSIGNED_BYTE, data.as_ptr() as *const _,
            );
        }
    }

    /// Рисует список квадов в пять проходов (по одной текстуре на проход),
    /// затем отдельным проходом — треугольники.
    pub fn draw(&mut self, items: &[Item], tris: &[Tri], w: i32, h: i32) {
        self.width = w;
        self.height = h;
        unsafe {
            ::gl::UseProgram(self.prog);
            ::gl::BindVertexArray(self.vao);
            ::gl::BindBuffer(::gl::ARRAY_BUFFER, self.vbo);
            ::gl::ActiveTexture(::gl::TEXTURE0);
        }
        for &unit in PASSES.iter() {
            let tex = match unit {
                UNIT_BG | UNIT_SOLID => self.tex_white,
                UNIT_CHECKER => self.tex_pattern,
                UNIT_CANVAS => self.tex_canvas,
                UNIT_THUMBS => self.tex_thumbs,
                UNIT_FLOAT => self.tex_float,
                _ => self.tex_atlas,
            };
            let alpha_mask = if unit == UNIT_ATLAS { 1.0 } else { 0.0 };
            unsafe {
                ::gl::BindTexture(::gl::TEXTURE_2D, tex);
                ::gl::Uniform1i(self.u_tex, 0);
                ::gl::Uniform1f(self.u_alpha_mask, alpha_mask);
            }
            self.verts.clear();
            let mut cur_clip = [f32::NAN; 4];
            let mut any = false;
            for it in items.iter().filter(|i| i.unit == unit) {
                if it.clip != cur_clip {
                    self.flush();
                    cur_clip = it.clip;
                    self.set_scissor(&cur_clip, w, h);
                }
                self.push_quad(it, w, h);
                any = true;
            }
            if any {
                self.flush();
            }
        }

        // Треугольники: та же белая текстура, сплошная заливка.
        if !tris.is_empty() {
            unsafe {
                ::gl::BindTexture(::gl::TEXTURE_2D, self.tex_white);
                ::gl::Uniform1i(self.u_tex, 0);
                ::gl::Uniform1f(self.u_alpha_mask, 0.0);
            }
            self.verts.clear();
            let mut cur_clip = [f32::NAN; 4];
            for t in tris {
                if t.clip != cur_clip {
                    self.flush();
                    cur_clip = t.clip;
                    self.set_scissor(&cur_clip, w, h);
                }
                let (r, g, b, a) = (t.color[0], t.color[1], t.color[2], t.color[3]);
                for (i, p) in t.p.iter().enumerate() {
                    self.verts.push(p[0] / w as f32 * 2.0 - 1.0);
                    self.verts.push(1.0 - p[1] / h as f32 * 2.0);
                    self.verts.push([0.0, 1.0, 0.0, 0.0][i]);
                    self.verts.push([0.0, 0.0, 1.0, 0.0][i]);
                    self.verts.push(r);
                    self.verts.push(g);
                    self.verts.push(b);
                    self.verts.push(a);
                }
            }
            self.flush();
        }
    }

    fn set_scissor(&self, clip: &[f32; 4], w: i32, h: i32) {
        let x0 = clip[0].max(0.0).floor() as i32;
        let y0 = clip[1].max(0.0).floor() as i32;
        let x1 = clip[2].min(w as f32).ceil() as i32;
        let y1 = clip[3].min(h as f32).ceil() as i32;
        unsafe {
            ::gl::Enable(::gl::SCISSOR_TEST);
            // В GL начало координат снизу, у нас сверху.
            ::gl::Scissor(x0, h - y1, (x1 - x0).max(0), (y1 - y0).max(0));
        }
    }

    fn push_quad(&mut self, it: &Item, w: i32, h: i32) {
        let nx = |x: f32| x / w as f32 * 2.0 - 1.0;
        let ny = |y: f32| 1.0 - y / h as f32 * 2.0;
        let (x0, y0, x1, y1) = (nx(it.x0), ny(it.y0), nx(it.x1), ny(it.y1));
        let (u0, v0, u1, v1) = (it.u0, it.v0, it.u1, it.v1);
        let (r, g, b, a) = (it.color[0], it.color[1], it.color[2], it.color[3]);
        let v = &mut self.verts;
        for (px, py, pu, pv) in [(x0, y0, u0, v0), (x1, y0, u1, v0), (x1, y1, u1, v1), (x0, y0, u0, v0), (x1, y1, u1, v1), (x0, y1, u0, v1)] {
            v.push(px);
            v.push(py);
            v.push(pu);
            v.push(pv);
            v.push(r);
            v.push(g);
            v.push(b);
            v.push(a);
        }
    }

    fn flush(&mut self) {
        if self.verts.is_empty() {
            return;
        }
        unsafe {
            ::gl::BufferData(
                ::gl::ARRAY_BUFFER,
                (self.verts.len() * 4) as ::gl::types::GLsizeiptr,
                self.verts.as_ptr() as *const _,
                ::gl::STREAM_DRAW,
            );
            ::gl::DrawArrays(::gl::TRIANGLES, 0, self.verts.len() as GLint / 8);
        }
        self.verts.clear();
    }
}

/// Имя uniform'а обязательно должно быть NUL-терминированной строкой C,
/// иначе GetUniformLocation возвращает -1 и сэмплер молча уезжает на 0 юнит.
fn get_uniform(prog: GLuint, name: &str) -> GLint {
    let c = std::ffi::CString::new(name).unwrap();
    unsafe { ::gl::GetUniformLocation(prog, c.as_ptr()) }
}

fn compile(kind: ::gl::types::GLenum, src: &str) -> GLuint {
    unsafe {
        let sh = ::gl::CreateShader(kind);
        let len = src.len() as ::gl::types::GLint;
        let ptr = src.as_ptr() as *const i8;
        ::gl::ShaderSource(sh, 1, &ptr, &len);
        ::gl::CompileShader(sh);
        let mut ok = 0;
        ::gl::GetShaderiv(sh, ::gl::COMPILE_STATUS, &mut ok);
        if ok != 1 {
            let mut len = 0;
            ::gl::GetShaderiv(sh, ::gl::INFO_LOG_LENGTH, &mut len);
            let mut log = vec![0u8; len.max(1) as usize];
            ::gl::GetShaderInfoLog(sh, len, std::ptr::null_mut(), log.as_mut_ptr() as *mut _);
            panic!("шейдер: {}", String::from_utf8_lossy(&log));
        }
        sh
    }
}

