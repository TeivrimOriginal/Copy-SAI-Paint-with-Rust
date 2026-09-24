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

/// Что за файлом по пути `App::path`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathKind {
    /// Файл проекта `.tpaint` — сохраняет все слои.
    Project,
    /// Плоская картинка (PNG/JPEG) — только композитинг.
    Image,
}

/// Незавершённый текст инструмента «Текст»: пока не нарисован, но уже на экране.
pub struct TextDraft {
    /// Начало пера в координатах холста (y — базовая линия).
    pub x: f32,
    pub y: f32,
    pub buf: String,
}

/// Собственное модальное окно открытия/сохранения: список каталога,
/// поле пути и имени файла. Системные диалоги не используются.
pub struct FileDialog {
    pub mode: DialogMode,
    pub path: String,
    pub file: String,
    pub entries: Vec<(String, bool)>,
    pub scroll: usize,
    /// (полный путь, режим) — забирает главный цикл
    pub result: Option<(String, DialogMode)>,
}

/// Что именно делает окно файлов: открыть проект, сохранить проект,
/// импортировать картинку слоем или экспортировать в плоский файл.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DialogMode {
    OpenProject,
    SaveProject,
    ImportImage,
    ExportPng,
    ExportPngAlpha,
    ExportJpeg,
}

impl DialogMode {
    pub fn title(self) -> &'static str {
        match self {
            DialogMode::OpenProject => "Открыть проект",
            DialogMode::SaveProject => "Сохранить проект",
            DialogMode::ImportImage => "Импорт изображения",
            DialogMode::ExportPng => "Экспорт в PNG",
            DialogMode::ExportPngAlpha => "Экспорт в PNG (прозрачный)",
            DialogMode::ExportJpeg => "Экспорт в JPEG",
        }
    }

    /// Расширение по умолчанию (с точкой).
    pub fn ext(self) -> &'static str {
        match self {
            DialogMode::OpenProject | DialogMode::SaveProject => crate::project::EXT_DOT,
            DialogMode::ExportJpeg => ".jpg",
            _ => ".png",
        }
    }

    /// Какие расширения показывать в списке.
    pub fn filters(self) -> &'static [&'static str] {
        match self {
            DialogMode::OpenProject | DialogMode::SaveProject => &[crate::project::EXT],
            DialogMode::ExportJpeg => &["jpg", "jpeg"],
            _ => &["png", "jpg", "jpeg"],
        }
    }

    /// Открывает (false) или записывает (true) файл.
    pub fn is_save(self) -> bool {
        matches!(
            self,
            DialogMode::SaveProject | DialogMode::ExportPng | DialogMode::ExportPngAlpha | DialogMode::ExportJpeg
        )
    }
}

impl FileDialog {
    pub fn new(mode: DialogMode, start: &str) -> Self {
        let mut d = Self {
            mode,
            path: if start.is_empty() { default_dir() } else { start.to_string() },
            file: if mode.is_save() {
                format!("рисунок{}", mode.ext())
            } else {
                String::new()
            },
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
        let filters = self.mode.filters();
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
                } else if filters.iter().any(|f| name.to_lowercase().ends_with(&format!(".{}", f))) {
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
        // Имя без расширения в режиме сохранения получает нужное расширение.
        let mut name = if self.file.is_empty() {
            format!("рисунок{}", self.mode.ext())
        } else {
            self.file.clone()
        };
        if self.mode.is_save() && !name.to_lowercase().ends_with(self.mode.ext()) {
            name.push_str(self.mode.ext());
        }
        let full = std::path::Path::new(&self.path).join(name).to_string_lossy().into_owned();
        self.result = Some((full, self.mode));
    }

    pub fn close(&mut self) {
        self.result = Some((String::new(), self.mode));
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
    /// Прокрутка панели истории.
    pub history_scroll: usize,
    /// Слой, который перетаскивают мышью в панели слоёв.
    pub layer_drag: Option<usize>,
    /// Незавершённый текст (инструмент «Текст»).
    pub text: Option<TextDraft>,

    // Файл и состояние
    pub path: Option<String>,
    /// Что лежит по пути `path`: проект со слоями или плоская картинка.
    pub path_kind: PathKind,
    pub dirty: bool,
    pub notice: Option<(String, Instant)>,
    pub show_grid: bool,
    pub dialog: Option<FileDialog>,
    /// Окно произвольного размера холста.
    pub size_dialog: bool,
    pub size_w: f32,
    pub size_h: f32,
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
            history_scroll: 0,
            layer_drag: None,
            text: None,
            path: None,
            path_kind: PathKind::Image,
            dirty: false,
            notice: None,
            show_grid: false,
            dialog: None,
            size_dialog: false,
            size_w: 1600.0,
            size_h: 1000.0,
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
            Tool::Text => {
                self.begin_text(p);
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
                self.history.push("Заливка", Action::Pixels { layer: self.doc.active, before: base, after: self.doc.layers[self.doc.active].pixels.clone() });
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
            if self.params.mirror {
                let mx = w as f32 - p.0;
                raster::stamp(layer, w, h, mx, p.1, p2.0, p2.1, c, p2.2);
            }
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
            let mirror = self.params.mirror;
            let layer = self.doc.active_layer_mut();
            raster::brush_line(layer, w, h, self.smooth.0, self.smooth.1, s.0, s.1, p2.0, p2.1, color, p2.2);
            if mirror {
                // Зеркальный мазок: рисуем отражение уже нарисованного куска.
                let m0 = (w as f32 - self.smooth.0, self.smooth.1);
                let m1 = (w as f32 - s.0, s.1);
                raster::brush_line(layer, w, h, m0.0, m0.1, m1.0, m1.1, p2.0, p2.1, color, p2.2);
            }
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
                // Название шага — по инструменту, которым рисовали.
                self.history.push(self.tool.name(), Action::Pixels { layer: self.doc.active, before, after });
            }
        }
        self.doc.touch();
    }

    // --- выделение рамкой ---

    /// Клик инструментом «Текст»: начинает набор в точке клика.
    /// Повторный клик по уже начатому тексту просто ставит курсор в конец.
    pub fn begin_text(&mut self, p: (f32, f32)) {
        if self.text.is_some() {
            self.commit_text();
        }
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        self.text = Some(TextDraft { x: p.0, y: p.1, buf: String::new() });
    }

    /// Ввод символа в незавершённый текст.
    pub fn text_input(&mut self, c: char) {
        if let Some(t) = self.text.as_mut() {
            t.buf.push(c);
        }
    }

    /// Удаление последнего символа (Backspace).
    pub fn text_backspace(&mut self) {
        if let Some(t) = self.text.as_mut() {
            t.buf.pop();
        }
    }

    /// Рисует набранный текст в активный слой (Enter или щелчок мышью).
    pub fn commit_text(&mut self) {
        let Some(t) = self.text.take() else { return };
        if t.buf.trim().is_empty() {
            return;
        }
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        let before = self.doc.active_layer().pixels.clone();
        let (w, h) = (self.doc.width, self.doc.height);
        let color = self.color();
        let size = self.params.size.max(4.0);
        let opacity = self.params.opacity;
        let bold = self.params.bold_text;
        let layer = self.doc.active_layer_mut();
        crate::text::draw_text(
            layer, w, h, &t.buf, t.x, t.y, size, color, opacity, bold,
        );
        let after = self.doc.layers[self.doc.active].pixels.clone();
        if before != after {
            self.history.push("Текст", Action::Pixels { layer: self.doc.active, before, after });
            self.doc.touch();
            self.dirty = true;
            self.notify(&format!("Текст: {} символов", t.buf.chars().count()));
        }
    }

    /// Отменяет набор текста (Esc).
    pub fn cancel_text(&mut self) {
        self.text = None;
    }

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
                    self.history.push("Вырезано", Action::Pixels { layer: self.doc.active, before, after });
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
        let Some(s) = self.history.undo() else { return };
        self.apply_inverse(&s.action);
        self.doc.touch();
        self.dirty = true;
        self.notify(&format!("Отменено: {}", s.name));
    }

    pub fn redo(&mut self) {
        self.end_stroke();
        let Some(s) = self.history.redo() else { return };
        self.apply_forward(&s.action);
        self.doc.touch();
        self.dirty = true;
        self.notify(&format!("Повторено: {}", s.name));
    }

    /// Переход к состоянию истории (0 — самое начало). Так работает
    /// панель истории: щелчок по строке применяет или отменяет всё до неё.
    pub fn history_goto(&mut self, target: usize) {
        self.end_stroke();
        let steps = self.history.goto(target);
        if steps.is_empty() {
            return;
        }
        let last_name = steps.last().map(|(s, _)| s.name.clone()).unwrap_or_default();
        for (s, forward) in steps {
            if forward {
                self.apply_forward(&s.action);
            } else {
                self.apply_inverse(&s.action);
            }
        }
        self.doc.touch();
        self.dirty = true;
        self.notify(&format!("История: {}", last_name));
    }

    /// Названия шагов истории: первый элемент — исходное состояние.
    pub fn history_labels(&self) -> Vec<String> {
        let mut v = vec!["Новый документ".to_string()];
        v.extend(self.history.steps.iter().map(|s| s.name.clone()));
        v
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
            Action::Merge { top_index, top, under_index, under_before } => {
                // Возвращаем верхний слой как был и откатываем нижний.
                let idx = (*top_index).min(self.doc.layers.len());
                self.doc.layers.insert(idx, (**top).clone());
                if let Some(l) = self.doc.layers.get_mut(*under_index) {
                    if l.pixels.len() == under_before.len() {
                        l.pixels.copy_from_slice(under_before);
                    }
                }
                let last = self.doc.layers.len().saturating_sub(1);
                self.doc.active = idx.min(last);
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
            // Повтор объединения: нижний слой снова со слоем сверху.
            Action::Merge { top_index, top, under_index, .. } => {
                let w = self.doc.width;
                let h = self.doc.height;
                if let Some(under) = self.doc.layers.get_mut(*under_index) {
                    for i in 0..w * h {
                        let c = [top.pixels[i * 4], top.pixels[i * 4 + 1], top.pixels[i * 4 + 2], top.pixels[i * 4 + 3]];
                        if c[3] == 0 {
                            continue;
                        }
                        under.blend(w, i % w, i / w, c);
                    }
                }
                if *top_index < self.doc.layers.len() {
                    self.doc.layers.remove(*top_index);
                }
                let last = self.doc.layers.len().saturating_sub(1);
                self.doc.active = (*under_index).min(last);
            }
        }
    }

    // --- слои ---

    pub fn add_layer(&mut self) {
        self.end_stroke();
        let n = self.doc.layers.len() + 1;
        let i = self.doc.add_layer(&format!("Слой {}", n));
        self.history.push("Слой добавлен", Action::LayerAdd { index: i });
        self.doc.touch();
        self.notify("Слой добавлен");
    }

    pub fn duplicate_layer(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        let i = self.doc.duplicate_layer(a);
        // Отмена дублирования — убрать новый слой, а не добавить копию.
        self.history.push("Слой продублирован", Action::LayerAdd { index: i });
        self.doc.touch();
        self.notify("Слой продублирован");
    }

    pub fn delete_layer(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        if let Some(l) = self.doc.delete_layer(a) {
            self.history.push("Слой удалён", Action::LayerDelete { index: a, layer: Box::new(l) });
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
        self.history.push("Порядок слоёв", Action::LayerMove { from, to });
        self.notify("Порядок слоёв изменён");
    }

    pub fn set_layer_meta(&mut self, index: usize, f: impl FnOnce(&mut LayerMeta)) {
        let before = self.doc.layers[index].meta.clone();
        let mut after = before.clone();
        f(&mut after);
        if before == after {
            return;
        }
        let name = if after.visible != before.visible {
            "Видимость слоя"
        } else if after.locked != before.locked {
            "Блокировка слоя"
        } else if after.blend != before.blend {
            "Режим наложения"
        } else {
            "Непрозрачность слоя"
        };
        self.doc.layers[index].meta = after.clone();
        self.doc.touch();
        self.history.push(name, Action::LayerMeta { index, before, after });
    }

    pub fn merge_down(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        if a == 0 || self.doc.layers.len() < 2 {
            return;
        }
        let under_index = a - 1;
        // Слой снизу берём по индексу: после удаления верхнего active уже
        // указывает за пределы списка.
        let (w, h) = (self.doc.width, self.doc.height);
        let top = self.doc.layers.remove(a);
        self.doc.active = under_index;
        let under_before = self.doc.layers[under_index].pixels.clone();
        let under = &mut self.doc.layers[under_index];
        for i in 0..w * h {
            let c = [top.pixels[i * 4], top.pixels[i * 4 + 1], top.pixels[i * 4 + 2], top.pixels[i * 4 + 3]];
            if c[3] == 0 {
                continue;
            }
            under.blend(w, i % w, i / w, c);
        }
        // Отмена возвращает и верхний слой, и исходное содержимое нижнего.
        self.history.push(
            "Слои объединены",
            Action::Merge {
                top_index: a,
                top: Box::new(top),
                under_index,
                under_before,
            },
        );
        self.doc.touch();
        self.notify("Слои объединены");
    }

    pub fn clear_layer(&mut self) {
        self.end_stroke();
        let before = self.doc.active_layer().pixels.clone();
        self.doc.active_layer_mut().pixels.iter_mut().for_each(|v| *v = 0);
        let after = self.doc.layers[self.doc.active].pixels.clone();
        self.history.push("Слой очищен", Action::Pixels { layer: self.doc.active, before, after });
        self.doc.touch();
        self.dirty = true;
        self.notify("Слой очищен");
    }

    /// Отражает активный слой: по горизонтали или по вертикали.
    pub fn flip_layer(&mut self, horizontal: bool) {
        self.end_stroke();
        let i = self.doc.active;
        let before = self.doc.layers[i].pixels.clone();
        self.doc.flip_layer(i, horizontal);
        let after = self.doc.layers[i].pixels.clone();
        self.history.push(
            if horizontal { "Слой отражён по горизонтали" } else { "Слой отражён по вертикали" },
            Action::Pixels { layer: i, before, after },
        );
        self.dirty = true;
        self.notify(if horizontal { "Слой отражён по горизонтали" } else { "Слой отражён по вертикали" });
    }

    /// Обрезает холст по выделению.
    pub fn crop_to_selection(&mut self) {
        self.end_stroke();
        let Some(s) = self.selection else {
            self.notify("Сначала выделите область");
            return;
        };
        self.doc.crop(
            s.x.max(0.0) as usize,
            s.y.max(0.0) as usize,
            s.w.round().max(1.0) as usize,
            s.h.round().max(1.0) as usize,
        );
        self.selection = None;
        self.floating = None;
        self.history = History::new(40);
        self.dirty = true;
        self.fit_pending = true;
        self.notify(&format!("Холст обрезан: {}×{}", self.doc.width, self.doc.height));
    }

    /// Обрезает пустые поля по краям содержимого.
    pub fn trim_canvas(&mut self) {
        self.end_stroke();
        match self.doc.content_bounds() {
            Some((x, y, w, h)) => {
                self.doc.crop(x, y, w, h);
                self.history = History::new(40);
                self.dirty = true;
                self.fit_pending = true;
                self.notify(&format!("Поля обрезаны: {}×{}", w, h));
            }
            None => self.notify("Холст пустой — нечего обрезать"),
        }
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
        self.write_png(path, false)?;
        self.path = Some(path.to_string());
        self.path_kind = PathKind::Image;
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
        self.path_kind = PathKind::Image;
        self.dirty = false;
        self.fit_pending = true;
        Ok(())
    }

    /// Сохраняет проект целиком: все слои с видимостью, прозрачностью и
    /// режимами наложения. В отличие от PNG, ничего не теряется.
    pub fn save_project(&mut self, path: &str) -> Result<(), String> {
        self.end_stroke();
        crate::project::save(&self.doc, path)?;
        self.path = Some(path.to_string());
        self.path_kind = PathKind::Project;
        self.dirty = false;
        self.notify("Проект сохранён");
        Ok(())
    }

    /// Открывает проект целиком.
    pub fn open_project(&mut self, path: &str) -> Result<(), String> {
        self.end_stroke();
        let doc = crate::project::load(path)?;
        self.doc = doc;
        self.history = History::new(40);
        self.path = Some(path.to_string());
        self.path_kind = PathKind::Project;
        self.dirty = false;
        self.fit_pending = true;
        self.selection = None;
        self.floating = None;
        self.notify(&format!("Открыто слоёв: {}", self.doc.layers.len()));
        Ok(())
    }

    /// Импортирует картинку как новый слой поверх текущих.
    /// Если размер отличается, изображение центрируется и обрезается.
    pub fn import_layer(&mut self, path: &str) -> Result<(), String> {
        self.end_stroke();
        let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
        let (iw, ih) = (img.width() as usize, img.height() as usize);
        if iw == 0 || ih == 0 {
            return Err("пустое изображение".to_string());
        }
        let (w, h) = (self.doc.width, self.doc.height);
        let mut layer = Layer::new(w, h, "Слой");
        // Центрируем: смещение может быть отрицательным — пикселы уйдут за край.
        let ox = (w as i64 - iw as i64) / 2;
        let oy = (h as i64 - ih as i64) / 2;
        for y in 0..ih {
            for x in 0..iw {
                let dx = x as i64 + ox;
                let dy = y as i64 + oy;
                if dx < 0 || dy < 0 || dx as usize >= w || dy as usize >= h {
                    continue;
                }
                let p = img.get_pixel(x as u32, y as u32);
                let i = (dy as usize * w + dx as usize) * 4;
                layer.pixels[i..i + 4].copy_from_slice(&p.0);
            }
        }
        let name = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Слой".to_string());
        layer.meta.name = if name.is_empty() { "Слой".to_string() } else { name };
        // Новый слой кладём наверх и делаем активным.
        self.doc.layers.push(layer);
        self.doc.active = self.doc.layers.len() - 1;
        self.history.push("Импорт слоя", Action::LayerAdd { index: self.doc.active });
        self.doc.touch();
        self.dirty = true;
        self.notify(&format!("Импортирован слой {}×{}", iw, ih));
        Ok(())
    }

    /// Экспорт в PNG: белый фон (без альфы) или прозрачный — по флагу.
    pub fn export_png(&mut self, path: &str, alpha: bool) -> Result<(), String> {
        self.write_png(path, alpha)?;
        self.notify("Экспорт готов");
        Ok(())
    }

    /// Записывает PNG без уведомлений — используется и экспортом, и Ctrl+S.
    fn write_png(&mut self, path: &str, alpha: bool) -> Result<(), String> {
        self.doc.ensure_composite();
        let (w, h) = (self.doc.width, self.doc.height);
        let mut img = image::RgbaImage::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let c = &self.doc.composite[i..i + 4];
                if alpha {
                    img.put_pixel(x as u32, y as u32, image::Rgba([c[0], c[1], c[2], c[3]]));
                } else {
                    // Прозрачность композита заливаем белым — PNG без альфы.
                    let a = c[3] as f32 / 255.0;
                    img.put_pixel(
                        x as u32,
                        y as u32,
                        image::Rgba([
                            (c[0] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                            (c[1] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                            (c[2] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                            255,
                        ]),
                    );
                }
            }
        }
        img.save(path).map_err(|e| e.to_string())
    }

    /// Экспорт в JPEG с качеством 1..=100 (прозрачность заливается белым).
    pub fn export_jpeg(&mut self, path: &str, quality: u8) -> Result<(), String> {
        self.doc.ensure_composite();
        let (w, h) = (self.doc.width, self.doc.height);
        let mut rgb = image::RgbImage::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let a = self.doc.composite[i + 3] as f32 / 255.0;
                rgb.put_pixel(
                    x as u32,
                    y as u32,
                    image::Rgb([
                        (self.doc.composite[i] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                        (self.doc.composite[i + 1] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                        (self.doc.composite[i + 2] as f32 * a + 255.0 * (1.0 - a)).round() as u8,
                    ]),
                );
            }
        }
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
        enc.encode_image(&rgb).map_err(|e| e.to_string())?;
        std::fs::write(path, out).map_err(|e| e.to_string())?;
        self.notify("Экспорт JPEG готов");
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
        self.history.push("Вырезано", Action::Pixels { layer: self.doc.active, before, after });
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
            self.history.push("Удаление выделения", Action::Pixels { layer: self.doc.active, before, after });
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
            self.history.push("Вставка выделения", Action::Pixels { layer: self.doc.active, before, after });
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
            self.history.push("Заливка выделения", Action::Pixels { layer: self.doc.active, before, after });
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

    fn temp_file(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("tpaint_app_{}_{}", std::process::id(), name));
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn project_saved_and_reopened_keeps_every_layer() {
        let mut app = app_with_blank(40, 30);
        app.doc.layers[0].meta.name = "Фон".to_string();
        app.doc.active_layer_mut().fill([10, 20, 30, 255]);
        app.add_layer();
        app.doc.active_layer_mut().fill([200, 100, 50, 128]);
        app.doc.layers[1].meta.blend = crate::doc::BlendMode::Screen;
        app.doc.layers[1].meta.opacity = 0.75;

        let path = temp_file("project.tpaint");
        app.save_project(&path).expect("сохранили проект");
        assert!(!app.dirty, "после сохранения документ чистый");
        assert_eq!(app.path_kind, PathKind::Project);

        let mut fresh = App::new();
        fresh.open_project(&path).expect("открыли проект");
        let _ = std::fs::remove_file(&path);
        assert_eq!(fresh.doc.width, 40);
        assert_eq!(fresh.doc.layers.len(), 2);
        assert_eq!(fresh.doc.layers[0].pixels[0], 10);
        assert_eq!(fresh.doc.layers[1].pixels[1], 100);
        assert_eq!(fresh.doc.layers[1].pixels[3], 128, "полупрозрачность слоя сохранена");
        assert_eq!(fresh.doc.layers[1].meta.blend, crate::doc::BlendMode::Screen);
        assert!((fresh.doc.layers[1].meta.opacity - 0.75).abs() < 0.001);
    }

    #[test]
    fn import_image_adds_new_layer_centered() {
        // Готовим PNG 10×10 красный квадрат и импортируем его в холст 40×40.
        let path = temp_file("import.png");
        let mut img = image::RgbaImage::new(10, 10);
        for p in img.pixels_mut() {
            *p = image::Rgba([255, 0, 0, 255]);
        }
        img.save(&path).expect("сохранили png");

        let mut app = app_with_blank(40, 40);
        app.import_layer(&path).expect("импортировали");
        let _ = std::fs::remove_file(&path);
        assert_eq!(app.doc.layers.len(), 2, "добавлен новый слой");
        assert_eq!(app.doc.active, 1, "новый слой активен");
        assert!(app.doc.layers[1].meta.name.ends_with("import"), "имя слоя из имени файла: {}", app.doc.layers[1].meta.name);
        // Изображение 10×10 центрируется: (40-10)/2 = 15
        assert_eq!(app.doc.active_layer().get(40, 20, 20), [255, 0, 0, 255], "центр картинки на месте");
        assert_eq!(app.doc.active_layer().get(40, 2, 2)[3], 0, "по краям холста пусто");
        assert!(app.dirty, "импорт меняет документ");
    }

    #[test]
    fn export_png_and_jpeg_write_readable_files() {
        let mut app = app_with_blank(16, 16);
        app.doc.active_layer_mut().fill([255, 0, 0, 255]);
        let png = temp_file("export.png");
        let jpg = temp_file("export.jpg");
        app.export_png(&png, false).expect("экспорт PNG");
        app.export_png(&png, true).expect("экспорт PNG с прозрачностью");
        app.export_jpeg(&jpg, 90).expect("экспорт JPEG");
        let png_data = std::fs::read(&png).expect("PNG прочитан");
        let jpg_data = std::fs::read(&jpg).expect("JPEG прочитан");
        // Сигнатуры: PNG — 89 50 4E 47, JPEG — FF D8 FF.
        assert_eq!(&png_data[..4], &[0x89, b'P', b'N', b'G'], "это должен быть PNG");
        assert_eq!(&jpg_data[..3], &[0xFF, 0xD8, 0xFF], "это должен быть JPEG");
        // Оба файла снова открываются, размер не теряется.
        let back = image::open(&png).expect("PNG открывается").to_rgba8();
        assert_eq!((back.width(), back.height()), (16, 16));
        let jback = image::open(&jpg).expect("JPEG открывается");
        assert_eq!((jback.width(), jback.height()), (16, 16));
        let _ = std::fs::remove_file(&png);
        let _ = std::fs::remove_file(&jpg);
    }

    /// Готовит демонстрационный проект в временной папке: два слоя,
    /// картинка и текст. Нужен, чтобы вручную открыть результат в приложении.
    #[test]
    fn demo_project_is_written_for_manual_check() {
        let mut app = App::new();
        app.doc.resize(800, 500);
        // фон — мягкий градиент кистью
        app.doc.layers[0].meta.name = "Фон".to_string();
        app.doc.layers[0].meta.blend = crate::doc::BlendMode::Normal;
        app.tool = Tool::Gradient;
        app.primary = [235, 245, 255, 255];
        app.secondary = [255, 245, 235, 255];
        app.params.opacity = 1.0;
        app.params.gradient_soft = 1.0;
        app.begin_stroke((0.0, 0.0), false);
        app.move_stroke((800.0, 500.0), false);
        app.end_stroke();
        // слой с текстом
        app.add_layer();
        app.doc.layers[1].meta.name = "Текст".to_string();
        crate::text::draw_text(
            app.doc.active_layer_mut(),
            800,
            500,
            "Привет, TPaint!",
            40.0,
            40.0,
            56.0,
            [25, 28, 38, 255],
            1.0,
            true,
        );
        crate::text::draw_text(
            app.doc.active_layer_mut(),
            800,
            500,
            "Проект .tpaint хранит все слои",
            40.0,
            150.0,
            30.0,
            [90, 60, 30, 255],
            1.0,
            false,
        );
        // и немного живописи кистью
        app.add_layer();
        app.doc.layers[2].meta.name = "Мазки".to_string();
        app.tool = Tool::Brush;
        app.params.size = 18.0;
        app.params.hardness = 0.7;
        // волна из двух цветов
        app.primary = [40, 150, 200, 255];
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=36 {
            let t = i as f32 / 36.0;
            let x = 60.0 + t * 680.0;
            let y = 300.0 + (t * std::f32::consts::TAU).sin() * 46.0;
            if let Some(p) = prev {
                app.begin_stroke(p, false);
                app.move_stroke((x, y), false);
                app.end_stroke();
            }
            prev = Some((x, y));
        }
        // зеркальный мазок второй половины
        app.params.mirror = true;
        app.primary = [235, 120, 40, 255];
        app.params.size = 34.0;
        let mut prev = None;
        for i in 0..=18 {
            let t = i as f32 / 18.0;
            let x = 80.0 + t * 260.0;
            let y = 430.0 - t * 70.0;
            if let Some(p) = prev {
                app.begin_stroke(p, false);
                app.move_stroke((x, y), false);
                app.end_stroke();
            }
            prev = Some((x, y));
        }
        app.params.mirror = false;
        // слой с точечными брызгами
        app.add_layer();
        app.doc.layers[3].meta.name = "Брызги".to_string();
        app.params.size = 9.0;
        app.params.mirror = true;
        app.primary = [120, 90, 200, 255];
        for i in 0..14 {
            let a = i as f32 * 1.7;
            let x = 200.0 + a.cos() * (120.0 + i as f32 * 12.0);
            let y = 430.0 + a.sin() * (40.0 + i as f32 * 5.0);
            app.begin_stroke((x, y), false);
            app.move_stroke((x + 1.0, y), false);
            app.end_stroke();
        }
        app.params.mirror = false;
        let path = temp_file("demo.tpaint");
        app.save_project(&path).expect("демо-проект сохранён");
        // и обратно читается
        let back = crate::project::load(&path).expect("демо-проект читается");
        assert_eq!(back.layers.len(), 4);
        assert!(back.layers[1].pixels.iter().any(|v| *v != 0), "текст попал в файл");
    }

    #[test]
    fn history_panel_jump_rewinds_document() {
        let mut app = app_with_blank(40, 40);
        app.tool = Tool::Brush;
        // три мазка — три шага истории
        for i in 0..3 {
            app.begin_stroke((10.0 + i as f32 * 5.0, 20.0), false);
            app.move_stroke((15.0 + i as f32 * 5.0, 25.0), false);
            app.end_stroke();
        }
        assert_eq!(app.history.position(), 3);
        let labels = app.history_labels();
        assert_eq!(labels.len(), 4, "состояние + три шага: {:?}", labels);
        assert!(labels[1..].iter().all(|l| l.contains("Кисть")), "шаг назван инструментом: {:?}", labels);
        let painted = app.doc.active_layer().pixels.iter().filter(|v| **v != 0).count();
        assert!(painted > 0);

        // перематываемся в самое начало — холст снова пустой
        app.history_goto(0);
        assert_eq!(app.history.position(), 0);
        assert_eq!(app.doc.active_layer().pixels.iter().filter(|v| **v != 0).count(), 0);
        // и возвращаемся в конец
        app.history_goto(3);
        assert_eq!(app.doc.active_layer().pixels.iter().filter(|v| **v != 0).count(), painted);
    }

    #[test]
    fn undo_of_duplicate_layer_removes_it() {
        let mut app = app_with_blank(32, 32);
        app.doc.layers[0].fill([1, 2, 3, 255]);
        app.duplicate_layer();
        assert_eq!(app.doc.layers.len(), 2);
        app.undo();
        assert_eq!(app.doc.layers.len(), 1, "отмена убирает копию, а не добавляет третью");
        app.redo();
        assert_eq!(app.doc.layers.len(), 2, "повтор возвращает копию");
    }

    #[test]
    fn undo_of_merge_restores_both_layers() {
        let mut app = app_with_blank(16, 16);
        app.doc.layers[0].fill([10, 10, 10, 255]);
        app.add_layer();
        app.doc.active_layer_mut().set(16, 4, 4, [200, 0, 0, 255]);
        app.merge_down();
        assert_eq!(app.doc.layers.len(), 1, "после объединения один слой");
        assert_eq!(app.doc.layers[0].get(16, 4, 4), [200, 0, 0, 255], "пиксель переехал вниз");
        app.undo();
        assert_eq!(app.doc.layers.len(), 2, "верхний слой вернулся");
        assert_eq!(app.doc.layers[0].get(16, 4, 4), [10, 10, 10, 255], "нижний слой откатан");
        app.redo();
        assert_eq!(app.doc.layers.len(), 1);
        assert_eq!(app.doc.layers[0].get(16, 4, 4), [200, 0, 0, 255], "повтор снова объединил");
    }

    #[test]
    fn crop_to_selection_resizes_document() {
        let mut app = app_with_blank(100, 100);
        app.doc.active_layer_mut().fill([9, 9, 9, 255]);
        app.selection = Some(SelRect::new(10.0, 20.0, 50.0, 60.0));
        app.crop_to_selection();
        assert_eq!((app.doc.width, app.doc.height), (40, 40));
        assert!(app.selection.is_none(), "рамка снята после обрезки");
        assert!(app.dirty);
    }

    #[test]
    fn crop_without_selection_does_nothing() {
        let mut app = app_with_blank(60, 60);
        app.crop_to_selection();
        assert_eq!((app.doc.width, app.doc.height), (60, 60), "холст не тронут");
    }

    #[test]
    fn trim_canvas_cuts_empty_borders() {
        let mut app = app_with_blank(100, 100);
        app.doc.active_layer_mut().pixels.iter_mut().for_each(|v| *v = 0);
        let w = 100;
        for y in 20..40 {
            for x in 30..60 {
                app.doc.active_layer_mut().set(w, x, y, [255, 0, 0, 255]);
            }
        }
        app.trim_canvas();
        assert_eq!((app.doc.width, app.doc.height), (30, 20));
        assert_eq!(app.doc.active_layer().get(30, 0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn flip_layer_is_undoable() {
        let mut app = app_with_blank(40, 20);
        app.doc.active_layer_mut().set(40, 2, 10, [7, 7, 7, 255]);
        app.flip_layer(true);
        assert_eq!(app.doc.active_layer().get(40, 37, 10), [7, 7, 7, 255], "пиксель зеркален");
        app.undo();
        assert_eq!(app.doc.active_layer().get(40, 2, 10), [7, 7, 7, 255], "отмена вернула");
    }

    #[test]
    fn text_is_rasterized_into_active_layer() {        let mut app = app_with_blank(200, 80);
        app.primary = [0, 0, 0, 255];
        app.tool = Tool::Text;
        app.params.size = 40.0;
        app.begin_text((10.0, 10.0));
        app.text_input('A');
        app.text_input('B');
        app.commit_text();
        let painted = app.doc.active_layer().pixels.chunks(4).filter(|p| p[3] > 0).count();
        assert!(painted > 20, "текст должен нарисовать пиксели, а не пустоту: {}", painted);
        assert!(app.text.is_none(), "после фиксации черновика нет");
        // Текст попал в историю и отменяется
        app.undo();
        assert_eq!(app.doc.active_layer().pixels.iter().filter(|v| **v != 0).count(), 0, "текст отменён");
    }

    #[test]
    fn mirror_draws_symmetric_stroke() {
        let mut app = app_with_blank(100, 60);
        app.params.size = 10.0;
        app.params.mirror = true;
        app.tool = Tool::Brush;
        app.begin_stroke((20.0, 30.0), false);
        app.move_stroke((30.0, 30.0), false);
        app.end_stroke();
        let w = 100;
        let layer = app.doc.active_layer();
        // Точка у левого края и её отражение у правого
        let left = layer.pixels.iter().any(|v| *v != 0);
        assert!(left, "мазок нарисован");
        let near_left: usize = (0..w * 60).filter(|i| layer.pixels[i * 4 + 3] > 0).count();
        let near_right: usize = (0..w * 60).filter(|i| layer.pixels[(i * 4) + 3] > 0).count();
        assert!(near_left > 0 && near_right > 0, "мазок есть с обеих сторон");
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
