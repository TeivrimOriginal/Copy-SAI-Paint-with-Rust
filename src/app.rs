//! Состояние приложения: инструменты, цвета, история (undo/redo), активный штрих.

use crate::canvas::{Canvas, WHITE};

/// Инструменты рисования.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pencil,
    Brush,
    Eraser,
    Line,
    Rect,
    Ellipse,
    Fill,
}

impl Tool {
    pub fn from_idx(i: u32) -> Tool {
        match i {
            0 => Tool::Pencil,
            1 => Tool::Brush,
            2 => Tool::Eraser,
            3 => Tool::Line,
            4 => Tool::Rect,
            5 => Tool::Ellipse,
            _ => Tool::Fill,
        }
    }

    #[allow(dead_code)]
    pub fn idx(self) -> u32 {
        match self {
            Tool::Pencil => 0,
            Tool::Brush => 1,
            Tool::Eraser => 2,
            Tool::Line => 3,
            Tool::Rect => 4,
            Tool::Ellipse => 5,
            Tool::Fill => 6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Pencil => "Карандаш",
            Tool::Brush => "Кисть",
            Tool::Eraser => "Ластик",
            Tool::Line => "Линия",
            Tool::Rect => "Прямоугольник",
            Tool::Ellipse => "Эллипс",
            Tool::Fill => "Заливка",
        }
    }
}

const UNDO_LIMIT: usize = 25;

#[derive(Clone)]
pub struct Stroke {
    pub start_x: i32,
    pub start_y: i32,
    pub last_x: i32,
    pub last_y: i32,
    /// Снимок до начала штриха (для «резиновой» отрисовки линий/фигур).
    pub snapshot: Vec<u32>,
}

pub struct App {
    pub canvas: Canvas,
    pub tool: Tool,
    pub color: u32,
    pub thickness: i32,
    pub undo: Vec<Vec<u32>>,
    pub redo: Vec<Vec<u32>>,
    pub stroke: Option<Stroke>,
    pub drawing: bool,
}

impl App {
    pub fn new(w: i32, h: i32) -> Self {
        Self {
            canvas: Canvas::new(w, h),
            tool: Tool::Pencil,
            color: 0x00_00_00_00, // чёрный
            thickness: 3,
            undo: Vec::new(),
            redo: Vec::new(),
            stroke: None,
            drawing: false,
        }
    }

    pub fn radius(&self) -> i32 {
        (self.thickness / 2).max(1)
    }

    /// Цвет для текущего инструмента (ластик всегда белый).
    pub fn tool_color(&self) -> u32 {
        if self.tool == Tool::Eraser {
            WHITE
        } else {
            self.color
        }
    }

    fn push_undo(&mut self, snap: Vec<u32>) {
        if self.undo.len() >= UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(snap);
    }

    pub fn set_tool(&mut self, t: Tool) {
        self.tool = t;
    }

    /// Начало штриха (или мгновенное действие заливки).
    pub fn begin_stroke(&mut self, x: i32, y: i32) {
        if !self.canvas.contains(x, y) {
            return;
        }
        if self.tool == Tool::Fill {
            self.push_undo(self.canvas.snapshot());
            self.canvas.flood_fill(x, y, self.color);
            self.redo.clear();
            return;
        }
        self.redo.clear();
        let snap = self.canvas.snapshot();
        self.push_undo(snap.clone());
        let c = self.tool_color();
        self.stroke = Some(Stroke {
            start_x: x,
            start_y: y,
            last_x: x,
            last_y: y,
            snapshot: snap,
        });
        self.drawing = true;
        match self.tool {
            Tool::Pencil => self.canvas.disc(x, y, 0, c),
            Tool::Brush | Tool::Eraser => self.canvas.disc(x, y, self.radius(), c),
            _ => {}
        }
    }

    /// Движение мыши во время штриха. Возвращает грязную область для перерисовки.
    pub fn move_stroke(&mut self, x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
        if self.stroke.is_none() {
            return None;
        }
        let c = self.tool_color();
        let r = self.radius();
        let (x, y) = (
            x.clamp(0, self.canvas.width - 1).max(0),
            y.clamp(0, self.canvas.height - 1).max(0),
        );
        let st = self.stroke.as_mut().unwrap();
        let (lx, ly) = (st.last_x, st.last_y);
        let (sx, sy) = (st.start_x, st.start_y);

        match self.tool {
            Tool::Pencil => {
                self.canvas.thick_line(lx, ly, x, y, 0, c);
            }
            Tool::Brush | Tool::Eraser => {
                self.canvas.thick_line(lx, ly, x, y, r, c);
            }
            Tool::Line => {
                self.canvas.restore(&st.snapshot);
                self.canvas.thick_line(sx, sy, x, y, r, c);
            }
            Tool::Rect => {
                self.canvas.restore(&st.snapshot);
                self.canvas.rect(sx, sy, x, y, false, r, c);
            }
            Tool::Ellipse => {
                self.canvas.restore(&st.snapshot);
                self.canvas.ellipse(sx, sy, x, y, false, r, c);
            }
            Tool::Fill => {}
        }
        st.last_x = x;
        st.last_y = y;

        let (minx, maxx) = (sx.min(x).min(lx) - r, sx.max(x).max(lx) + r);
        let (miny, maxy) = (sy.min(y).min(ly) - r, sy.max(y).max(ly) + r);
        Some((minx, miny, maxx, maxy))
    }

    pub fn end_stroke(&mut self) {
        if self.drawing {
            self.stroke = None;
            self.drawing = false;
        }
    }

    pub fn undo(&mut self) {
        self.end_stroke();
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.canvas.snapshot());
            self.canvas.restore(&s);
        }
    }

    pub fn redo(&mut self) {
        self.end_stroke();
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.canvas.snapshot());
            self.canvas.restore(&s);
        }
    }

    pub fn clear_canvas(&mut self) {
        self.end_stroke();
        self.push_undo(self.canvas.snapshot());
        self.canvas.clear(WHITE);
        self.redo.clear();
    }

    pub fn load_file(&mut self, path: &str) -> Result<(), String> {
        self.end_stroke();
        self.canvas.load_png(path)?;
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }

    pub fn save_file(&self, path: &str) -> Result<(), String> {
        self.canvas.save_png(path)
    }
}