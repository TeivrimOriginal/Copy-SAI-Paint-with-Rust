//! Immediate-mode UI на собственном OpenGL-рендерере.
//!
//! Виджеты не хранят состояние, кроме идентификаторов наведения/перетаскивания
//! и открытых выпадающих списков: каждый кадр панели собираются заново и
//! сразу превращаются в список квадов (`gl::Item`).

use crate::renderer::{Item, Tri, UNIT_ATLAS, UNIT_BG, UNIT_CANVAS, UNIT_CHECKER, UNIT_SOLID, UNIT_THUMBS};
use crate::text::{Fonts, Weight};

pub type Rect = [f32; 4];
pub type Color = [f32; 4];

/// Тёмная тема в духе Clip Studio.
pub mod theme {
    use super::Color;
    pub const BG: Color = [0.105, 0.105, 0.117, 1.0];
    pub const PANEL: Color = [0.149, 0.149, 0.165, 1.0];
    pub const PANEL_HI: Color = [0.192, 0.192, 0.212, 1.0];
    pub const FIELD: Color = [0.235, 0.235, 0.259, 1.0];
    pub const BORDER: Color = [0.286, 0.286, 0.318, 1.0];
    pub const TEXT: Color = [0.902, 0.902, 0.925, 1.0];
    pub const TEXT_DIM: Color = [0.596, 0.596, 0.635, 1.0];
    pub const ACCENT: Color = [0.153, 0.541, 0.898, 1.0];
    pub const ACCENT_DIM: Color = [0.106, 0.376, 0.639, 1.0];
    pub const CANVAS_BG: Color = [0.078, 0.078, 0.086, 1.0];
}

pub const PAD: f32 = 8.0;
pub const FONT_UI: f32 = 13.0;
pub const FONT_SMALL: f32 = 11.0;

/// Событие клавиши, передаваемое в виджеты ввода текста.
pub enum KeyEv {
    Char(char),
    Backspace,
    Delete,
    Enter,
    Escape,
}

pub struct Ui {
    pub fonts: Fonts,
    pub items: Vec<Item>,
    pub tris: Vec<Tri>,
    pub mouse: (f32, f32),
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
    pub wheel: f32,
    pub keys: Vec<KeyEv>,
    hot: u32,
    active: u32,
    next_id: u32,
    clip: Rect,
    clips: Vec<Rect>,
    frame_no: u32,
    pressed_x: f32,
    pub open_menu: Option<u32>,
    pub focus: Option<u32>,
    pub consumed_click: bool,
    pub tooltip: Option<(f32, f32, String)>,
    scratch: Vec<([f32; 4], [f32; 4], f32)>,
}

impl Ui {
    pub fn new(fonts: Fonts) -> Self {
        Self {
            fonts,
            items: Vec::with_capacity(8192),
            tris: Vec::with_capacity(2048),
            mouse: (0.0, 0.0),
            down: false,
            pressed: false,
            released: false,
            wheel: 0.0,
            keys: Vec::new(),
            hot: 0,
            active: 0,
            next_id: 1,
            clip: [0.0, 0.0, 1e5, 1e5],
            clips: Vec::new(),
            frame_no: 0,
            pressed_x: 0.0,
            open_menu: None,
            focus: None,
            consumed_click: false,
            tooltip: None,
            scratch: Vec::with_capacity(256),
        }
    }

    pub fn begin(&mut self, mouse: (f32, f32), pressed: bool, released: bool, down: bool, wheel: f32) {
        self.items.clear();
        self.tris.clear();
        self.tooltip = None;
        self.consumed_click = false;
        self.mouse = mouse;
        self.pressed = pressed;
        self.released = released;
        self.down = down;
        self.wheel = wheel;
        self.frame_no = self.frame_no.wrapping_add(1);
        // Идентификаторы виджетов должны быть одинаковыми от кадра к кадру,
        // иначе нажатый виджет не найдёт себя в кадре отпускания.
        self.next_id = 0;
        if pressed {
            self.pressed_x = mouse.0;
        }
        self.hot = 0;
        // ВАЖНО: active не сбрасываем здесь. В кадре отпускания виджет должен
        // увидеть, что он был нажат, и только сам снимет захват.
    }

    pub fn end(&mut self) {
        self.keys.clear();
        if !self.down {
            self.active = 0;
        }
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    // --- примитивы ---

    /// Миниатюра слоя из атласа миниатюр.
    pub fn thumb(&mut self, r: Rect, uv: [f32; 4]) {
        self.items.push(Item {
            x0: r[0], y0: r[1], x1: r[2], y1: r[3],
            u0: uv[0], v0: uv[1], u1: uv[2], v1: uv[3],
            color: [1.0, 1.0, 1.0, 1.0],
            unit: UNIT_THUMBS,
            clip: self.clip,
        });
    }

    /// Квад с произвольной текстурой (плавающий фрагмент, миниатюра).
    pub fn textured(&mut self, r: Rect, uv: [f32; 4], unit: u8) {
        self.items.push(Item {
            x0: r[0], y0: r[1], x1: r[2], y1: r[3],
            u0: uv[0], v0: uv[1], u1: uv[2], v1: uv[3],
            color: [1.0, 1.0, 1.0, 1.0],
            unit,
            clip: self.clip,
        });
    }

    /// Фон окна и рабочего поля — рисуется первым, до шахматки и холста.
    pub fn background(&mut self, r: Rect, color: Color) {
        if r[2] <= r[0] || r[3] <= r[1] || color[3] <= 0.0 {
            return;
        }
        self.items.push(Item::new(r[0], r[1], r[2], r[3], color, UNIT_BG, self.clip));
    }

    pub fn quad(&mut self, r: Rect, color: Color) {
        if r[2] <= r[0] || r[3] <= r[1] || color[3] <= 0.0 {
            return;
        }
        self.items.push(Item::new(r[0], r[1], r[2], r[3], color, UNIT_SOLID, self.clip));
    }

    pub fn canvas_quad(&mut self, r: Rect, uv: [f32; 4]) {
        self.items.push(Item {
            x0: r[0], y0: r[1], x1: r[2], y1: r[3],
            u0: uv[0], v0: uv[1], u1: uv[2], v1: uv[3],
            color: [1.0, 1.0, 1.0, 1.0],
            unit: UNIT_CANVAS,
            clip: self.clip,
        });
    }

    /// Шахматка прозрачности: uv считаются от экранных координат.
    pub fn checker(&mut self, r: Rect, cell: f32) {
        let u0 = r[0] / cell;
        let v0 = r[1] / cell;
        let u1 = r[2] / cell;
        let v1 = r[3] / cell;
        self.items.push(Item {
            x0: r[0], y0: r[1], x1: r[2], y1: r[3],
            u0, v0, u1, v1,
            color: [1.0, 1.0, 1.0, 1.0],
            unit: UNIT_CHECKER,
            clip: self.clip,
        });
    }

    /// Треугольник — для линий, окружностей и прочих наклонных фигур.
    pub fn tri(&mut self, p: [(f32, f32); 3], color: Color) {
        self.tris.push(Tri {
            p: [[p[0].0, p[0].1], [p[1].0, p[1].1], [p[2].0, p[2].1]],
            color,
            clip: self.clip,
        });
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color, width: f32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt();
        let hw = (width * 0.5).max(0.5);
        if len < 0.001 {
            self.quad([x0 - hw, y0 - hw, x0 + hw, y0 + hw], color);
            return;
        }
        let nx = -dy / len * hw;
        let ny = dx / len * hw;
        let a = (x0 + nx, y0 + ny);
        let b = (x1 + nx, y1 + ny);
        let c = (x1 - nx, y1 - ny);
        let d = (x0 - nx, y0 - ny);
        self.tri([a, b, c], color);
        self.tri([a, c, d], color);
    }

    pub fn frame(&mut self, r: Rect, color: Color, width: f32) {
        self.quad([r[0], r[1], r[2], r[1] + width], color);
        self.quad([r[0], r[3] - width, r[2], r[3]], color);
        self.quad([r[0], r[1] + width, r[0] + width, r[3] - width], color);
        self.quad([r[2] - width, r[1] + width, r[2], r[3] - width], color);
    }

    /// Залитый круг (например, образец кисти в панели).
    pub fn circle(&mut self, cx: f32, cy: f32, rad: f32, color: Color) {
        if rad <= 0.0 {
            return;
        }
        let y0 = (cy - rad).floor() as i32;
        let y1 = (cy + rad).ceil() as i32;
        for y in y0..=y1 {
            let dy = (y as f32 + 0.5 - cy) / rad;
            if dy.abs() > 1.0 {
                continue;
            }
            let dx = (1.0 - dy * dy).sqrt() * rad;
            self.quad([cx - dx, y as f32, cx + dx, y as f32 + 1.0], color);
        }
    }

    pub fn ring(&mut self, cx: f32, cy: f32, rad: f32, color: Color, width: f32) {
        let n = ((rad * 2.0).ceil() as i32).clamp(12, 64);
        for i in 0..n {
            let a0 = i as f32 / n as f32 * std::f32::consts::PI * 2.0;
            let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::PI * 2.0;
            let p0 = (cx + a0.cos() * rad, cy + a0.sin() * rad);
            let p1 = (cx + a1.cos() * rad, cy + a1.sin() * rad);
            self.line(p0.0, p0.1, p1.0, p1.1, color, width);
        }
    }

    pub fn text(&mut self, x: f32, y: f32, s: &str, size: f32, color: Color, bold: bool) -> f32 {
        self.scratch.clear();
        let w = self.fonts.layout(
            s,
            x,
            y,
            size,
            if bold { Weight::Bold } else { Weight::Regular },
            &mut self.scratch,
        );
        for (rect, uv, _) in self.scratch.iter() {
            self.items.push(Item {
                x0: rect[0], y0: rect[1], x1: rect[2], y1: rect[3],
                u0: uv[0], v0: uv[1], u1: uv[2], v1: uv[3],
                color,
                unit: UNIT_ATLAS,
                clip: self.clip,
            });
        }
        w
    }

    /// Текст по центру прямоугольника (по вертикали — по базовой линии).
    pub fn text_center(&mut self, r: Rect, s: &str, size: f32, color: Color, bold: bool) {
        let w = self.fonts.text_width(s, size, if bold { Weight::Bold } else { Weight::Regular });
        let h = self.fonts.line_height(size);
        self.text(r[0] + (r[2] - r[0] - w) / 2.0, r[1] + (r[3] - r[1] + h * 0.72) / 2.0, s, size, color, bold);
    }

    pub fn text_right(&mut self, x_right: f32, y: f32, s: &str, size: f32, color: Color, bold: bool) {
        let w = self.fonts.text_width(s, size, if bold { Weight::Bold } else { Weight::Regular });
        self.text(x_right - w, y, s, size, color, bold);
    }

    pub fn text_width(&mut self, s: &str, size: f32, bold: bool) -> f32 {
        self.fonts.text_width(s, size, if bold { Weight::Bold } else { Weight::Regular })
    }

    pub fn line_height(&mut self, size: f32) -> f32 {
        self.fonts.line_height(size)
    }

    pub fn push_clip(&mut self, r: Rect) {
        self.clips.push(self.clip);
        self.clip = [
            r[0].max(self.clip[0]),
            r[1].max(self.clip[1]),
            r[2].min(self.clip[2]),
            r[3].min(self.clip[3]),
        ];
    }

    pub fn pop_clip(&mut self) {
        self.clip = self.clips.pop().unwrap_or([0.0, 0.0, 1e5, 1e5]);
    }

    // --- ввод ---

    fn hovered(&self, r: Rect) -> bool {
        let (mx, my) = self.mouse;
        mx >= r[0] && mx < r[2] && my >= r[1] && my < r[3]
            && mx >= self.clip[0] && mx < self.clip[2] && my >= self.clip[1] && my < self.clip[3]
    }

    fn capture(&mut self, id: u32) {
        self.active = id;
        self.consumed_click = true;
    }

    pub fn is_dragging(&self, id: u32) -> bool {
        self.active == id
    }

    // --- виджеты ---

    pub fn button(&mut self, r: Rect, label: &str) -> bool {
        let id = self.id();
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                self.capture(id);
            }
        }
        let active = self.active == id;
        let clicked = self.released && active && hot;
        if self.released && active {
            self.active = 0;
        }
        let bg = if active { theme::ACCENT_DIM } else if hot { theme::PANEL_HI } else { theme::FIELD };
        self.quad(r, bg);
        self.frame(r, theme::BORDER, 1.0);
        self.text_center(r, label, FONT_UI, theme::TEXT, false);
        if clicked {
            self.consumed_click = true;
        }
        clicked
    }

    pub fn small_button(&mut self, r: Rect, label: &str) -> bool {
        let id = self.id();
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                self.capture(id);
            }
        }
        let active = self.active == id;
        let clicked = self.released && active && hot;
        if self.released && active {
            self.active = 0;
        }
        let bg = if active { theme::ACCENT_DIM } else if hot { theme::PANEL_HI } else { theme::FIELD };
        self.quad(r, bg);
        self.frame(r, theme::BORDER, 1.0);
        self.text_center(r, label, FONT_SMALL, theme::TEXT, false);
        if clicked {
            self.consumed_click = true;
        }
        clicked
    }

    /// Кнопка инструмента: квадратная, с иконкой и подсветкой активного.
    pub fn tool_button(&mut self, r: Rect, icon: Icon, label: &str, selected: bool) -> bool {
        let id = self.id();
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                self.capture(id);
            }
            self.tooltip = Some((r[2] + 6.0, r[1], label.to_string()));
        }
        let active = self.active == id;
        let clicked = self.released && active && hot;
        if self.released && active {
            self.active = 0;
        }
        if selected {
            self.quad(r, theme::ACCENT);
        } else if hot || active {
            self.quad(r, theme::PANEL_HI);
        }
        if !selected {
            self.frame(r, theme::BORDER, 1.0);
        }
        let cx = (r[0] + r[2]) / 2.0;
        let cy = (r[1] + r[3]) / 2.0;
        let col = if selected { [1.0, 1.0, 1.0, 1.0] } else { theme::TEXT };
        self.icon(icon, cx, cy, (r[3] - r[1]) * 0.3, col);
        if clicked {
            self.consumed_click = true;
        }
        clicked
    }

    pub fn icon(&mut self, kind: Icon, cx: f32, cy: f32, s: f32, color: Color) {
        match kind {
            Icon::Pencil => {
                self.line(cx - s * 0.6, cy + s * 0.6, cx + s * 0.6, cy - s * 0.6, color, 2.0);
                self.quad([cx + s * 0.3, cy - s * 0.9, cx + s * 0.75, cy - s * 0.45], color);
            }
            Icon::Brush => {
                self.line(cx - s * 0.2, cy + s * 0.8, cx + s * 0.7, cy - s * 0.6, color, 2.5);
                self.circle(cx - s * 0.45, cy + s * 0.55, s * 0.3, color);
            }
            Icon::Eraser => {
                self.quad([cx - s * 0.7, cy - s * 0.35, cx + s * 0.7, cy + s * 0.5], color);
                self.quad([cx - s * 0.7, cy - s * 0.35, cx + s * 0.1, cy - s * 0.05], [
                    color[0] * 0.6, color[1] * 0.6, color[2] * 0.6, color[3],
                ]);
            }
            Icon::Line => {
                self.line(cx - s * 0.7, cy + s * 0.7, cx + s * 0.7, cy - s * 0.7, color, 2.0);
                self.circle(cx - s * 0.7, cy + s * 0.7, 2.0, color);
                self.circle(cx + s * 0.7, cy - s * 0.7, 2.0, color);
            }
            Icon::Rect => self.frame([cx - s * 0.7, cy - s * 0.55, cx + s * 0.7, cy + s * 0.55], color, 2.0),
            Icon::Ellipse => self.ring(cx, cy, s * 0.62, color, 2.0),
            Icon::Fill => {
                self.quad([cx - s * 0.7, cy - s * 0.2, cx + s * 0.7, cy + s * 0.7], color);
                self.quad([cx - s * 0.25, cy - s * 0.8, cx + s * 0.25, cy - s * 0.2], color);
            }
            Icon::Eyedropper => {
                self.line(cx - s * 0.5, cy + s * 0.7, cx + s * 0.5, cy - s * 0.3, color, 3.0);
                self.quad([cx + s * 0.2, cy - s * 0.8, cx + s * 0.8, cy - s * 0.2], color);
            }
            Icon::Pan => {
                self.line(cx - s * 0.7, cy, cx + s * 0.7, cy, color, 2.0);
                self.line(cx, cy - s * 0.6, cx, cy + s * 0.6, color, 2.0);
                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    self.line(
                        cx + dx * s * 0.5, cy + dy * s * 0.5,
                        cx + dx * s * 0.8, cy + dy * s * 0.8, color, 2.0,
                    );
                }
            }
            Icon::Undo => {
                self.ring(cx, cy, s * 0.6, color, 2.0);
                self.quad([cx - s * 0.9, cy - s * 0.9, cx - s * 0.2, cy - s * 0.2], theme::PANEL_HI);
            }
            Icon::Redo => {
                self.ring(cx, cy, s * 0.6, color, 2.0);
                self.quad([cx + s * 0.2, cy - s * 0.9, cx + s * 0.9, cy - s * 0.2], theme::PANEL_HI);
            }
            Icon::Swap => {
                self.line(cx - s * 0.6, cy - s * 0.4, cx + s * 0.6, cy - s * 0.4, color, 2.0);
                self.line(cx - s * 0.6, cy + s * 0.4, cx + s * 0.6, cy + s * 0.4, color, 2.0);
            }
            Icon::Eye | Icon::EyeOff => {
                self.ring(cx, cy, s * 0.5, color, 2.0);
                if matches!(kind, Icon::EyeOff) {
                    self.line(cx - s * 0.7, cy - s * 0.7, cx + s * 0.7, cy + s * 0.7, theme::PANEL, 2.5);
                    self.line(cx - s * 0.7, cy - s * 0.7, cx + s * 0.7, cy + s * 0.7, color, 1.5);
                }
            }
            Icon::Gradient => {
                // прямоугольник с градиентом: верх светлее, низ темнее
                let hh = s.max(1.0);
                let n = 6;
                for i in 0..n {
                    let t0 = i as f32 / n as f32;
                    let t1 = (i + 1) as f32 / n as f32;
                    let y0 = cy - hh + t0 * hh * 2.0;
                    let y1 = cy - hh + t1 * hh * 2.0;
                    let v = 1.0 - t0 * 0.85;
                    self.quad([cx - s * 0.8, y0, cx + s * 0.8, y1], [v, v, v, 1.0]);
                }
                self.frame([cx - s * 0.8, cy - hh, cx + s * 0.8, cy + hh], color, 1.0);
            }
            Icon::Select => {
                // рамка выделения с пунктиром — как «муравьиные дорожки»
                let hw = s * 0.75;
                let hh = s * 0.6;
                let rect = [cx - hw, cy - hh, cx + hw, cy + hh];
                let n = 5;
                // горизонтальные стороны
                for i in 0..n {
                    if i % 2 == 1 {
                        continue;
                    }
                    let t0 = i as f32 / n as f32;
                    let t1 = (i + 1) as f32 / n as f32;
                    let x0 = rect[0] + t0 * hw * 2.0;
                    let x1 = rect[0] + t1 * hw * 2.0;
                    self.quad([x0, rect[1], x1, rect[1] + 1.6], color);
                    self.quad([x0, rect[3] - 1.6, x1, rect[3]], color);
                }
                for i in 0..4 {
                    if i % 2 == 1 {
                        continue;
                    }
                    let t0 = i as f32 / 4.0;
                    let t1 = (i + 1) as f32 / 4.0;
                    let y0 = rect[1] + t0 * hh * 2.0;
                    let y1 = rect[1] + t1 * hh * 2.0;
                    self.quad([rect[0], y0, rect[0] + 1.6, y1], color);
                    self.quad([rect[2] - 1.6, y0, rect[2], y1], color);
                }
            }
            Icon::Text => {
                // буква «А» из трёх штрихов — читается даже в 20 px
                let s2 = s * 0.62;
                self.line(cx, cy + s2 * 0.9, cx - s2, cy - s2, color, 2.0);
                self.line(cx, cy + s2 * 0.9, cx + s2, cy - s2, color, 2.0);
                self.line(cx - s2 * 0.55, cy - s2 * 0.05, cx + s2 * 0.55, cy - s2 * 0.05, color, 2.0);
            }
        }
    }

    /// Пунктирная рамка выделения «муравьиными дорожками» (анимированная).
    /// phase — сдвиг по времени, чтобы пунктир «бежал».
    pub fn marching_ants(&mut self, r: Rect, phase: f32, dash: f32, gap: f32) {
        let col_dark = [0.0, 0.0, 0.0, 0.9];
        let col_light = [1.0, 1.0, 1.0, 0.9];
        let period = dash + gap;
        for pass in 0..2 {
            let c = if pass == 0 { col_dark } else { col_light };
            let off = if pass == 0 { 0.0 } else { dash };
            let edges: [(f32, f32, f32, f32); 4] = [
                (r[0], r[1], r[2], r[1] + 1.0), // верх
                (r[0], r[3] - 1.0, r[2], r[3]), // низ
                (r[0], r[1], r[0] + 1.0, r[3]), // лево
                (r[2] - 1.0, r[1], r[2], r[3]), // право
            ];
            for (x0, y0, x1, y1) in edges {
                let len = (x1 - x0).abs().max((y1 - y0).abs());
                if len <= 0.0 {
                    continue;
                }
                let horizontal = (x1 - x0).abs() >= (y1 - y0).abs();
                let mut t = (phase + off).rem_euclid(period);
                while t < len {
                    let e = (t + dash).min(len);
                    if horizontal {
                        self.quad([x0 + t, y0, x0 + e, y1], c);
                    } else {
                        self.quad([x0, y0 + t, x1, y0 + e], c);
                    }
                    t += period;
                }
            }
        }
    }

    /// Поле-«число»: подпись слева, значение справа, перетаскивание меняет.
    pub fn value_field(&mut self, r: Rect, label: &str, v: &mut f32, min: f32, max: f32, step: f32) -> bool {
        let id = self.id();
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                self.capture(id);
            }
        }
        let active = self.active == id;
        let mut changed = false;
        if active && self.down {
            let dx = self.mouse.0 - self.mouse_down_x();
            let nv = (*v + dx * step).clamp(min, max);
            if (nv - *v).abs() > 1e-6 {
                *v = nv;
                changed = true;
            }
        }
        if self.released && active {
            self.active = 0;
        }
        self.quad(r, if hot || active { theme::PANEL_HI } else { theme::FIELD });
        self.frame(r, theme::BORDER, 1.0);
        let h = self.line_height(FONT_SMALL);
        let y = r[1] + (r[3] - r[1] + h * 0.72) / 2.0;
        self.text(r[0] + 6.0, y, label, FONT_SMALL, theme::TEXT_DIM, false);
        let txt = fmt_num(*v);
        self.text_right(r[2] - 6.0, y, &txt, FONT_SMALL, theme::TEXT, false);
        changed
    }

    fn mouse_down_x(&self) -> f32 {
        // Верхняя граница изменения считается от места нажатия: хранится в pressed_x.
        self.pressed_x
    }

    /// Зона перетаскивания с постоянным идентификатором (для своих виджетов).
    /// Возвращает нормализованные координаты указателя, пока идёт перетаскивание.
    pub fn drag_with(&mut self, id: u32, r: Rect) -> Option<(f32, f32)> {
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                self.capture(id);
            }
        }
        if self.active == id && self.down {
            let nx = ((self.mouse.0 - r[0]) / (r[2] - r[0]).max(1.0)).clamp(0.0, 1.0);
            let ny = ((self.mouse.1 - r[1]) / (r[3] - r[1]).max(1.0)).clamp(0.0, 1.0);
            self.consumed_click = true;
            return Some((nx, ny));
        }
        if self.released && self.active == id {
            self.active = 0;
        }
        None
    }

    pub fn checkbox(&mut self, r: Rect, v: &mut bool, label: &str) -> bool {
        let id = self.id();
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                self.capture(id);
            }
        }
        let active = self.active == id;
        let clicked = self.released && active && hot;
        if self.released && active {
            self.active = 0;
        }
        let box_r = [r[0], r[1] + 2.0, r[0] + 14.0, r[1] + 16.0];
        self.quad(box_r, if *v { theme::ACCENT } else { theme::FIELD });
        self.frame(box_r, theme::BORDER, 1.0);
        if *v {
            self.line(box_r[0] + 3.0, box_r[1] + 7.0, box_r[0] + 6.0, box_r[1] + 11.0, [1.0; 4], 2.0);
            self.line(box_r[0] + 6.0, box_r[1] + 11.0, box_r[0] + 11.0, box_r[1] + 3.0, [1.0; 4], 2.0);
        }
        let h = self.line_height(FONT_SMALL);
        self.text(r[0] + 20.0, r[1] + (r[3] - r[1] + h * 0.72) / 2.0 - 2.0, label, FONT_SMALL, theme::TEXT, false);
        if clicked {
            *v = !*v;
            self.consumed_click = true;
        }
        clicked
    }

    pub fn dropdown(&mut self, r: Rect, cur: usize, items: &[&str]) -> Option<usize> {
        let id = self.id();
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                self.open_menu = if self.open_menu == Some(id) { None } else { Some(id) };
                self.consumed_click = true;
            }
        }
        let open = self.open_menu == Some(id);
        self.quad(r, if hot { theme::PANEL_HI } else { theme::FIELD });
        self.frame(r, if open { theme::ACCENT } else { theme::BORDER }, 1.0);
        let h = self.line_height(FONT_SMALL);
        let y = r[1] + (r[3] - r[1] + h * 0.72) / 2.0;
        self.text(r[0] + 6.0, y, items.get(cur).copied().unwrap_or("—"), FONT_SMALL, theme::TEXT, false);
        // стрелка
        let ax = r[2] - 12.0;
        let ay = r[1] + r[3] / 2.0;
        self.line(ax - 4.0, ay - 2.0, ax, ay + 2.0, theme::TEXT_DIM, 1.5);
        self.line(ax, ay + 2.0, ax + 4.0, ay - 2.0, theme::TEXT_DIM, 1.5);

        if open {
            let ih = 20.0;
            let list = [r[0], r[3] + 1.0, r[2], r[3] + 1.0 + ih * items.len() as f32];
            self.quad(list, theme::PANEL);
            self.frame(list, theme::BORDER, 1.0);
            for (i, it) in items.iter().enumerate() {
                let ir = [list[0] + 1.0, list[1] + 1.0 + i as f32 * ih, list[2] - 1.0, list[1] + 1.0 + (i + 1) as f32 * ih];
                if self.hovered(ir) && self.pressed {
                    self.open_menu = None;
                    self.consumed_click = true;
                    return Some(i);
                }
                if i % 2 == 0 {
                    self.quad([ir[0], ir[1], ir[2], ir[3]], [1.0, 1.0, 1.0, 0.03]);
                }
                let lh = self.line_height(FONT_SMALL);
                self.text(ir[0] + 6.0, ir[1] + (ih + lh * 0.72) / 2.0, it, FONT_SMALL, theme::TEXT, false);
            }
        }
        None
    }

    /// Строка списка (слои, палитра) с выделением.
    pub fn list_row(&mut self, r: Rect, selected: bool, hover: bool) -> bool {
        let id = self.id();
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                self.capture(id);
            }
        }
        let active = self.active == id;
        let clicked = self.released && active && hot;
        if self.released && active {
            self.active = 0;
        }
        if selected {
            self.quad(r, theme::ACCENT_DIM);
        } else if hover || active {
            self.quad(r, theme::PANEL_HI);
        }
        if clicked {
            self.consumed_click = true;
        }
        clicked
    }

    /// Цветовой квад с шахматкой под альфой.
    pub fn swatch(&mut self, r: Rect, c: [u8; 4]) {
        self.checker(r, 8.0);
        let col = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0];
        self.quad(r, col);
        self.frame(r, theme::BORDER, 1.0);
    }

    // --- текстовое поле с вводом ---

    pub fn text_field(&mut self, r: Rect, value: &mut String, filter: fn(char) -> bool) -> bool {
        let id = self.id();
        let hot = self.hovered(r);
        if hot {
            self.hot = id;
            if self.pressed {
                if self.focus == Some(id) {
                    self.focus = None;
                } else {
                    self.focus = Some(id);
                }
                self.consumed_click = true;
            }
        }
        let focused = self.focus == Some(id);
        if focused {
            // Клавиши достаются полем в фокусе; нерассмотренные возвращаем в очередь.
            let keys = std::mem::take(&mut self.keys);
            let mut rest = Vec::new();
            for k in keys {
                match k {
                    KeyEv::Char(c) if filter(c) => value.push(c),
                    KeyEv::Backspace => {
                        value.pop();
                    }
                    KeyEv::Delete => value.clear(),
                    KeyEv::Enter => self.focus = None,
                    KeyEv::Escape => self.focus = None,
                    other => rest.push(other),
                }
            }
            self.keys = rest;
        }
        self.quad(r, if focused { theme::PANEL_HI } else { theme::FIELD });
        self.frame(r, if focused { theme::ACCENT } else { theme::BORDER }, 1.0);
        let h = self.line_height(FONT_SMALL);
        let y = r[1] + (r[3] - r[1] + h * 0.72) / 2.0;
        let shown = if value.is_empty() { "—" } else { value.as_str() };
        self.text(r[0] + 6.0, y, shown, FONT_SMALL, if value.is_empty() { theme::TEXT_DIM } else { theme::TEXT }, false);
        if focused && ((self.frame_no as i32) / 30) % 2 == 0 {
            let w = self.text_width(value, FONT_SMALL, false);
            self.quad([r[0] + 6.0 + w + 1.0, y - h * 0.72, r[0] + 8.0 + w, y + 2.0], theme::ACCENT);
        }
        focused
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Pencil,
    Brush,
    Eraser,
    Line,
    Rect,
    Ellipse,
    Fill,
    Eyedropper,
    Pan,
    Undo,
    Redo,
    Swap,
    Eye,
    EyeOff,
    Gradient,
    Select,
    Text,
}

pub fn rgba(c: [u8; 4]) -> Color {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0]
}

pub fn fmt_num(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i32)
    } else {
        format!("{:.2}", v)
    }
}

