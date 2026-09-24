//! Состояние приложения: холст со слоями, инструменты, ввод, история, файлы.

use crate::doc::{Action, Document, History, Layer, LayerMeta};
use crate::raster;
use crate::tools::{Params, Tool};
use crate::raster::{RectData, SelRect};
use crate::ui::Rect;
use std::time::Instant;

/// Вставленный фрагмент, который ещё можно перетащить по холсту.
pub struct Floating {
    pub data: RectData,
    pub x: f32,
    pub y: f32,
}

/// Что происходит с выделением при нажатой кнопке мыши.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelDrag {
    /// Ничего: обычное состояние.
    None,
    /// Тянем новую рамку выделения.
    Marquee,
    /// Тянем плавающий фрагмент (вставленное или вырезанное содержимое).
    Move,
}

/// Собственное модальное окно открытия/сохранения: список каталога,
/// поле пути и имени файла. Системные диалоги не используются.
pub struct FileDialog {
    pub save: bool,
    pub path: String,
    pub file: String,
    pub entries: Vec<(String, bool)>,
    pub scroll: usize,
    /// (полный путь, режим сохранения) — забирает главный цикл
    pub result: Option<(String, bool)>,
}

impl FileDialog {
    pub fn new(save: bool, start: &str) -> Self {
        let mut d = Self {
            save,
            path: if start.is_empty() { default_dir() } else { start.to_string() },
            file: if save { "рисунок.png".to_string() } else { String::new() },
            entries: Vec::new(),
            scroll: 0,
            result: None,
        };
        d.scan();
        d
    }

    pub fn scan(&mut self) {
        self.entries.clear();
        self.scroll = 0;
        if self.path.len() > 3 {
            self.entries.push(("..".to_string(), true));
        }
        if let Ok(rd) = std::fs::read_dir(&self.path) {
            let mut files = Vec::new();
            let mut dirs = Vec::new();
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let is_dir = e.path().is_dir();
                if is_dir {
                    dirs.push((name, true));
                } else if name.to_lowercase().ends_with(".png") {
                    files.push((name, false));
                }
            }
            dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
            files.sort_by(|a, b| b.0.to_lowercase().cmp(&a.0.to_lowercase()));
            self.entries.extend(dirs);
            self.entries.extend(files);
        }
    }

    pub fn enter_dir(&mut self, name: &str) {
        if name == ".." {
            if let Some(up) = std::path::Path::new(&self.path).parent() {
                self.path = up.to_string_lossy().into_owned();
            }
        } else {
            self.path = std::path::Path::new(&self.path).join(name).to_string_lossy().into_owned();
        }
        self.scan();
    }

    pub fn accept(&mut self) {
        let name = if self.file.is_empty() { "рисунок.png".to_string() } else { self.file.clone() };
        let full = std::path::Path::new(&self.path).join(name).to_string_lossy().into_owned();
        self.result = Some((full, self.save));
    }

    pub fn close(&mut self) {
        self.result = Some((String::new(), false));
    }
}

fn default_dir() -> String {
    std::env::var("USERPROFILE")
        .map(|p| format!("{}\\Pictures", p))
        .unwrap_or_else(|_| "C:\\".to_string())
}

pub struct App {
    pub doc: Document,
    pub history: History,
    pub tool: Tool,
    pub params: Params,
    pub primary: [u8; 4],
    pub secondary: [u8; 4],
    pub hex_buf: String,
    pub color_swapped: bool,

    // Вид: масштаб и смещение холста в экранных координатах
    pub zoom: f32,
    pub pan: (f32, f32),
    pub canvas_rect: Rect,
    pub fit_pending: bool,

    // Рисование
    pub drawing: bool,
    pub stroke_start: (f32, f32),
    pub stroke_last: (f32, f32),
    pub stroke_base: Option<Vec<u8>>,
    pub smooth: (f32, f32),
    pub cursor: (f32, f32),
    /// Курсор мыши в экранных координатах (для рамки кисти и подсказок).
    pub cursor_screen: (f32, f32),
    pub cursor_in_canvas: bool,

    // Выделение рамкой и плавающий фрагмент после вставки
    pub selection: Option<SelRect>,
    pub clipboard: Option<RectData>,
    pub floating: Option<Floating>,
    /// Фаза анимации «муравьиных дорожек» в пикселях.
    pub ants_phase: f32,
    /// Фрагмент изменился — текстуру нужно перезалить в GPU.
    pub float_dirty: bool,
    /// Что именно происходит с выделением в данный момент.
    pub sel_drag: SelDrag,
    /// Плавающий фрагмент реально сдвинули (иначе щелчком он не крепится).
    pub float_moved: bool,
    /// Прокрутка списка слоёв (первая видимая строка сверху).
    pub layer_scroll: usize,

    // Файл и состояние
    pub path: Option<String>,
    pub dirty: bool,
    pub notice: Option<(String, Instant)>,
    pub show_grid: bool,
    pub dialog: Option<FileDialog>,
}

impl App {
    pub fn new() -> Self {
        let mut doc = Document::new(1600, 1000);
        // Фон — белый, как в новом документе графического редактора.
        doc.layers[0].meta.name = "Фон".to_string();
        doc.layers[0].fill([255, 255, 255, 255]);
        doc.touch();
        let mut app = Self {
            doc,
            history: History::new(40),
            tool: Tool::Brush,
            params: Params::default(),
            primary: [0, 0, 0, 255],
            secondary: [255, 255, 255, 255],
            hex_buf: "000000FF".to_string(),
            color_swapped: false,
            zoom: 1.0,
            pan: (0.0, 0.0),
            canvas_rect: [0.0, 0.0, 100.0, 100.0],
            fit_pending: true,
            drawing: false,
            stroke_start: (0.0, 0.0),
            stroke_last: (0.0, 0.0),
            stroke_base: None,
            smooth: (0.0, 0.0),
            cursor: (0.0, 0.0),
            cursor_screen: (0.0, 0.0),
            cursor_in_canvas: false,
            selection: None,
            clipboard: None,
            floating: None,
            ants_phase: 0.0,
            float_dirty: false,
            sel_drag: SelDrag::None,
            float_moved: false,
            layer_scroll: 0,
            path: None,
            dirty: false,
            notice: None,
            show_grid: false,
            dialog: None,
        };
        app.doc.ensure_composite();
        app
    }

    pub fn color(&self) -> [u8; 4] {
        if self.color_swapped {
            self.secondary
        } else {
            self.primary
        }
    }

    pub fn set_color(&mut self, c: [u8; 4]) {
        if self.color_swapped {
            self.secondary = c;
        } else {
            self.primary = c;
        }
    }

    pub fn notify(&mut self, msg: &str) {
        self.notice = Some((msg.to_string(), Instant::now()));
    }

    // --- преобразование координат ---

    pub fn canvas_to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        (self.canvas_rect[0] + self.pan.0 + x * self.zoom, self.canvas_rect[1] + self.pan.1 + y * self.zoom)
    }

    pub fn screen_to_canvas(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.canvas_rect[0] - self.pan.0) / self.zoom, (y - self.canvas_rect[1] - self.pan.1) / self.zoom)
    }

    pub fn in_canvas(&self, p: (f32, f32)) -> bool {
        let (x, y) = p;
        x >= self.canvas_rect[0] && x < self.canvas_rect[2] && y >= self.canvas_rect[1] && y < self.canvas_rect[3]
    }

    pub fn fit_view(&mut self) {
        let r = self.canvas_rect;
        let w = (r[2] - r[0]).max(1.0);
        let h = (r[3] - r[1]).max(1.0);
        let pad = 24.0;
        let zx = (w - pad * 2.0) / self.doc.width as f32;
        let zy = (h - pad * 2.0) / self.doc.height as f32;
        self.zoom = zx.min(zy).clamp(0.02, 16.0);
        self.pan.0 = (w - self.doc.width as f32 * self.zoom) / 2.0;
        self.pan.1 = (h - self.doc.height as f32 * self.zoom) / 2.0;
    }

    pub fn zoom_at(&mut self, screen: (f32, f32), factor: f32) {
        let before = self.screen_to_canvas(screen.0, screen.1);
        self.zoom = (self.zoom * factor).clamp(0.05, 32.0);
        let after = self.screen_to_canvas(screen.0, screen.1);
        self.pan.0 += (after.0 - before.0) * self.zoom;
        self.pan.1 += (after.1 - before.1) * self.zoom;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        let c = ((self.canvas_rect[0] + self.canvas_rect[2]) / 2.0, (self.canvas_rect[1] + self.canvas_rect[3]) / 2.0);
        self.zoom_at(c, factor);
    }

    // --- рисование ---

    fn inside(&self, x: f32, y: f32) -> bool {
        x >= 0.0 && y >= 0.0 && x < self.doc.width as f32 && y < self.doc.height as f32
    }

    pub fn begin_stroke(&mut self, p: (f32, f32), secondary: bool) {
        if !self.inside(p.0, p.1) {
            return;
        }
        let color = if secondary { self.other_color() } else { self.color() };
        match self.tool {
            Tool::Select => {
                self.begin_select(p);
                return;
            }
            Tool::Eyedropper => {
                self.pick(p);
                return;
            }
            Tool::Fill => {
                let base = self.doc.active_layer().pixels.clone();
                let (w, h) = (self.doc.width, self.doc.height);
                let col = self.fill_color(secondary);
                let layer = self.doc.active_layer_mut();
                raster::flood_fill(
                    layer,
                    w,
                    h,
                    p.0 as i32,
                    p.1 as i32,
                    col,
                    self.params.opacity,
                    self.params.tolerance as u8,
                    self.params.contiguous,
                );
                self.history.push(Action::Pixels { layer: self.doc.active, before: base, after: self.doc.layers[self.doc.active].pixels.clone() });
                self.doc.touch();
                self.dirty = true;
                return;
            }
            _ => {}
        }
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        self.stroke_base = Some(self.doc.active_layer().pixels.clone());
        self.drawing = true;
        self.stroke_start = p;
        self.stroke_last = p;
        self.smooth = p;
        let _ = color;
        // Первая точка: для карандаша/кисти сразу ставим dab, иначе ждём движения.
        if self.tool.is_freehand() {
            let p2 = self.params_for_stroke();
            let c = self.stroke_color(secondary);
            let (w, h) = (self.doc.width, self.doc.height);
            let layer = self.doc.active_layer_mut();
            raster::stamp(layer, w, h, p.0, p.1, p2.0, p2.1, c, p2.2);
            self.doc.touch();
        }
    }

    pub fn move_stroke(&mut self, p: (f32, f32), secondary: bool) {
        if !self.drawing {
            return;
        }
        let p = if self.inside(p.0, p.1) {
            p
        } else {
            (
                p.0.clamp(0.0, self.doc.width as f32 - 1.0),
                p.1.clamp(0.0, self.doc.height as f32 - 1.0),
            )
        };
        let (w, h) = (self.doc.width, self.doc.height);
        let color = self.stroke_color(secondary);
        let base = self.stroke_base.clone();
        let p2 = self.params_for_stroke();
        let tool = self.tool;

        if tool.is_freehand() {
            // Стабилизатор: чем выше сглаживание, тем «инертнее» мазок.
            let k = 1.0 - self.params.smoothing * 0.9;
            let s = (self.smooth.0 + (p.0 - self.smooth.0) * k, self.smooth.1 + (p.1 - self.smooth.1) * k);
            let layer = self.doc.active_layer_mut();
            raster::brush_line(layer, w, h, self.smooth.0, self.smooth.1, s.0, s.1, p2.0, p2.1, color, p2.2);
            self.smooth = s;
        } else if let Some(base) = base {
            // Цвета для градиента и флаг заливки берём ДО взятия слоя:
            // иначе активный слой уже занят mutable-ссылкой.
            let shape_fill = self.params.shape_fill;
            let gsoft = self.params.gradient_soft;
            let grad_a = if secondary { self.other_color() } else { self.color() };
            let grad_b = if secondary { self.color() } else { self.other_color() };
            self.doc.active_layer_mut().pixels.copy_from_slice(&base);
            let layer = self.doc.active_layer_mut();
            let (sx, sy) = self.stroke_start;
            match tool {
                Tool::Line => raster::brush_line(layer, w, h, sx, sy, p.0, p.1, p2.0, p2.1, color, p2.2),
                Tool::Rect => raster::rect(layer, w, h, sx, sy, p.0, p.1, shape_fill, p2.0, p2.1, color, p2.2),
                Tool::Ellipse => raster::ellipse(layer, w, h, sx, sy, p.0, p.1, shape_fill, p2.0, p2.1, color, p2.2),
                // Градиент идёт от выбранного цвета ко второму
                Tool::Gradient => raster::gradient(layer, w, h, sx, sy, p.0, p.1, grad_a, grad_b, p2.2, gsoft),
                _ => {}
            }
        }
        self.stroke_last = p;
        self.doc.touch();
        self.dirty = true;
    }

    pub fn end_stroke(&mut self) {
        if !self.drawing {
            return;
        }
        self.drawing = false;
        if let Some(before) = self.stroke_base.take() {
            let after = self.doc.layers[self.doc.active].pixels.clone();
            if before != after {
                self.history.push(Action::Pixels { layer: self.doc.active, before, after });
            }
        }
        self.doc.touch();
    }

    // --- выделение рамкой ---

    /// Нажатие инструментом «Выделение»: по плавающему фрагменту — двигаем
    /// его, внутри рамки — переносим содержимое, иначе рисуем новую рамку.
    pub fn begin_select(&mut self, p: (f32, f32)) {
        // Щелчок рядом с плавающим фрагментом закрепляет его на слое.
        if self.floating.is_some() {
            let (fx, fy, fw, fh) = match &self.floating {
                Some(f) => (f.x, f.y, f.data.width() as f32, f.data.height() as f32),
                None => (0.0, 0.0, 0.0, 0.0),
            };
            if p.0 >= fx && p.0 < fx + fw && p.1 >= fy && p.1 < fy + fh {
                self.sel_drag = SelDrag::Move;
                self.stroke_last = p;
                return;
            }
            self.floating_commit();
            if !self.inside(p.0, p.1) {
                return;
            }
        }
        if let Some(s) = self.selection {
            if s.contains(p) {
                if self.params.shape_fill {
                    // Флажок «Залить выделение»: клик по рамке заливает её.
                    self.fill_selection();
                    return;
                }
                // Перенос содержимого: вырезаем в плавающий фрагмент и тянем.
                if self.doc.active_layer().meta.locked {
                    self.notify("Слой заблокирован");
                    return;
                }
                let before = self.doc.active_layer().pixels.clone();
                let (w, h) = (self.doc.width, self.doc.height);
                let layer = self.doc.active_layer_mut();
                let data = raster::extract_rect(layer, w, h, s);
                raster::clear_rect(layer, w, h, s);
                let after = self.doc.layers[self.doc.active].pixels.clone();
                if before != after {
                    self.history.push(Action::Pixels { layer: self.doc.active, before, after });
                    self.doc.touch();
                    self.dirty = true;
                }
                self.floating = Some(Floating { data, x: s.x, y: s.y });
                self.float_dirty = true;
                self.sel_drag = SelDrag::Move;
                self.float_moved = false;
                self.stroke_last = p;
                return;
            }
            self.selection = None;
        }
        // Новая рамка выделения.
        self.selection = Some(SelRect::new(p.0, p.1, p.0, p.1));
        self.sel_drag = SelDrag::Marquee;
        self.stroke_start = p;
        self.notify("");
    }

    /// Протягивание при активном выделении.
    pub fn move_select(&mut self, p: (f32, f32)) {
        match self.sel_drag {
            SelDrag::Marquee => {
                if let Some(s) = self.selection.as_mut() {
                    *s = SelRect::new(self.stroke_start.0, self.stroke_start.1, p.0, p.1);
                }
            }
            SelDrag::Move => self.floating_move(p),
            SelDrag::None => {
                // Фрагмент уже ждёт щелчка — просто следуем за курсором.
                if self.floating.is_some() {
                    self.floating_move(p);
                }
            }
        }
    }

    /// Отпускание кнопки: рамка фиксируется, фрагмент — закрепляется.
    pub fn end_select(&mut self) {
        match self.sel_drag {
            SelDrag::Marquee => {
                // Щелчок без протягивания — снимаем выделение.
                if let Some(s) = self.selection {
                    if s.w < 2.0 || s.h < 2.0 {
                        self.selection = None;
                    } else {
                        self.notify(&format!("Выделено {}×{} пикселей", s.w as i32, s.h as i32));
                    }
                }
            }
            SelDrag::Move => {
                if self.float_moved {
                    self.floating_commit();
                }
            }
            SelDrag::None => {}
        }
        self.sel_drag = SelDrag::None;
    }

    /// Выделяет весь холст.
    pub fn select_all(&mut self) {
        self.end_stroke();
        self.floating = None;
        self.selection = Some(SelRect {
            x: 0.0,
            y: 0.0,
            w: self.doc.width as f32,
            h: self.doc.height as f32,
        });
        self.notify("Выделен весь холст");
    }

    /// Снимает выделение (Esc).
    pub fn deselect(&mut self) {
        self.selection = None;
        self.sel_drag = SelDrag::None;
    }

    fn other_color(&self) -> [u8; 4] {
        if self.color_swapped {
            self.primary
        } else {
            self.secondary
        }
    }

    fn stroke_color(&self, secondary: bool) -> [u8; 4] {
        if self.tool == Tool::Eraser {
            // Ластик стирает до прозрачности.
            return [0, 0, 0, 0];
        }
        let c = if secondary { self.other_color() } else { self.color() };
        if c[3] == 0 {
            // Прозрачный цветом рисовать нельзя — берём непрозрачный оттенок.
            [c[0], c[1], c[2], 255]
        } else {
            c
        }
    }

    fn fill_color(&self, secondary: bool) -> [u8; 4] {
        let c = if secondary { self.other_color() } else { self.color() };
        [c[0], c[1], c[2], if c[3] == 0 { 255 } else { c[3] }]
    }

    /// (размер кисти, мягкость, непрозрачность мазка)
    fn params_for_stroke(&self) -> (f32, f32, f32) {
        match self.tool {
            Tool::Pencil => (self.params.size.max(1.0), 1.0, self.params.opacity),
            Tool::Brush | Tool::Eraser => (self.params.size.max(1.0) * 0.5, self.params.hardness, self.params.opacity),
            Tool::Line | Tool::Rect | Tool::Ellipse => (self.params.size.max(1.0) * 0.5, 1.0, self.params.opacity),
            _ => (self.params.size.max(1.0) * 0.5, self.params.hardness, self.params.opacity),
        }
    }

    pub fn pick(&mut self, p: (f32, f32)) {
        let w = self.doc.width;
        let h = self.doc.height;
        let mut c = raster::pick(self.doc.active_layer(), w, h, p.0 as i32, p.1 as i32);
        if c[3] == 0 {
            c = [255, 255, 255, 255];
        }
        self.set_color(c);
        self.tool = Tool::Brush;
    }

    // --- история ---

    pub fn undo(&mut self) {
        self.end_stroke();
        let Some(a) = self.history.undo.pop() else { return };
        self.apply_inverse(&a);
        self.history.redo.push(a);
        self.doc.touch();
        self.dirty = true;
        self.notify("Отменено");
    }

    pub fn redo(&mut self) {
        self.end_stroke();
        let Some(a) = self.history.redo.pop() else { return };
        self.apply_forward(&a);
        self.history.undo.push(a);
        self.doc.touch();
        self.dirty = true;
        self.notify("Повторено");
    }

    fn apply_inverse(&mut self, a: &Action) {
        match a {
            Action::Pixels { layer, before, .. } => {
                if let Some(l) = self.doc.layers.get_mut(*layer) {
                    if l.pixels.len() == before.len() {
                        l.pixels.copy_from_slice(before);
                    }
                }
            }
            Action::LayerAdd { index } => {
                if *index < self.doc.layers.len() {
                    self.doc.layers.remove(*index);
                    self.doc.active = self.doc.active.min(self.doc.layers.len().saturating_sub(1));
                }
            }
            Action::LayerDelete { index, layer } => {
                let idx = (*index).min(self.doc.layers.len());
                self.doc.layers.insert(idx, (**layer).clone());
                self.doc.active = idx;
            }
            Action::LayerMove { from, to } => {
                let _ = (from, to);
            }
            Action::LayerMeta { index, before, .. } => {
                if let Some(l) = self.doc.layers.get_mut(*index) {
                    l.meta = before.clone();
                }
            }
        }
    }

    fn apply_forward(&mut self, a: &Action) {
        match a {
            Action::Pixels { layer, after, .. } => {
                if let Some(l) = self.doc.layers.get_mut(*layer) {
                    if l.pixels.len() == after.len() {
                        l.pixels.copy_from_slice(after);
                    }
                }
            }
            Action::LayerAdd { index } => {
                let n = self.doc.layers.len();
                self.doc.layers.insert((*index).min(n), Layer::new(self.doc.width, self.doc.height, "Слой"));
                self.doc.active = (*index).min(self.doc.layers.len() - 1);
            }
            Action::LayerDelete { index, .. } => {
                if *index < self.doc.layers.len() {
                    self.doc.layers.remove(*index);
                    self.doc.active = self.doc.active.min(self.doc.layers.len().saturating_sub(1));
                }
            }
            Action::LayerMove { from, to } => {
                let _ = (from, to);
            }
            Action::LayerMeta { index, after, .. } => {
                if let Some(l) = self.doc.layers.get_mut(*index) {
                    l.meta = after.clone();
                }
            }
        }
    }

    // --- слои ---

    pub fn add_layer(&mut self) {
        self.end_stroke();
        let n = self.doc.layers.len() + 1;
        let i = self.doc.add_layer(&format!("Слой {}", n));
        self.history.push(Action::LayerAdd { index: i });
        self.doc.touch();
        self.notify("Слой добавлен");
    }

    pub fn duplicate_layer(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        let i = self.doc.duplicate_layer(a);
        self.history.push(Action::LayerDelete { index: a, layer: Box::new(self.doc.layers[i].clone()) });
        self.doc.touch();
        self.notify("Слой продублирован");
    }

    pub fn delete_layer(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        if let Some(l) = self.doc.delete_layer(a) {
            self.history.push(Action::LayerDelete { index: a, layer: Box::new(l) });
            self.doc.touch();
            self.notify("Слой удалён");
        }
    }

    pub fn move_layer(&mut self, to: usize) {
        let from = self.doc.active;
        if from == to {
            return;
        }
        self.doc.move_layer(from, to);
        self.history.push(Action::LayerMove { from, to });
        self.notify("Порядок слоёв изменён");
    }

    pub fn set_layer_meta(&mut self, index: usize, f: impl FnOnce(&mut LayerMeta)) {
        let before = self.doc.layers[index].meta.clone();
        let mut after = before.clone();
        f(&mut after);
        if before == after {
            return;
        }
        self.doc.layers[index].meta = after.clone();
        self.doc.touch();
        self.history.push(Action::LayerMeta { index, before, after });
    }

    pub fn merge_down(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        if a == 0 || self.doc.layers.len() < 2 {
            return;
        }
        let top = self.doc.layers.remove(a);
        let (w, h) = (self.doc.width, self.doc.height);
        let under = self.doc.active_layer_mut();
        for i in 0..w * h {
            let c = [top.pixels[i * 4], top.pixels[i * 4 + 1], top.pixels[i * 4 + 2], top.pixels[i * 4 + 3]];
            if c[3] == 0 {
                continue;
            }
            under.blend(w, i % w, i / w, c);
        }
        self.doc.active = a - 1;
        self.history.push(Action::LayerDelete { index: a, layer: Box::new(top) });
        self.doc.touch();
        self.notify("Слои объединены");
    }

    pub fn clear_layer(&mut self) {
        self.end_stroke();
        let before = self.doc.active_layer().pixels.clone();
        self.doc.active_layer_mut().pixels.iter_mut().for_each(|v| *v = 0);
        let after = self.doc.layers[self.doc.active].pixels.clone();
        self.history.push(Action::Pixels { layer: self.doc.active, before, after });
        self.doc.touch();
        self.dirty = true;
        self.notify("Слой очищен");
    }

    // --- документ и файлы ---

    pub fn new_document(&mut self, w: usize, h: usize) {
        self.end_stroke();
        *self = Self { canvas_rect: self.canvas_rect, ..Self::new() };
        self.doc = Document::new(w, h);
        self.doc.layers[0].meta.name = "Фон".to_string();
        self.doc.layers[0].fill([255, 255, 255, 255]);
        self.doc.touch();
        self.history = History::new(40);
        self.path = None;
        self.dirty = false;
        self.fit_pending = true;
        self.notify("Новый документ");
    }

    /// Миниатюры слоёв для панели: атлас 8×8 ячеек по 64 px, RGBA.
    /// Заполняется заново при каждом изменении холста.
    pub fn build_thumb_atlas(&self) -> Vec<u8> {
        use crate::renderer::{THUMB_ATLAS, THUMB_CELL, THUMB_COLS};
        let mut atlas = vec![0u8; THUMB_ATLAS * THUMB_ATLAS * 4];
        for (idx, layer) in self.doc.layers.iter().enumerate() {
            let col = (idx % THUMB_COLS) * THUMB_CELL;
            let row = (idx / THUMB_COLS) * THUMB_CELL;
            let sw = self.doc.width.max(1) as f32;
            let sh = self.doc.height.max(1) as f32;
            for ty in 0..THUMB_CELL {
                // усреднение по области исходника, попадающей в строку миниатюры
                let y0 = (ty as f32 / THUMB_CELL as f32) * sh;
                let y1 = ((ty + 1) as f32 / THUMB_CELL as f32) * sh;
                for tx in 0..THUMB_CELL {
                    let x0 = (tx as f32 / THUMB_CELL as f32) * sw;
                    let x1 = ((tx + 1) as f32 / THUMB_CELL as f32) * sw;
                    let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
                    let mut n = 0u32;
                    let sx0 = x0.floor() as usize;
                    let sy0 = y0.floor() as usize;
                    let sx1 = (x1.ceil() as usize).min(self.doc.width);
                    let sy1 = (y1.ceil() as usize).min(self.doc.height);
                    for sy in sy0..sy1.max(sy0 + 1) {
                        for sx in sx0..sx1.max(sx0 + 1) {
                            let p = layer.get(self.doc.width, sx, sy);
                            r += p[0] as u32;
                            g += p[1] as u32;
                            b += p[2] as u32;
                            a += p[3] as u32;
                            n += 1;
                        }
                    }
                    let d = ((row + ty) * THUMB_ATLAS + col + tx) * 4;
                    if n == 0 {
                        continue;
                    }
                    atlas[d] = (r / n) as u8;
                    atlas[d + 1] = (g / n) as u8;
                    atlas[d + 2] = (b / n) as u8;
                    atlas[d + 3] = (a / n) as u8;
                }
            }
        }
        atlas
    }

    /// uv миниатюры слоя в атласе.
    pub fn thumb_uv(&self, idx: usize) -> [f32; 4] {
        use crate::renderer::{THUMB_ATLAS, THUMB_CELL, THUMB_COLS};
        let col = (idx % THUMB_COLS) * THUMB_CELL;
        let row = (idx / THUMB_COLS) * THUMB_CELL;
        let a = THUMB_ATLAS as f32;
        [col as f32 / a, row as f32 / a, (col + THUMB_CELL) as f32 / a, (row + THUMB_CELL) as f32 / a]
    }

    pub fn resize_canvas(&mut self, w: usize, h: usize) {
        self.end_stroke();
        self.doc.resize(w, h);
        self.history = History::new(40);
        self.fit_pending = true;
        self.notify(&format!("Размер холста: {}×{}", w, h));
    }

    pub fn save_png(&mut self, path: &str) -> Result<(), String> {
        self.doc.ensure_composite();
        let (w, h) = (self.doc.width, self.doc.height);
        let mut img = image::RgbImage::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                // Прозрачность композита заливаем белым — PNG без альфы.
                let a = self.doc.composite[i + 3] as f32 / 255.0;
                let c = [
                    (self.doc.composite[i] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                    (self.doc.composite[i + 1] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                    (self.doc.composite[i + 2] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                ];
                img.put_pixel(x as u32, y as u32, image::Rgb(c));
            }
        }
        img.save(path).map_err(|e| e.to_string())?;
        self.path = Some(path.to_string());
        self.dirty = false;
        Ok(())
    }

    pub fn open_png(&mut self, path: &str) -> Result<(), String> {
        let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let mut doc = Document::new(w, h);
        doc.layers[0].meta.name = "Фон".to_string();
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x as u32, y as u32);
                let i = (y * w + x) * 4;
                doc.layers[0].pixels[i] = p[0];
                doc.layers[0].pixels[i + 1] = p[1];
                doc.layers[0].pixels[i + 2] = p[2];
                doc.layers[0].pixels[i + 3] = p[3];
            }
        }
        doc.touch();
        self.doc = doc;
        self.history = History::new(40);
        self.path = Some(path.to_string());
        self.dirty = false;
        self.fit_pending = true;
        Ok(())
    }

    // --- работа с выделением ---

    /// Копирует содержимое выделения в буфер обмена.
    pub fn copy_selection(&mut self) {
        let Some(s) = self.selection else {
            self.notify("Сначала выделите область");
            return;
        };
        self.end_stroke();
        let (w, h) = (self.doc.width, self.doc.height);
        let data = raster::extract_rect(self.doc.active_layer(), w, h, s);
        if data.w == 0 {
            self.notify("Выделение вне холста");
            return;
        }
        self.clipboard = Some(data);
        self.notify("Скопировано");
    }

    /// Вырезает содержимое выделения в буфер обмена.
    pub fn cut_selection(&mut self) {
        let Some(s) = self.selection else {
            self.notify("Сначала выделите область");
            return;
        };
        self.end_stroke();
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        let before = self.doc.active_layer().pixels.clone();
        let (w, h) = (self.doc.width, self.doc.height);
        let layer = self.doc.active_layer_mut();
        let data = raster::extract_rect(layer, w, h, s);
        if data.w == 0 {
            self.notify("Выделение вне холста");
            return;
        }
        // Стираем выделение, записывая прозрачные пиксели напрямую:
        // обычный blend прозрачным цветом ничего не делает.
        raster::clear_rect(layer, w, h, s);
        let after = self.doc.layers[self.doc.active].pixels.clone();
        self.clipboard = Some(data);
        self.history.push(Action::Pixels { layer: self.doc.active, before, after });
        self.doc.touch();
        self.dirty = true;
        self.notify("Вырезано");
    }

    /// Вставляет буфер как «плавающий» фрагмент — его можно перетащить
    /// и затем щёлкнуть, чтобы закрепить на слое.
    pub fn paste_selection(&mut self) {
        let Some(data) = self.clipboard.clone() else {
            self.notify("Буфер обмена пуст");
            return;
        };
        self.end_stroke();
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        // Вставляем в начало выделения, а если его нет — по центру холста.
        let (px, py) = match self.selection {
            Some(s) => (s.x, s.y),
            None => (
                (self.doc.width as f32 - data.width() as f32) * 0.5,
                (self.doc.height as f32 - data.height() as f32) * 0.5,
            ),
        };
        self.floating = Some(Floating { data, x: px, y: py });
        self.float_dirty = true;
        self.float_moved = false;
        self.notify("Вставлено — перетащите и щёлкните для применения");
    }

    /// Удаляет содержимое выделения (Delete).
    pub fn delete_selection(&mut self) {
        let Some(s) = self.selection else { return };
        self.end_stroke();
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        let before = self.doc.active_layer().pixels.clone();
        let (w, h) = (self.doc.width, self.doc.height);
        let layer = self.doc.active_layer_mut();
        raster::clear_rect(layer, w, h, s);
        let after = self.doc.layers[self.doc.active].pixels.clone();
        if before != after {
            self.history.push(Action::Pixels { layer: self.doc.active, before, after });
            self.doc.touch();
            self.dirty = true;
            self.notify("Выделение удалено");
        }
    }

    /// Двигает плавающий фрагмент за курсором.
    pub fn floating_move(&mut self, p: (f32, f32)) {
        if let Some(f) = self.floating.as_mut() {
            let dx = p.0 - self.stroke_last.0;
            let dy = p.1 - self.stroke_last.1;
            if dx.abs() > 0.0 || dy.abs() > 0.0 {
                f.x += dx;
                f.y += dy;
                self.float_moved = true;
                self.float_dirty = true;
            }
            self.stroke_last = p;
        }
    }

    /// Закрепляет плавающий фрагмент на активном слое.
    pub fn floating_commit(&mut self) {
        let Some(f) = self.floating.take() else { return };
        let before = self.doc.active_layer().pixels.clone();
        let (w, h) = (self.doc.width, self.doc.height);
        let layer = self.doc.active_layer_mut();
        raster::blit_rect(layer, w, h, &f.data, f.x, f.y);
        let after = self.doc.layers[self.doc.active].pixels.clone();
        if before != after {
            self.history.push(Action::Pixels { layer: self.doc.active, before, after });
        }
        if let Some(s) = self.selection.as_mut() {
            s.x = f.x;
            s.y = f.y;
            s.w = f.data.width() as f32;
            s.h = f.data.height() as f32;
        } else {
            self.selection = Some(SelRect {
                x: f.x,
                y: f.y,
                w: f.data.width() as f32,
                h: f.data.height() as f32,
            });
        }
        self.float_moved = false;
        self.float_dirty = true;
        self.doc.touch();
        self.dirty = true;
        self.notify("Выделение применено");
    }

    /// Заливает область выделения текущим цветом.
    pub fn fill_selection(&mut self) {
        let Some(s) = self.selection else {
            self.notify("Сначала выделите область");
            return;
        };
        self.end_stroke();
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        let before = self.doc.active_layer().pixels.clone();
        let (w, h) = (self.doc.width, self.doc.height);
        let color = self.color();
        let opacity = self.params.opacity;
        let layer = self.doc.active_layer_mut();
        for y in (s.y as i32)..((s.y + s.h) as i32) {
            for x in (s.x as i32)..((s.x + s.w) as i32) {
                if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                    continue;
                }
                let mut c = color;
                c[3] = (c[3] as f32 * opacity) as u8;
                layer.blend(w, x as usize, y as usize, c);
            }
        }
        let after = self.doc.layers[self.doc.active].pixels.clone();
        if before != after {
            self.history.push(Action::Pixels { layer: self.doc.active, before, after });
        }
        self.doc.touch();
        self.dirty = true;
        self.notify("Область залита");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_restores_pixels_and_redo_reapplies() {
        let mut app = App::new();
        app.doc.resize(64, 64);
        let before = app.doc.layers[app.doc.active].pixels.clone();
        app.begin_stroke((32.0, 32.0), false);
        app.move_stroke((40.0, 40.0), false);
        app.end_stroke();
        assert_ne!(app.doc.layers[app.doc.active].pixels, before, "мазок ничего не нарисовал");

        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, before, "отмена не вернула слой");

        app.redo();
        assert_ne!(app.doc.layers[app.doc.active].pixels, before, "повтор не вернул мазок");
    }

    #[test]
    fn eyedropper_picks_active_layer_color() {
        let mut app = App::new();
        app.doc.resize(32, 32);
        app.doc.active_layer_mut().set(32, 10, 10, [12, 34, 56, 255]);
        app.tool = Tool::Eyedropper;
        app.begin_stroke((10.0, 10.0), false);
        assert_eq!(app.primary, [12, 34, 56, 255]);
    }

    #[test]
    fn view_transform_round_trip() {
        let mut app = App::new();
        app.canvas_rect = [0.0, 0.0, 800.0, 600.0];
        app.zoom = 2.0;
        app.pan = (100.0, 50.0);
        let p = app.canvas_to_screen(10.0, 20.0);
        let back = app.screen_to_canvas(p.0, p.1);
        assert!((back.0 - 10.0).abs() < 0.001 && (back.1 - 20.0).abs() < 0.001);
    }

    /// Готовим приложение: пустой холст нужного размера и белый слой.
    fn app_with_blank(w: usize, h: usize) -> App {
        let mut app = App::new();
        app.doc.resize(w, h);
        app.doc.active_layer_mut().pixels.fill(0);
        app.tool = Tool::Select;
        app
    }

    #[test]
    fn marquee_creates_and_normalizes_selection() {
        let mut app = app_with_blank(64, 64);
        app.begin_select((40.0, 10.0));
        app.move_select((20.0, 30.0));
        let s = app.selection.expect("рамка создана");
        assert_eq!((s.x, s.y, s.w, s.h), (20.0, 10.0, 20.0, 20.0));
        app.end_select();
        assert!(app.selection.is_some(), "рамка сохранилась после отпускания");
    }

    #[test]
    fn click_without_drag_clears_selection() {
        let mut app = app_with_blank(64, 64);
        app.selection = Some(SelRect::new(0.0, 0.0, 10.0, 10.0));
        app.begin_select((30.0, 30.0));
        app.end_select();
        assert!(app.selection.is_none(), "щелчок без протягивания снимает рамку");
    }

    #[test]
    fn copy_paste_and_commit_moves_pixels() {
        let mut app = app_with_blank(64, 64);
        // красим белый квадрат 10×10 в левом верхнем углу
        let w = 64;
        let layer = app.doc.active_layer_mut();
        for y in 0..10 {
            for x in 0..10 {
                layer.set(w, x, y, [255, 255, 255, 255]);
            }
        }
        app.selection = Some(SelRect::new(0.0, 0.0, 10.0, 10.0));
        app.copy_selection();
        let clip = app.clipboard.clone().expect("буфер заполнен");
        assert_eq!((clip.width(), clip.height()), (10, 10));

        app.selection = Some(SelRect::new(30.0, 30.0, 40.0, 40.0));
        app.paste_selection();
        let f = app.floating.as_ref().expect("фрагмент появился");
        assert_eq!((f.x, f.y), (30.0, 30.0));
        // до щелчка пиксели ещё не на слое
        assert_eq!(app.doc.active_layer().get(64, 35, 35)[3], 0);
        app.floating_commit();
        assert_eq!(app.doc.active_layer().get(64, 35, 35), [255, 255, 255, 255]);
    }

    #[test]
    fn dragging_selection_content_cuts_then_places() {
        let mut app = app_with_blank(64, 64);
        let w = 64;
        let layer = app.doc.active_layer_mut();
        for y in 10..20 {
            for x in 10..20 {
                layer.set(w, x, y, [10, 200, 30, 255]);
            }
        }
        app.selection = Some(SelRect::new(10.0, 10.0, 20.0, 20.0));
        // берём содержимое и тянем
        app.begin_select((15.0, 15.0));
        assert!(app.floating.is_some(), "содержимое вырезано в фрагмент");
        assert_eq!(app.doc.active_layer().get(64, 15, 15)[3], 0, "на месте осталась дырка");
        app.move_select((45.0, 45.0));
        app.end_select();
        assert_eq!(app.doc.active_layer().get(64, 45, 45), [10, 200, 30, 255], "фрагмент лёг на новое место");
        assert!(app.selection.is_some(), "выделение переехало вместе с фрагментом");
    }

    #[test]
    fn cut_and_delete_are_undoable() {
        let mut app = app_with_blank(32, 32);
        let w = 32;
        app.doc.active_layer_mut().set(w, 5, 5, [1, 2, 3, 255]);
        app.selection = Some(SelRect::new(0.0, 0.0, 10.0, 10.0));
        app.cut_selection();
        assert_eq!(app.doc.active_layer().get(32, 5, 5)[3], 0, "вырезано");
        app.undo();
        assert_eq!(app.doc.active_layer().get(32, 5, 5), [1, 2, 3, 255], "отмена вернула пиксель");

        app.selection = Some(SelRect::new(0.0, 0.0, 10.0, 10.0));
        app.delete_selection();
        assert_eq!(app.doc.active_layer().get(32, 5, 5)[3], 0, "удалено");
        app.undo();
        assert_eq!(app.doc.active_layer().get(32, 5, 5), [1, 2, 3, 255], "отмена вернула удаление");
    }

    #[test]
    fn select_all_covers_whole_canvas() {
        let mut app = app_with_blank(100, 50);
        app.select_all();
        let s = app.selection.expect("выделено всё");
        assert_eq!((s.x, s.y, s.w, s.h), (0.0, 0.0, 100.0, 50.0));
        app.deselect();
        assert!(app.selection.is_none());
    }

    #[test]
    fn fill_selection_paints_inside_only() {
        let mut app = app_with_blank(40, 40);
        app.primary = [255, 0, 0, 255];
        app.selection = Some(SelRect::new(10.0, 10.0, 20.0, 20.0));
        app.fill_selection();
        assert_eq!(app.doc.active_layer().get(40, 15, 15), [255, 0, 0, 255]);
        assert_eq!(app.doc.active_layer().get(40, 5, 5)[3], 0, "снаружи пусто");
    }
}
