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

/// Стабилизатор мазка: держит последние точки пера и строит дугу по
/// сглаженным опорным точкам.
///
/// Принцип: текущая точка мазка ВСЕГДА равна курсору (поэтому мазок не
/// отстаёт и не обрывается на быстром движении), а сглаживаются только
/// предыдущие опорные точки — от них зависит направление дуги. Дрожание
/// руки уходит в форму кривой, но конец мазка всегда точно под курсором.
#[derive(Clone, Copy)]
pub struct Stabilizer {
    /// Последние сырые точки пера, [0] — самая новая.
    hist: [(f32, f32); 4],
    /// Сколько точек накоплено.
    n: usize,
    /// Сглаженная позиция предыдущей точки — начало текущей дуги.
    prev_smooth: (f32, f32),
    /// Ещё более ранняя сглаженная точка — конец предыдущей дуги.
    prev2_smooth: (f32, f32),
    /// Текущая позиция мазка (равна курсору).
    pub pos: (f32, f32),
}

impl Default for Stabilizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Stabilizer {
    pub fn new() -> Self {
        Self { hist: [(0.0, 0.0); 4], n: 0, prev_smooth: (0.0, 0.0), prev2_smooth: (0.0, 0.0), pos: (0.0, 0.0) }
    }

    pub fn reset(&mut self, p: (f32, f32)) {
        self.hist = [p; 4];
        self.n = 1;
        self.prev_smooth = p;
        self.prev2_smooth = p;
        self.pos = p;
    }

    /// Взвешенное среднее по накопленным точкам: свежая весит вдвое больше
    /// предыдущей и вдвое больше следующей за ней.
    fn average(&self, keep: usize) -> (f32, f32) {
        let n = self.n.min(keep).max(1);
        let (mut sx, mut sy, mut sw) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..n {
            let w = 2f32.powi((n - 1 - i) as i32);
            sx += self.hist[i].0 * w;
            sy += self.hist[i].1 * w;
            sw += w;
        }
        (sx / sw, sy / sw)
    }

    /// Добавляет новую точку пера. Возвращает позицию мазка: она всегда равна
    /// самой точке, поэтому отставание невозможно в принципе.
    pub fn push(&mut self, p: (f32, f32), smoothing: f32) -> (f32, f32) {
        // Сколько точек участвует: 0 — только курсор (прямая линия),
        // 1 — все четыре, то есть сильное сглаживание формы.
        let keep = if smoothing <= 0.001 { 1 } else { 1 + (smoothing * 3.0).round() as usize };
        // Опорная точка дуги считается по истории ДО добавления новой точки.
        let smooth = if self.n == 0 { p } else { self.average(keep.max(1)) };
        self.prev2_smooth = smooth;
        self.prev_smooth = smooth;
        self.hist.rotate_right(1);
        self.hist[0] = p;
        self.n = (self.n + 1).min(4);
        self.pos = p;
        p
    }

    /// Четыре точки дуги Катмулла-Рома. Дуга всегда проходит через
    /// предыдущую и текущую сырые точки (поэтому мазок точно следует за
    /// мышью), а сглаженная точка задаёт направление входа — от неё
    /// зависит, насколько плавным получится поворот.
    pub fn curve(&self) -> [(f32, f32); 4] {
        let p2 = self.pos;
        let p1 = if self.n >= 2 { self.hist[1] } else { self.prev_smooth };
        let dir = (p2.0 - p1.0, p2.1 - p1.1);
        let p3 = if dir.0 * dir.0 + dir.1 * dir.1 > 0.0001 {
            (p2.0 + dir.0, p2.1 + dir.1)
        } else {
            p2
        };
        [self.prev_smooth, p1, p2, p3]
    }
}

/// Что за файлом по пути `App::path`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathKind {
    /// Файл проекта `.tpaint` — сохраняет все слои.
    Project,
    /// Плоская картинка (PNG/JPEG) — только композитинг.
    Image,
}

/// Свободная трансформация содержимого выделения: перенос, масштаб
/// за углы и поворот. `pts` — четыре угла результата в координатах холста.
pub struct Transform {
    pub data: RectData,
    /// Пиксели слоя до трансформации — из них каждый кадр собирается кадр.
    pub before: Vec<u8>,
    /// Исходный прямоугольник: x0, y0, x1, y1.
    pub src: [f32; 4],
    /// Углы результата: 0 — левый верхний, далее по часовой.
    pub pts: [(f32, f32); 4],
    /// Углы, для которых уже построен предпросмотр (чтобы не считать зря).
    pub shown: [(f32, f32); 4],
    /// Что схватили мышью: угол (0..3), перенос или поворот.
    pub grab: TransformGrab,
    /// Опорные точки при переносе.
    pub anchor: (f32, f32),
    /// Угол поворота в градусах на момент начала.
    pub start_angle: f32,
    /// Размер до поворота (для масштаба с поворотом).
    pub start_w: f32,
    pub start_h: f32,
    pub rot_start: [(f32, f32); 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransformGrab {
    None,
    /// Угол: индекс 0..3.
    Corner(usize),
    /// Перенос всей рамки.
    Move,
    /// Поворот вокруг центра.
    Rotate,
}

impl Transform {
    pub fn center(&self) -> (f32, f32) {
        (
            (self.pts[0].0 + self.pts[1].0 + self.pts[2].0 + self.pts[3].0) / 4.0,
            (self.pts[0].1 + self.pts[1].1 + self.pts[2].1 + self.pts[3].1) / 4.0,
        )
    }
}

/// Преобразование всего холста: повороты и отражения.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CanvasOp {
    Rotate90,
    Rotate180,
    Rotate270,
    MirrorH,
    MirrorV,
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
    /// Маска до мазка, если рисуем по маске слоя.
    pub mask_base: Option<Vec<u8>>,
    /// Рисование идёт по маске активного слоя, а не по пикселям.
    pub edit_mask: bool,
    pub smooth: (f32, f32),
    pub cursor: (f32, f32),
    /// Курсор мыши в экранных координатах (для рамки кисти и подсказок).
    pub cursor_screen: (f32, f32),
    pub cursor_in_canvas: bool,

    // Выделение рамкой и плавающий фрагмент после вставки
    pub selection: Option<SelRect>,
    /// Маска выделения в пикселях холста (0..255). None — выделен весь
    /// прямоугольник `selection`; иначе прямоугольник лишь границы области,
    /// а форма задана маской (эллипс, палочка, растушёвка).
    pub sel_mask: Option<Vec<u8>>,
    /// Счётчик изменения маски выделения — по нему GPU перезаливает текстуру.
    pub sel_mask_rev: u64,
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
    /// Вершины набираемого многоугольника в координатах холста.
    pub poly: Vec<(f32, f32)>,
    /// Многоугольник набирается (значит, рисуем его вершины и резиновую нить).
    pub poly_active: bool,
    /// Прокрутка списка слоёв (первая видимая строка сверху).
    pub layer_scroll: usize,
    /// Прокрутка панели истории.
    pub history_scroll: usize,
    /// Слой, который перетаскивают мышью в панели слоёв.
    pub layer_drag: Option<usize>,
    /// Активная свободная трансформация выделения.
    pub transform: Option<Transform>,
    /// Стабилизатор мазка: последние точки пера для сглаживания.
    pub stab: Stabilizer,
    /// Незавершённый текст (инструмент «Текст»).
    pub text: Option<TextDraft>,

    // Файл и состояние
    pub path: Option<String>,
    /// Что лежит по пути `path`: проект со слоями или плоская картинка.
    pub path_kind: PathKind,
    pub dirty: bool,
    pub notice: Option<(String, Instant)>,
    pub show_grid: bool,
    /// Шаг сетки в пикселях холста.
    pub grid_size: f32,
    /// Показывать линейки над и слева от холста.
    pub show_rulers: bool,
    /// Направляющие: (позиция в пикселях холста, true — горизонтальная).
    pub guides: Vec<(f32, bool)>,
    /// Какую направляющую тянут мышью: индекс в `guides`.
    pub guide_drag: Option<usize>,
    pub dialog: Option<FileDialog>,
    /// Окно произвольного размера холста.
    pub size_dialog: bool,
    pub size_w: f32,
    pub size_h: f32,
    /// Окно коррекции слоя (фильтры).
    pub filter_dialog: bool,
    /// Окно эффектов слоя: тень и обводка.
    pub fx_dialog: bool,
    /// Слой, который переименовывают прямо в панели (двойной щелчок).
    pub rename: Option<usize>,
    /// Черновик имени при переименовании.
    pub rename_buf: String,
    pub f_bright: f32,
    pub f_contrast: f32,
    pub f_saturate: f32,
    pub f_hue: f32,
    pub f_blur: f32,
    pub f_sharpen: f32,
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
            stab: Stabilizer::new(),
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
            mask_base: None,
            edit_mask: false,
            smooth: (0.0, 0.0),
            cursor: (0.0, 0.0),
            cursor_screen: (0.0, 0.0),
            cursor_in_canvas: false,
            selection: None,
        sel_mask: None,
        sel_mask_rev: 0,
            clipboard: None,
            floating: None,
            ants_phase: 0.0,
            float_dirty: false,
            sel_drag: SelDrag::None,
            float_moved: false,
            poly: Vec::new(),
            poly_active: false,
            layer_scroll: 0,
            history_scroll: 0,
            layer_drag: None,
            transform: None,
            text: None,
            path: None,
            path_kind: PathKind::Image,
            dirty: false,
            notice: None,
            show_grid: false,
            grid_size: 64.0,
            show_rulers: true,
            guides: Vec::new(),
            guide_drag: None,
            dialog: None,
            size_dialog: false,
            size_w: 1600.0,
            size_h: 1000.0,
            filter_dialog: false,
        fx_dialog: false,
        rename: None,
        rename_buf: String::new(),
            f_bright: 0.0,
            f_contrast: 0.0,
            f_saturate: 0.0,
            f_hue: 0.0,
            f_blur: 0.0,
            f_sharpen: 0.0,
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
            Tool::EllipseSelect => {
                self.begin_select(p);
                return;
            }
            Tool::Wand => {
                // Shift добавляет область к текущей, иначе заменяет её.
                self.wand_select(p, false);
                return;
            }
            Tool::Polygon => {
                // Вершины ставятся кликами, протягивание ничего не делает.
                self.poly_click(p);
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
        // Рисование по маске: белым открываем, ластиком закрываем.
        if self.edit_mask {
            if !self.tool.is_freehand() {
                self.notify("По маске рисует кисть или ластик");
                return;
            }
            let has_mask = self.doc.active_layer().mask.is_some();
            if !has_mask {
                self.edit_mask = false;
                self.notify("У слоя нет маски");
                return;
            }
            self.mask_base = self.doc.active_layer().mask.clone();
            self.drawing = true;
            self.stroke_start = p;
            self.stroke_last = p;
            self.smooth = p;
            self.stab.reset(p);
            let (w, h) = (self.doc.width, self.doc.height);
            let value = if self.tool == Tool::Eraser { 0 } else { 255 };
            let (r, hard, op) = (self.params.size / 2.0, self.params.hardness, self.params.opacity);
            let shape = self.brush_shape();
            if let Some(m) = self.doc.active_layer_mut().mask.as_mut() {
                raster::mask_stamp_shape(m, w, h, p.0, p.1, r, hard, value, op, shape);
            }
            self.doc.touch();
            return;
        }
        self.stroke_base = Some(self.doc.active_layer().pixels.clone());
        self.drawing = true;
        self.stroke_start = p;
        self.stroke_last = p;
        self.smooth = p;
        self.stab.reset(p);
        let _ = color;
        // Первая точка: для карандаша/кисти сразу ставим dab, иначе ждём движения.
        if self.tool.is_freehand() {
            let p2 = self.params_for_stroke();
            let c = self.stroke_color(secondary);
            let (w, h) = (self.doc.width, self.doc.height);
            let shape = self.brush_shape();
            let layer = self.doc.active_layer_mut();
            raster::stamp_shape(layer, w, h, p.0, p.1, p2.0, p2.1, c, p2.2, shape);
            if self.params.mirror {
                let mx = w as f32 - p.0;
                raster::stamp_shape(layer, w, h, mx, p.1, p2.0, p2.1, c, p2.2, shape);
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

        // Мазок по маске: тот же стабилизатор, но в градации серого.
        if self.mask_base.is_some() {
            let s = self.stab.push(p, self.params.smoothing);
            let value = if tool == Tool::Eraser { 0 } else { 255 };
            let (r, hard, op) = (p2.0, p2.1, p2.2);
            let shape = self.brush_shape();
            let spacing = (p2.0 * self.params.spacing).max(0.4);
            let curve = self.stab.curve();
            if let Some(m) = self.doc.active_layer_mut().mask.as_mut() {
                let mut mm = std::mem::take(m);
                raster::stamp_curve(
                    |x, y| raster::mask_stamp_shape(&mut mm, w, h, x, y, r, hard, value, op, shape),
                    curve[0], curve[1], curve[2], curve[3], spacing,
                );
                self.doc.active_layer_mut().mask = Some(mm);
            }
            self.smooth = s;
            self.stroke_last = p;
            self.doc.touch();
            self.dirty = true;
            return;
        }

        if tool.is_freehand() {
            // Стабилизатор сглаживает дрожание, дуга Катмулла-Рома ведёт
            // мазок по кривой, а шаг между отпечатками задан в пикселях —
            // поэтому быстрый и медленный мазок имеют одинаковую плотность.
            let s = self.stab.push(p, self.params.smoothing);
            let spacing = (p2.0 * self.params.spacing).max(0.4);
            let curve = self.stab.curve();
            let mirror = self.params.mirror;
            let shape = self.brush_shape();
            let (r, hard, op) = (p2.0, p2.1, p2.2);
            let layer = self.doc.active_layer_mut();
            raster::stamp_curve(
                |x, y| raster::stamp_shape(layer, w, h, x, y, r, hard, color, op, shape),
                curve[0], curve[1], curve[2], curve[3], spacing,
            );
            if mirror {
                // Зеркальный мазок: рисуем отражение уже нарисованного куска.
                let m0 = (w as f32 - curve[0].0, curve[0].1);
                let m1 = (w as f32 - curve[1].0, curve[1].1);
                let m2 = (w as f32 - curve[2].0, curve[2].1);
                let m3 = (w as f32 - curve[3].0, curve[3].1);
                raster::stamp_curve(
                    |x, y| raster::stamp_shape(layer, w, h, x, y, r, hard, color, op, shape),
                    m0, m1, m2, m3, spacing,
                );
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
        if let Some(before) = self.mask_base.take() {
            let after = self.doc.layers[self.doc.active].mask.clone();
            if Some(before.clone()) != after {
                let name = if self.tool == Tool::Eraser { "Ластик по маске" } else { "Кисть по маске" };
                self.history.push(name, Action::Mask { layer: self.doc.active, before: Some(before), after });
            }
        } else if let Some(before) = self.stroke_base.take() {
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
    /// Для эллипса форма выделения задаётся маской, а не рамкой.
    pub fn begin_select(&mut self, p: (f32, f32)) {
        if self.tool == Tool::EllipseSelect {
            self.clear_selection();
            self.selection = Some(SelRect::new(p.0, p.1, p.0, p.1));
            self.sel_drag = SelDrag::Marquee;
            self.stroke_start = p;
            self.notify("");
            return;
        }
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
                if self.tool == Tool::EllipseSelect {
                    if let Some(s) = self.selection {
                        self.set_sel_mask(raster::ellipse_mask(
                            self.doc.width,
                            self.doc.height,
                            s,
                        ));
                    }
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

    /// Клик по вершине многоугольника. Первый клик начинает контур, клик рядом
    /// с первой вершиной (или Enter) замыкает его.
    pub fn poly_click(&mut self, p: (f32, f32)) {
        if self.poly.len() >= 3 {
            let f = self.poly[0];
            if (p.0 - f.0).abs() < 6.0 / self.zoom.max(0.05) && (p.1 - f.1).abs() < 6.0 / self.zoom.max(0.05) {
                self.finish_polygon();
                return;
            }
        }
        self.poly.push(p);
        self.poly_active = true;
        self.notify(&format!("Вершин: {} — Enter, чтобы замкнуть", self.poly.len()));
    }

    /// Замыкает контур и превращает его в выделение.
    pub fn finish_polygon(&mut self) {
        if self.poly.len() < 3 {
            self.poly.clear();
            self.poly_active = false;
            self.notify("Нужно минимум три вершины");
            return;
        }
        let pts = std::mem::take(&mut self.poly);
        self.poly_active = false;
        let (w, h) = (self.doc.width, self.doc.height);
        self.set_sel_mask(raster::polygon_mask(w, h, &pts));
        // Рамка выделения — это границы фигуры, форма живёт в маске.
        let x0 = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let y0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let x1 = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let y1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        self.selection = Some(SelRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
        self.notify(&format!(
            "Выделен многоугольник {}×{}",
            (x1 - x0) as i32, (y1 - y0) as i32
        ));
    }

    /// Бросает недобранный контур (Esc).
    pub fn cancel_polygon(&mut self) {
        if self.poly_active || !self.poly.is_empty() {
            self.poly.clear();
            self.poly_active = false;
            self.notify("Контур сброшен");
        }
    }

    /// «Волшебная палочка»: выделяет пиксели, близкие по цвету к точке клика.
    pub fn wand_select(&mut self, p: (f32, f32), add: bool) {
        if !self.inside(p.0, p.1) {
            return;
        }
        let (w, h) = (self.doc.width, self.doc.height);
        let mut mask = if add { self.sel_mask.clone().unwrap_or_else(|| vec![0u8; w * h]) } else { vec![0u8; w * h] };
        // Работаем по активному слою: палочка выбирает то, что на слое.
        let tol = self.params.tolerance.clamp(0.0, 255.0) as i32;
        let layer = self.doc.active_layer();
        raster::wand(layer, &mut mask, w, h, p.0 as i32, p.1 as i32, tol, self.params.contiguous);
        self.set_sel_mask(mask);
        match self.selection {
            Some(s) => self.notify(&format!("Выделено {}×{} пикселей", s.w as i32, s.h as i32)),
            None => self.notify("Под курсором нечего выделять"),
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
        self.clear_selection();
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
        self.cancel_polygon();
        self.clear_selection();
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

    /// Форма отпечатка кисти: у карандаша всегда круг.
    pub fn brush_shape(&self) -> raster::Shape {
        use crate::tools::BrushShape;
        match (self.tool, self.params.shape) {
            (Tool::Pencil, _) => raster::Shape::Round,
            (_, BrushShape::Ellipse) => raster::Shape::Ellipse,
            (_, BrushShape::Square) => raster::Shape::Square,
            _ => raster::Shape::Round,
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
        // Пока идёт трансформация, отмена возвращает её исходный слой.
        if self.transform.is_some() {
            self.transform_cancel();
            return;
        }
        let Some(s) = self.history.undo() else { return };
        self.apply_inverse(&s.action);
        self.doc.touch();
        self.dirty = true;
        self.notify(&format!("Отменено: {}", s.name));
    }

    pub fn redo(&mut self) {
        self.end_stroke();
        if self.transform.is_some() {
            self.transform_cancel();
            return;
        }
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
        // Прыжок по истории во время трансформации сначала возвращает слой.
        if self.transform.is_some() {
            self.transform_cancel();
        }
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
            Action::Mask { layer, before, .. } => {
                if let Some(l) = self.doc.layers.get_mut(*layer) {
                    l.mask = before.clone();
                    if before.is_some() {
                        l.mask_on = true;
                    }
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
            Action::Document { before, .. } => {
                self.doc = *before.clone();
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
            Action::Mask { layer, after, .. } => {
                if let Some(l) = self.doc.layers.get_mut(*layer) {
                    l.mask = after.clone();
                    if after.is_some() {
                        l.mask_on = true;
                    }
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
            Action::Document { after, .. } => {
                self.doc = *after.clone();
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

    // --- эффекты слоя, метки и переименование ---

    /// Открывает окно эффектов активного слоя.
    pub fn open_fx(&mut self) {
        self.end_stroke();
        self.fx_dialog = true;
    }

    /// Переключает следующую метку цвета слоя (0 → красная → … → нет).
    pub fn cycle_tag(&mut self) {
        let i = self.doc.active;
        self.set_layer_meta(i, |m| m.tag = if m.tag >= 6 { 0 } else { m.tag + 1 });
    }

    /// Начинает переименование слоя прямо в панели.
    pub fn begin_rename(&mut self) {
        let i = self.doc.active;
        self.rename_buf = self.doc.layer_name(i);
        self.rename = Some(i);
    }

    /// Применяет переименование (Enter) или отменяет (Esc).
    pub fn finish_rename(&mut self, save: bool) {
        let Some(i) = self.rename.take() else { return };
        if save {
            let name = self.rename_buf.trim().to_string();
            if !name.is_empty() && i < self.doc.layers.len() {
                self.set_layer_meta(i, |m| m.name = name);
            }
        }
        self.rename_buf.clear();
    }

    /// Снимает/ставит тень активного слоя.
    pub fn toggle_fx_shadow(&mut self) {
        let i = self.doc.active;
        let has = self.doc.layers[i].meta.fx.shadow;
        self.set_layer_meta(i, |m| m.fx.shadow = !has);
        self.notify(if has { "Тень убрана" } else { "Тень включена" });
    }

    /// Снимает/ставит обводку активного слоя.
    pub fn toggle_fx_outline(&mut self) {
        let i = self.doc.active;
        let has = self.doc.layers[i].meta.fx.outline;
        self.set_layer_meta(i, |m| m.fx.outline = !has);
        self.notify(if has { "Обводка убрана" } else { "Обводка включена" });
    }

    // --- группы слоёв (папки) ---

    /// Создаёт папку: активный слой и все слои выше него (до конца списка)
    /// становятся содержимым папки. Шапка папки — самый верхний слой группы,
    /// в панели слоёв он рисуется над содержимым, как в Clip Studio.
    pub fn make_group(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        if a + 1 >= self.doc.layers.len() {
            self.notify("Для папки нужен слой над активным");
            return;
        }
        let n = self.doc.layers.iter().filter(|l| l.meta.group.is_some()).count() / 2 + 1;
        let name = format!("Группа {}", n);
        for l in self.doc.layers.iter_mut().skip(a) {
            l.meta.group = Some(name.clone());
        }
        self.history = History::new(40);
        self.doc.touch();
        self.notify(&format!("Папка «{}» создана", name));
    }

    /// Убирает слои из папок.
    pub fn ungroup(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        if self.doc.layers[a].meta.group.is_none() {
            self.notify("Слой не в папке");
            return;
        }
        for l in self.doc.layers.iter_mut() {
            l.meta.group = None;
            l.meta.group_open = true;
        }
        self.history = History::new(40);
        self.doc.touch();
        self.notify("Папка распущена");
    }

    /// Раскрыта ли папка, чьей шапкой является слой.
    pub fn group_open(&self, index: usize) -> bool {
        self.doc.layers.get(index).map_or(true, |l| l.meta.group_open)
    }

    /// Раскрытие/сворачивание папки по её шапке.
    pub fn toggle_group(&mut self, index: usize) {
        if index < self.doc.layers.len() {
            let open = !self.doc.layers[index].meta.group_open;
            self.doc.layers[index].meta.group_open = open;
        }
    }

    /// Является ли слой шапкой папки: он верхний в своей группе.
    pub fn is_group_header(&self, index: usize) -> bool {
        let Some(name) = self.doc.layers.get(index).and_then(|l| l.meta.group.clone()) else {
            return false;
        };
        self.doc
            .layers
            .get(index + 1)
            .and_then(|l| l.meta.group.as_deref())
            .map_or(true, |next| next != name)
    }

    /// Слои папки: шапка и всё её содержимое (сверху вниз по индексу).
    pub fn group_members(&self, header: usize) -> Vec<usize> {
        let Some(name) = self.doc.layers.get(header).and_then(|l| l.meta.group.clone()) else {
            return Vec::new();
        };
        let mut out = vec![header];
        let mut i = header as isize - 1;
        while i >= 0 {
            match self.doc.layers[i as usize].meta.group.as_deref() {
                Some(n) if n == name => {
                    out.push(i as usize);
                    i -= 1;
                }
                _ => break,
            }
        }
        out
    }

    /// Показывается ли слой в панели: содержимое закрытой папки скрыто.
    pub fn layer_visible_in_panel(&self, index: usize) -> bool {
        let Some(name) = self.doc.layers.get(index).and_then(|l| l.meta.group.clone()) else {
            return true;
        };
        // Идём вверх по индексу до конца группы — там её шапка.
        for h in (index + 1)..self.doc.layers.len() {
            if self.doc.layers[h].meta.group.as_deref() != Some(name.as_str()) {
                break;
            }
            if self.is_group_header(h) {
                return self.doc.layers[h].meta.group_open;
            }
        }
        true
    }

    /// Скрывает или показывает все слои папки вместе с её шапкой.
    pub fn toggle_group_visibility(&mut self, header: usize) {
        let members = self.group_members(header);
        if members.is_empty() {
            return;
        }
        let any_visible = members.iter().any(|i| self.doc.layers[*i].meta.visible);
        let want = !any_visible;
        let before: Vec<LayerMeta> = members.iter().map(|i| self.doc.layers[*i].meta.clone()).collect();
        for i in &members {
            self.doc.layers[*i].meta.visible = want;
        }
        let after: Vec<LayerMeta> = members.iter().map(|i| self.doc.layers[*i].meta.clone()).collect();
        for (k, i) in members.iter().enumerate() {
            self.history.push(
                if want { "Папка показана" } else { "Папка скрыта" },
                Action::LayerMeta { index: *i, before: before[k].clone(), after: after[k].clone() },
            );
        }
        self.doc.touch();
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

    // --- маски слоёв ---

    /// Выбирает активный слой. Если у нового слоя нет маски, режим правки
    /// маски выключается — иначе кисть красила бы не туда.
    pub fn select_layer(&mut self, i: usize) {
        self.doc.active = i.min(self.doc.layers.len().saturating_sub(1));
        if self.doc.active_layer().mask.is_none() {
            self.edit_mask = false;
        }
    }

    /// Прижимает слой к нижнему (слой-маска) или отпускает обратно.
    pub fn toggle_clip(&mut self) {
        self.end_stroke();
        let a = self.doc.active;
        if a == 0 {
            self.notify("Нижний слой прижать не к чему");
            return;
        }
        let on = !self.doc.layers[a].meta.clipped;
        self.set_layer_meta(a, |m| m.clipped = on);
        self.notify(if on { "Слой прижат к нижнему" } else { "Слой свободен" });
    }

    /// Создаёт маску активного слоя из текущего выделения.
    /// Без выделения маска полностью белая (весь слой виден).
    pub fn add_mask_from_selection(&mut self) {
        self.end_stroke();
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        let n = self.doc.width * self.doc.height;
        let mut mask = vec![0u8; n];
        match self.sel_mask.clone() {
            // Форма выделения важнее его рамки: маска повторяет область.
            Some(m) => mask = m,
            None => match self.selection {
                Some(s) => {
                    let (w, h) = (self.doc.width, self.doc.height);
                    let x0 = s.x.max(0.0).floor() as usize;
                    let y0 = s.y.max(0.0).floor() as usize;
                    let x1 = ((s.x + s.w).ceil().max(0.0) as usize).min(w);
                    let y1 = ((s.y + s.h).ceil().max(0.0) as usize).min(h);
                    for y in y0..y1 {
                        for x in x0..x1 {
                            mask[y * w + x] = 255;
                        }
                    }
                }
                None => mask.iter_mut().for_each(|v| *v = 255),
            },
        }
        let before = self.doc.active_layer().mask.clone();
        let layer = self.doc.active_layer_mut();
        layer.mask = Some(mask);
        layer.mask_on = true;
        self.history.push(
            if self.selection.is_some() { "Маска из выделения" } else { "Маска слоя" },
            Action::Mask { layer: self.doc.active, before, after: self.doc.layers[self.doc.active].mask.clone() },
        );
        self.edit_mask = true;
        self.doc.touch();
        self.dirty = true;
        self.notify("Маска добавлена — рисуем по ней белым");
    }

    /// Белая или чёрная заливка маски (с учётом текущего выделения).
    pub fn fill_mask(&mut self, white: bool) {
        self.end_stroke();
        if !self.ensure_mask() {
            return;
        }
        let n = self.doc.width * self.doc.height;
        let before = self.doc.active_layer().mask.clone();
        let value = if white { 255u8 } else { 0u8 };
        let mut mask = before.clone().unwrap_or(vec![0u8; n]);
        match self.sel_mask.clone() {
            Some(sel) => {
                for (m, s) in mask.iter_mut().zip(sel.iter()) {
                    if *s > 8 {
                        *m = value;
                    }
                }
            }
            None => match self.selection {
                Some(s) => {
                    let (w, h) = (self.doc.width, self.doc.height);
                    let x0 = s.x.max(0.0).floor() as usize;
                    let y0 = s.y.max(0.0).floor() as usize;
                    let x1 = ((s.x + s.w).ceil().max(0.0) as usize).min(w);
                    let y1 = ((s.y + s.h).ceil().max(0.0) as usize).min(h);
                    for y in y0..y1 {
                        for x in x0..x1 {
                            mask[y * w + x] = value;
                        }
                    }
                }
                None => mask.iter_mut().for_each(|v| *v = value),
            },
        }
        self.doc.active_layer_mut().mask = Some(mask.clone());
        self.history.push(
            if white { "Маска: белое" } else { "Маска: чёрное" },
            Action::Mask { layer: self.doc.active, before, after: Some(mask) },
        );
        self.doc.touch();
        self.dirty = true;
    }

    /// Инвертирует маску слоя.
    pub fn invert_mask(&mut self) {
        self.end_stroke();
        if !self.ensure_mask() {
            return;
        }
        let before = self.doc.active_layer().mask.clone();
        let mut after = before.clone().unwrap_or_default();
        after.iter_mut().for_each(|v| *v = 255 - *v);
        self.doc.active_layer_mut().mask = Some(after.clone());
        self.history.push("Инверсия маски", Action::Mask { layer: self.doc.active, before, after: Some(after) });
        self.doc.touch();
        self.dirty = true;
    }

    /// Включает/выключает маску, не теряя её.
    pub fn toggle_mask(&mut self) {
        self.end_stroke();
        let idx = self.doc.active;
        if self.doc.layers[idx].mask.is_none() {
            self.notify("У слоя нет маски");
            return;
        }
        self.doc.layers[idx].mask_on = !self.doc.layers[idx].mask_on;
        self.doc.touch();
        self.dirty = true;
        self.notify(if self.doc.layers[idx].mask_on { "Маска включена" } else { "Маска выключена" });
    }

    /// Удаляет маску слоя (с отменой).
    pub fn delete_mask(&mut self) {
        self.end_stroke();
        let idx = self.doc.active;
        let before = self.doc.layers[idx].mask.clone();
        if before.is_none() {
            self.notify("У слоя нет маски");
            return;
        }
        self.doc.layers[idx].mask = None;
        self.doc.layers[idx].mask_on = true;
        self.history.push("Удалить маску", Action::Mask { layer: idx, before, after: None });
        self.edit_mask = false;
        self.doc.touch();
        self.dirty = true;
        self.notify("Маска удалена");
    }

    /// Создаёт пустую маску, если её ещё нет. false — если слой заблокирован.
    fn ensure_mask(&mut self) -> bool {
        let idx = self.doc.active;
        if self.doc.layers[idx].meta.locked {
            self.notify("Слой заблокирован");
            return false;
        }
        if self.doc.layers[idx].mask.is_none() {
            let n = self.doc.width * self.doc.height;
            self.doc.layers[idx].mask = Some(vec![255u8; n]);
            self.doc.layers[idx].mask_on = true;
            self.edit_mask = true;
        }
        true
    }

    /// Включает режим рисования по маске (нужна маска на активном слое).
    pub fn set_edit_mask(&mut self, on: bool) {
        if on && self.doc.active_layer().mask.is_none() {
            self.notify("Сначала добавьте маску");
            return;
        }
        self.edit_mask = on && self.doc.active_layer().mask.is_some();
        self.notify(if self.edit_mask { "Рисование по маске" } else { "Рисование по слою" });
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

    /// Применяет фильтры к активному слою. Один вызов — один шаг истории.
    pub fn apply_filters(&mut self) {
        self.end_stroke();
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        let bright = self.f_bright;
        let contrast = self.f_contrast;
        let saturate = self.f_saturate;
        let hue = self.f_hue;
        let blur_r = self.f_blur;
        let sharp = self.f_sharpen;
        let busy = bright.abs() > 0.001
            || contrast.abs() > 0.001
            || saturate.abs() > 0.001
            || hue.abs() > 0.001
            || blur_r > 0.001
            || sharp > 0.001;
        if !busy {
            return;
        }
        let before = self.doc.active_layer().pixels.clone();
        let (w, h) = (self.doc.width, self.doc.height);
        let layer = self.doc.active_layer_mut();
        if blur_r > 0.0 {
            raster::blur(layer, w, h, blur_r);
        }
        if sharp > 0.0 {
            raster::sharpen(layer, w, h, sharp);
        }
        raster::adjust(layer, bright, contrast, saturate, hue);
        let after = self.doc.layers[self.doc.active].pixels.clone();
        if before != after {
            self.history.push("Фильтры слоя", Action::Pixels { layer: self.doc.active, before, after });
            self.doc.touch();
            self.dirty = true;
            self.notify("Фильтры применены к слою");
        }
        // Сбрасываем регуляторы, чтобы повторное нажатие не фильтровал дважды.
        self.f_bright = 0.0;
        self.f_contrast = 0.0;
        self.f_saturate = 0.0;
        self.f_hue = 0.0;
        self.f_blur = 0.0;
        self.f_sharpen = 0.0;
    }

    /// Открывает окно коррекции слоя.
    pub fn open_filters(&mut self) {
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        self.filter_dialog = true;
    }

    // --- свободная трансформация ---

    /// Начинает трансформацию: содержимое выделения поднимается в буфер,
    /// а сам слой каждый кадр перерисовывается по текущему положению рамки.
    pub fn begin_transform(&mut self) {
        self.end_stroke();
        let Some(s) = self.selection else {
            self.notify("Сначала выделите область");
            return;
        };
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        let (w, h) = (self.doc.width, self.doc.height);
        let x0 = s.x.max(0.0).floor() as usize;
        let y0 = s.y.max(0.0).floor() as usize;
        let x1 = ((s.x + s.w).ceil().max(1.0) as usize).min(w);
        let y1 = ((s.y + s.h).ceil().max(1.0) as usize).min(h);
        if x0 >= x1 || y0 >= y1 {
            self.notify("Выделение вне холста");
            return;
        }
        let data = raster::extract_rect(
            self.doc.active_layer(),
            w,
            h,
            SelRect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32),
        );
        self.transform = Some(Transform {
            data,
            before: self.doc.active_layer().pixels.clone(),
            // Исходный прямоугольник в углах 0 и 2 — по нему чистим слой.
            src: [x0 as f32, y0 as f32, x1 as f32, y1 as f32],
            pts: [
                (x0 as f32, y0 as f32),
                (x1 as f32, y0 as f32),
                (x1 as f32, y1 as f32),
                (x0 as f32, y1 as f32),
            ],
            grab: TransformGrab::None,
            anchor: (0.0, 0.0),
            start_angle: 0.0,
            start_w: (x1 - x0) as f32,
            start_h: (y1 - y0) as f32,
            rot_start: [(0.0, 0.0); 4],
            shown: [(x0 as f32, y0 as f32), (x1 as f32, y0 as f32), (x1 as f32, y1 as f32), (x0 as f32, y1 as f32)],
        });
        self.floating = None;
        self.sel_drag = SelDrag::None;
    }

    /// Перерисовывает слой по текущему положению рамки трансформации.
    /// Вызывается каждый кадр, пока трансформация активна.
    pub fn transform_preview(&mut self) {
        let Some(t) = self.transform.as_mut() else { return };
        if t.shown == t.pts {
            return;
        }
        t.shown = t.pts;
        let t = &*t;
        let (w, h) = (self.doc.width, self.doc.height);
        let mut pixels = t.before.clone();
        // Старое содержимое выделения убираем.
        let (x0, y0, x1, y1) = (
            t.src[0].max(0.0) as usize,
            t.src[1].max(0.0) as usize,
            (t.src[2].max(0.0) as usize).min(w),
            (t.src[3].max(0.0) as usize).min(h),
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * w + x) * 4;
                pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        let mut tmp = Layer::new(w, h, "трансформация");
        tmp.pixels = pixels;
        raster::blit_transformed(&mut tmp, w, h, &t.data, t.pts);
        self.doc.active_layer_mut().pixels = tmp.pixels;
        self.doc.touch();
    }

    /// Двигает трансформацию за угол, за рамку или вращает.
    pub fn transform_drag(&mut self, p: (f32, f32), grab: TransformGrab) {
        let Some(t) = self.transform.as_mut() else { return };
        match grab {
            TransformGrab::Corner(i) => {
                t.pts[i] = p;
                // Противоположный угол остаётся на месте — тянем «на глаз».
            }
            TransformGrab::Move => {
                let (dx, dy) = (p.0 - t.anchor.0, p.1 - t.anchor.1);
                t.pts = t.rot_start;
                for q in t.pts.iter_mut() {
                    q.0 += dx;
                    q.1 += dy;
                }
            }
            TransformGrab::Rotate => {
                let c = t.center();
                let a = (p.1 - c.1).atan2(p.0 - c.0).to_degrees() - t.start_angle;
                let rad = a * std::f32::consts::PI / 180.0;
                let (s, co) = rad.sin_cos();
                for i in 0..4 {
                    let (x, y) = (t.rot_start[i].0 - c.0, t.rot_start[i].1 - c.1);
                    t.pts[i] = (c.0 + x * co - y * s, c.1 + x * s + y * co);
                }
            }
            TransformGrab::None => {}
        }
    }

    /// Запоминает опорную точку и угол при начале перетаскивания.
    pub fn transform_grab_begin(&mut self, p: (f32, f32), grab: TransformGrab) {
        let Some(t) = self.transform.as_mut() else { return };
        t.grab = grab;
        t.anchor = p;
        t.rot_start = t.pts;
        let c = t.center();
        t.start_angle = (p.1 - c.1).atan2(p.0 - c.0).to_degrees();
    }

    /// Завершает перетаскивание (ручка остаётся активной до Enter).
    pub fn transform_grab_end(&mut self) {
        if let Some(t) = self.transform.as_mut() {
            t.grab = TransformGrab::None;
        }
    }

    /// Вписывает трансформацию в исходный прямоугольник (кнопка «Вписать»).
    pub fn transform_fit(&mut self) {
        let Some(t) = self.transform.as_mut() else { return };
        let c = t.center();
        let (w, h) = (t.start_w, t.start_h);
        t.pts = [(c.0, c.1), (c.0 + w, c.1), (c.0 + w, c.1 + h), (c.0, c.1 + h)];
    }

    /// Применяет трансформацию к слою (Enter).
    pub fn transform_commit(&mut self) {
        let Some(t) = self.transform.take() else { return };
        // Кадр уже нарисован transform_preview, значит слой содержит результат.
        let after = self.doc.active_layer().pixels.clone();
        if t.before != after {
            self.history.push(
                "Трансформация",
                Action::Pixels { layer: self.doc.active, before: t.before.clone(), after },
            );
            self.doc.touch();
            self.dirty = true;
        }
        // Рамка переезжает вместе с содержимым.
        let c = t.center();
        let w2 = (((t.pts[1].0 - t.pts[0].0).powi(2) + (t.pts[1].1 - t.pts[0].1).powi(2)).sqrt()) as f32;
        let h2 = (((t.pts[3].0 - t.pts[0].0).powi(2) + (t.pts[3].1 - t.pts[0].1).powi(2)).sqrt()) as f32;
        self.selection = Some(SelRect { x: c.0 - w2 / 2.0, y: c.1 - h2 / 2.0, w: w2, h: h2 });
        self.notify("Трансформация применена");
    }

    /// Отменяет трансформацию (Esc) и возвращает слой к исходному виду.
    pub fn transform_cancel(&mut self) {
        if let Some(t) = self.transform.take() {
            self.doc.active_layer_mut().pixels = t.before;
            self.doc.touch();
            self.notify("Трансформация отменена");
        }
    }

    /// Что схватили под курсором: угол рамки, перенос или поворот.
    /// Точка сравнивается в экранных координатах.
    pub fn transform_grab_at(&self, screen: (f32, f32)) -> TransformGrab {
        let Some(t) = self.transform.as_ref() else { return TransformGrab::None };
        const HANDLE: f32 = 7.0;
        for (i, p) in t.pts.iter().enumerate() {
            let s = self.canvas_to_screen(p.0, p.1);
            if (s.0 - screen.0).abs() <= HANDLE && (s.1 - screen.1).abs() <= HANDLE {
                return TransformGrab::Corner(i);
            }
        }
        let sp: [(f32, f32); 4] = [
            self.canvas_to_screen(t.pts[0].0, t.pts[0].1),
            self.canvas_to_screen(t.pts[1].0, t.pts[1].1),
            self.canvas_to_screen(t.pts[2].0, t.pts[2].1),
            self.canvas_to_screen(t.pts[3].0, t.pts[3].1),
        ];
        let top = ((sp[0].0 + sp[1].0) / 2.0, (sp[0].1 + sp[1].1) / 2.0);
        if (top.0 - screen.0).abs() <= HANDLE && (top.1 - 26.0 - screen.1).abs() <= HANDLE {
            return TransformGrab::Rotate;
        }
        // Внутри рамки — перенос всего фрагмента.
        let (mut x0, mut y0, mut x1, mut y1) = (1e9f32, 1e9f32, -1e9f32, -1e9f32);
        for p in sp.iter() {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        if screen.0 >= x0 && screen.0 <= x1 && screen.1 >= y0 && screen.1 <= y1 {
            return TransformGrab::Move;
        }
        TransformGrab::None
    }

    /// Поворачивает или отражает весь холст целиком (как «Вид» в CSP).    /// Отмена возвращает прежнее состояние целиком, поэтому история
    /// перед таким шагом укорачивается — снимки документа тяжёлые.
    pub fn transform_canvas(&mut self, action: CanvasOp) {
        self.end_stroke();
        let before = self.doc.clone();
        match action {
            CanvasOp::Rotate90 => self.doc.rotate(1),
            CanvasOp::Rotate270 => self.doc.rotate(3),
            CanvasOp::Rotate180 => self.doc.rotate(2),
            CanvasOp::MirrorH => self.doc.mirror(true),
            CanvasOp::MirrorV => self.doc.mirror(false),
        }
        let after = self.doc.clone();
        // Тяжёлые снимки: оставляем пару прошлых шагов, чтобы не съесть память.
        while self.history.steps.len() > 2 {
            self.history.steps.remove(0);
        }
        self.history.push(
            match action {
                CanvasOp::Rotate90 => "Поворот холста 90°",
                CanvasOp::Rotate180 => "Поворот холста 180°",
                CanvasOp::Rotate270 => "Поворот холста 270°",
                CanvasOp::MirrorH => "Холст слева направо",
                CanvasOp::MirrorV => "Холст сверху вниз",
            },
            Action::Document { before: Box::new(before), after: Box::new(after) },
        );
        self.selection = None;
        self.floating = None;
        self.dirty = true;
        self.fit_pending = true;
        self.notify("Холст изменён");
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

    /// Атлас масок слоёв в сером: белое — слой виден, чёрное — закрыт.
    pub fn build_mask_atlas(&self) -> Vec<u8> {
        use crate::renderer::{THUMB_ATLAS, THUMB_CELL, THUMB_COLS};
        let mut atlas = vec![0u8; THUMB_ATLAS * THUMB_ATLAS * 4];
        for (idx, layer) in self.doc.layers.iter().enumerate() {
            let Some(mask) = layer.mask.as_ref() else { continue };
            let col = (idx % THUMB_COLS) * THUMB_CELL;
            let row = (idx / THUMB_COLS) * THUMB_CELL;
            let sw = self.doc.width.max(1) as f32;
            let sh = self.doc.height.max(1) as f32;
            for ty in 0..THUMB_CELL {
                let y0 = (ty as f32 / THUMB_CELL as f32) * sh;
                let y1 = ((ty + 1) as f32 / THUMB_CELL as f32) * sh;
                for tx in 0..THUMB_CELL {
                    let x0 = (tx as f32 / THUMB_CELL as f32) * sw;
                    let x1 = ((tx + 1) as f32 / THUMB_CELL as f32) * sw;
                    let sx0 = x0.floor() as usize;
                    let sy0 = y0.floor() as usize;
                    let sx1 = (x1.ceil() as usize).min(self.doc.width);
                    let sy1 = (y1.ceil() as usize).min(self.doc.height);
                    let mut sum = 0u32;
                    let mut n = 0u32;
                    for sy in sy0..sy1.max(sy0 + 1) {
                        for sx in sx0..sx1.max(sx0 + 1) {
                            sum += mask[(sy * self.doc.width + sx).min(mask.len() - 1)] as u32;
                            n += 1;
                        }
                    }
                    if n == 0 {
                        continue;
                    }
                    let v = (sum / n) as u8;
                    let d = ((row + ty) * THUMB_ATLAS + col + tx) * 4;
                    atlas[d] = v;
                    atlas[d + 1] = v;
                    atlas[d + 2] = v;
                    atlas[d + 3] = 255;
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

    /// Покрытие выделения в точке холста: 255 — выделено, 0 — нет.
    /// Без маски действует прямоугольник.
    #[inline]
    pub fn sel_cover(&self, x: usize, y: usize) -> u8 {
        match &self.sel_mask {
            None => {
                let Some(r) = self.selection else { return 0 };
                if x < r.x.max(0.0) as usize
                    || y < r.y.max(0.0) as usize
                    || x >= (r.x + r.w).max(0.0).ceil() as usize
                    || y >= (r.y + r.h).max(0.0).ceil() as usize
                {
                    0
                } else {
                    255
                }
            }
            Some(m) => {
                if x >= self.doc.width || y >= self.doc.height {
                    return 0;
                }
                m.get(y * self.doc.width + x).copied().unwrap_or(0)
            }
        }
    }

    /// Есть ли маска выделения (форма сложнее прямоугольника).
    pub fn has_sel_mask(&self) -> bool {
        self.sel_mask.is_some() && self.selection.is_some()
    }

    /// Ставит маску выделения и пересчитывает её границы.
    pub fn set_sel_mask(&mut self, mask: Vec<u8>) {
        let (w, h) = (self.doc.width, self.doc.height);
        let rect = raster::mask_bounds(&mask, w, h);
        match rect {
            Some(r) => {
                self.selection = Some(r);
                self.sel_mask = Some(mask);
            }
            None => {
                // Выделение пустое — снимаем целиком.
                self.selection = None;
                self.sel_mask = None;
            }
        }
        self.sel_mask_rev = self.sel_mask_rev.wrapping_add(1);
    }

    /// Снимает выделение вместе с маской.
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.sel_mask = None;
        self.sel_mask_rev = self.sel_mask_rev.wrapping_add(1);
    }

    /// Инвертирует выделение: выделенное становится невыделенным и наоборот.
    pub fn invert_selection(&mut self) {
        self.end_stroke();
        if self.selection.is_none() {
            self.notify("Сначала что-нибудь выделите");
            return;
        }
        let (w, h) = (self.doc.width, self.doc.height);
        // Без маски выделение — это прямоугольник: разворачиваем именно его.
        let mut mask = self.sel_mask.clone().unwrap_or_else(|| self.rect_mask(w, h));
        for v in mask.iter_mut() {
            *v = 255 - *v;
        }
        self.set_sel_mask(mask);
        self.notify(if self.selection.is_some() { "Выделение инвертировано" } else { "Выделение пустое" });
    }

    /// Растушёвывает выделение на `px` пикселей: край становится мягким.
    pub fn feather_selection(&mut self, px: f32) {
        self.end_stroke();
        if px <= 0.0 {
            return;
        }
        let (w, h) = (self.doc.width, self.doc.height);
        let mut mask = self.sel_mask.clone().unwrap_or_else(|| self.rect_mask(w, h));
        raster::mask_blur(&mut mask, w, h, px);
        self.set_sel_mask(mask);
        self.notify(&format!("Растушёвка: {}", px.round() as i32));
    }

    /// Расширяет (плюс) или сужает (минус) выделение на `px` пикселей.
    pub fn grow_selection(&mut self, px: f32) {
        self.end_stroke();
        if px == 0.0 {
            return;
        }
        let (w, h) = (self.doc.width, self.doc.height);
        let mut mask = self.sel_mask.clone().unwrap_or_else(|| self.rect_mask(w, h));
        raster::mask_grow(&mut mask, w, h, px);
        self.set_sel_mask(mask);
        self.notify(if px > 0.0 { "Выделение расширено" } else { "Выделение сужено" });
    }

    /// Маска текущего выделения: прямоугольник — как есть, иначе уже готовая.
    fn rect_mask(&self, w: usize, h: usize) -> Vec<u8> {
        let mut m = vec![0u8; w * h];
        let Some(r) = self.selection else { return m };
        let x0 = r.x.max(0.0).floor() as usize;
        let y0 = r.y.max(0.0).floor() as usize;
        let x1 = ((r.x + r.w).ceil().max(0.0) as usize).min(w);
        let y1 = ((r.y + r.h).ceil().max(0.0) as usize).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                m[y * w + x] = 255;
            }
        }
        m
    }

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
        if self.selection.is_none() {
            return;
        }
        self.end_stroke();
        if self.doc.active_layer().meta.locked {
            self.notify("Слой заблокирован");
            return;
        }
        let before = self.doc.active_layer().pixels.clone();
        let (w, h) = (self.doc.width, self.doc.height);
        let layer = self.doc.active_layer_mut();
        // С формой выделения удаляем по покрытию, иначе — по прямоугольнику.
        match &self.sel_mask {
            Some(m) if self.selection.is_some() => {
                let m = m.clone();
                raster::clear_rect_mask(layer, w, h, &m);
            }
            _ => {
                if let Some(s) = self.selection {
                    raster::clear_rect(layer, w, h, s);
                }
            }
        }
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

    #[test]
    fn transform_moves_selection_and_keeps_history() {
        let mut app = app_with_blank(32, 32);
        app.doc.active_layer_mut().set(32, 8, 8, [255, 0, 0, 255]);
        app.selection = Some(SelRect { x: 6.0, y: 6.0, w: 6.0, h: 6.0 });
        app.begin_transform();
        assert!(app.transform.is_some(), "трансформация не началась");
        // Сдвигаем рамку на 10 пикселей вправо и вниз.
        let t = app.transform.as_mut().unwrap();
        for p in t.pts.iter_mut() {
            p.0 += 10.0;
            p.1 += 10.0;
        }
        app.transform_preview();
        let l = &app.doc.layers[app.doc.active];
        assert_eq!(l.get(32, 8, 8)[3], 0, "на старом месте должно быть пусто");
        assert_eq!(l.get(32, 18, 18), [255, 0, 0, 255], "пиксель переехал вместе с рамкой");
        app.transform_commit();
        assert!(app.transform.is_none());
        let after = app.doc.layers[app.doc.active].pixels.clone();
        app.undo();
        assert_ne!(app.doc.layers[app.doc.active].pixels, after, "отмена вернула картинку на место");
        app.redo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, after, "повтор не вернул сдвиг");
    }

    #[test]
    fn transform_cancel_restores_layer_exactly() {
        let mut app = app_with_blank(32, 32);
        app.doc.active_layer_mut().fill([7, 7, 7, 255]);
        let before = app.doc.layers[0].pixels.clone();
        app.selection = Some(SelRect { x: 4.0, y: 4.0, w: 8.0, h: 8.0 });
        app.begin_transform();
        let t = app.transform.as_mut().unwrap();
        t.pts = [(20.0, 20.0), (28.0, 20.0), (28.0, 28.0), (20.0, 28.0)];
        app.transform_preview();
        assert_ne!(app.doc.layers[0].pixels, before, "предпросмотр должен был что-то изменить");
        app.transform_cancel();
        assert_eq!(app.doc.layers[0].pixels, before, "отмена обязана вернуть слой целиком");
    }

    #[test]
    fn transform_rotates_around_center() {
        let mut app = app_with_blank(40, 40);
        app.doc.active_layer_mut().fill([255, 255, 255, 255]);
        app.selection = Some(SelRect { x: 10.0, y: 10.0, w: 10.0, h: 10.0 });
        app.begin_transform();
        let c = app.transform.as_ref().unwrap().center();
        // Захват в 30 px правее центра и поворот на 90°.
        app.transform_grab_begin((c.0 + 30.0, c.1), TransformGrab::Rotate);
        app.transform_drag((c.0, c.1 + 30.0), TransformGrab::Rotate);
        let t = app.transform.as_ref().unwrap();
        // Углы должны лечь на крест вокруг центра.
        for p in t.pts.iter() {
            let dx = (p.0 - c.0).abs();
            let dy = (p.1 - c.1).abs();
            assert!((dx - dy).abs() < 0.01, "угол {:?} не на кресте", p);
        }
        app.transform_cancel();
    }

    #[test]
    fn transform_needs_selection() {
        let mut app = app_with_blank(16, 16);
        app.selection = None;
        app.begin_transform();
        assert!(app.transform.is_none(), "без выделения трансформация не начинается");
    }

    #[test]
    fn grouping_layers_creates_folder_over_them() {
        let mut app = app_with_blank(20, 20);
        app.add_layer();
        app.add_layer();
        assert_eq!(app.doc.layers.len(), 3);
        // Активен слой 1: он и слой над ним (2) становятся папкой «Группа 1».
        app.doc.active = 1;
        app.make_group();
        assert_eq!(app.doc.layers[2].meta.group.as_deref(), Some("Группа 1"), "слой 2 в папке");
        assert_eq!(app.doc.layers[1].meta.group.as_deref(), Some("Группа 1"), "слой 1 в папке");
        assert_eq!(app.doc.layers[0].meta.group, None, "нижний слой вне папки");
        assert!(app.is_group_header(2), "слой 2 — шапка папки");
        assert!(!app.is_group_header(1), "слой 1 — содержимое папки");
    }

    #[test]
    fn collapsed_group_hides_children_in_panel_only() {
        let mut app = app_with_blank(20, 20);
        app.add_layer();
        app.add_layer();
        // Слой 2 — шапка, слой 1 — её содержимое.
        app.doc.active = 1;
        app.make_group();
        assert!(app.layer_visible_in_panel(1), "открытая папка показывает содержимое");
        app.toggle_group(2);
        assert!(!app.layer_visible_in_panel(1), "сворачивание прячет содержимое");
        assert!(app.layer_visible_in_panel(2), "шапка папки остаётся видна");
        app.toggle_group(2);
        assert!(app.layer_visible_in_panel(1), "разворачивание возвращает");
        // Пиксели при этом на месте — слой всего лишь не нарисован в панели.
        assert_eq!(app.doc.layers[1].pixels.len(), 20 * 20 * 4);
    }

    #[test]
    fn ungroup_removes_folder_membership() {
        let mut app = app_with_blank(20, 20);
        app.add_layer();
        app.doc.active = 1;
        app.make_group();
        app.ungroup();
        assert!(app.doc.layers.iter().all(|l| l.meta.group.is_none()), "папка распущена");
    }
    #[test]
    fn group_visibility_toggles_all_members() {
        let mut app = app_with_blank(20, 20);
        app.add_layer();
        app.add_layer();
        app.doc.active = 1;
        app.make_group(); // шапка 2, содержимое 1
        let members = app.group_members(2);
        assert_eq!(members, vec![2, 1], "в папке шапка и слой под ней");
        app.toggle_group_visibility(2);
        for i in &members {
            assert!(!app.doc.layers[*i].meta.visible, "слой {} скрыт вместе с папкой", i);
        }
        app.toggle_group_visibility(2);
        for i in &members {
            assert!(app.doc.layers[*i].meta.visible, "слой {} снова виден", i);
        }
    }

    #[test]
    fn mask_from_selection_shows_only_selected_part() {
        let mut app = app_with_blank(32, 32);
        app.doc.active_layer_mut().fill([255, 0, 0, 255]);
        app.selection = Some(SelRect { x: 8.0, y: 8.0, w: 8.0, h: 8.0 });
        app.add_mask_from_selection();
        app.doc.ensure_composite();
        let c = &app.doc.composite;
        let at = |x: usize, y: usize| c[(y * 32 + x) * 4 + 3];
        assert_eq!(at(12, 12), 255, "внутри выделения слой виден");
        assert_eq!(at(2, 2), 0, "вне выделения маска закрывает");
        assert!(app.edit_mask, "после создания маски включается её правка");
    }

    #[test]
    fn brush_paints_mask_not_pixels() {
        let mut app = app_with_blank(32, 32);
        app.doc.active_layer_mut().fill([10, 10, 10, 255]);
        // Маска открыта только слева — кистью открываем правую половину.
        app.selection = Some(SelRect { x: 0.0, y: 0.0, w: 12.0, h: 32.0 });
        app.add_mask_from_selection();
        app.tool = Tool::Brush;
        app.params.size = 10.0;
        let pixels_before = app.doc.layers[0].pixels.clone();
        app.begin_stroke((24.0, 16.0), false);
        app.move_stroke((28.0, 16.0), false);
        app.end_stroke();
        assert_eq!(app.doc.layers[0].pixels, pixels_before, "пиксели слоя не тронуты");
        let mask = app.doc.layers[0].mask.as_ref().expect("маска на месте");
        assert_eq!(mask[16 * 32 + 24], 255, "кисть открыла маску в мазке");
        // Отмена возвращает маску до кисти: справа снова закрыто.
        app.undo();
        let mask = app.doc.layers[0].mask.as_ref().expect("маска осталась после отмены кисти");
        assert_eq!(mask[16 * 32 + 24], 0, "отмена вернула закрытую маску");
        assert_eq!(mask[16 * 32 + 4], 255, "левая половина осталась открытой");
    }

    #[test]
    fn eraser_closes_mask_and_undo_restores() {
        let mut app = app_with_blank(32, 32);
        app.doc.active_layer_mut().fill([255, 255, 255, 255]);
        app.selection = Some(SelRect { x: 0.0, y: 0.0, w: 32.0, h: 32.0 });
        app.add_mask_from_selection();
        app.tool = Tool::Eraser;
        app.params.size = 12.0;
        app.begin_stroke((16.0, 16.0), false);
        app.move_stroke((16.0, 20.0), false);
        app.end_stroke();
        let mask = app.doc.layers[0].mask.clone().unwrap();
        assert_eq!(mask[16 * 32 + 16], 0, "ластик закрыл маску");
        app.undo();
        let mask = app.doc.layers[0].mask.as_ref().unwrap();
        assert_eq!(mask[16 * 32 + 16], 255, "отмена снова открыла");
    }

    #[test]
    fn mask_invert_flip_and_delete_with_history() {
        let mut app = app_with_blank(16, 16);
        app.selection = Some(SelRect { x: 0.0, y: 0.0, w: 8.0, h: 16.0 });
        app.add_mask_from_selection();
        let m = app.doc.layers[0].mask.clone().unwrap();
        assert_eq!(m[0], 255, "левая половина открыта");
        assert_eq!(m[15], 0, "правая закрыта");
        app.invert_mask();
        let m = app.doc.layers[0].mask.clone().unwrap();
        assert_eq!(m[0], 0);
        assert_eq!(m[15], 255);
        app.delete_mask();
        assert!(app.doc.layers[0].mask.is_none(), "маска удалена");
        app.undo();
        assert!(app.doc.layers[0].mask.is_some(), "отмена вернула маску");
    }

    #[test]
    fn mask_survives_project_round_trip() {
        let mut app = app_with_blank(24, 24);
        app.doc.active_layer_mut().set(24, 4, 4, [9, 8, 7, 255]);
        app.selection = Some(SelRect { x: 0.0, y: 0.0, w: 12.0, h: 24.0 });
        app.add_mask_from_selection();
        let expected = app.doc.layers[0].mask.clone().unwrap();
        let path = temp_file("mask.tpaint");
        app.save_project(&path).expect("сохранили");
        let mut back = App::new();
        back.open_project(&path).expect("прочитали");
        let _ = std::fs::remove_file(&path);
        let got = back.doc.layers[0].mask.as_ref().expect("маска должна сохраниться");
        assert_eq!(got, &expected, "маска сохранилась без потерь");
        assert_eq!(back.doc.layers[0].get(24, 4, 4), [9, 8, 7, 255]);
    }

    #[test]
    fn project_keeps_layer_folders() {
        let mut app = app_with_blank(16, 16);
        app.add_layer();
        app.doc.active = 0;
        app.make_group(); // слои 0 и 1 в папке, шапка — слой 1
        app.toggle_group(1);
        let path = temp_file("groups.tpaint");
        app.save_project(&path).expect("сохранили");
        let mut back = App::new();
        back.open_project(&path).expect("прочитали");
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.doc.layers[1].meta.group.as_deref(), Some("Группа 1"));
        assert!(!back.doc.layers[1].meta.group_open, "папка осталась свёрнутой");
        assert!(!back.layer_visible_in_panel(0), "содержимое осталось скрытым в панели");
    }

    #[test]
    fn stabilizer_smooths_jitter_but_reaches_cursor() {
        let mut s = Stabilizer::new();
        s.reset((0.0, 0.0));
        // Рука дрожит: точки скачут вокруг прямой линии.
        let raw = [
            (10.0, 0.0),
            (20.0, 4.0),
            (30.0, -3.0),
            (40.0, 3.0),
            (50.0, 0.0),
            (60.0, 0.0),
        ];
        for p in raw {
            s.push(p, 0.6);
        }
        // В конце мазок обязан дойти до последней точки — иначе кисть отстаёт.
        assert!((s.pos.0 - 60.0).abs() < 1.5, "стабилизатор не дошёл до курсора: {:?}", s.pos);
        assert!(s.pos.1.abs() < 2.0, "дрожание не сглажено: {:?}", s.pos);
    }

    #[test]
    fn stabilizer_without_smoothing_follows_cursor_exactly() {
        let mut s = Stabilizer::new();
        s.reset((0.0, 0.0));
        s.push((5.0, 7.0), 0.0);
        s.push((11.0, 3.0), 0.0);
        assert!((s.pos.0 - 11.0).abs() < 0.001 && (s.pos.1 - 3.0).abs() < 0.001, "без сглаживания точно за курсором: {:?}", s.pos);
    }

    #[test]
    fn brush_stroke_has_no_gaps_at_high_speed() {
        let mut app = app_with_blank(64, 64);
        app.tool = Tool::Brush;
        app.params.size = 16.0;
        app.params.spacing = 0.05;
        app.params.smoothing = 0.5;
        app.begin_stroke((4.0, 32.0), false);
        // Один огромный шаг: без постоянного шага между dab'ами остался бы провал.
        app.move_stroke((60.0, 32.0), false);
        app.end_stroke();
        let painted = |x: usize| app.doc.layers[0].get(64, x, 32)[3];
        for x in 6..58 {
            assert!(painted(x) > 0, "пробел в мазке на x={}", x);
        }
    }

    #[test]
    fn spacing_controls_stroke_density() {
        // Мелкий шаг красит больше пикселей, чем крупный, при том же размере.
        let strokes = |spacing: f32| {
            let mut app = app_with_blank(64, 64);
            app.tool = Tool::Brush;
            app.params.size = 20.0;
            app.params.spacing = spacing;
            app.begin_stroke((10.0, 32.0), false);
            app.move_stroke((50.0, 32.0), false);
            app.end_stroke();
            app.doc.layers[0].pixels.chunks(4).filter(|p| p[3] > 0).count()
        };
        assert!(strokes(0.02) >= strokes(0.5), "мелкий шаг не может красить меньше");
    }

    #[test]
    fn ellipse_select_makes_round_mask() {
        let mut app = app_with_blank(40, 40);
        app.tool = Tool::EllipseSelect;
        app.begin_select((8.0, 8.0));
        app.move_select((32.0, 32.0));
        app.end_select();
        assert!(app.has_sel_mask(), "эллипс должен задать маску");
        let c = app.sel_cover(20, 20);
        assert!(c > 200, "центр эллипса выделен: {}", c);
        assert_eq!(app.sel_cover(9, 9), 0, "угол рамки вне эллипса");
        let rect = app.selection.unwrap();
        assert!(rect.w > 20.0, "рамка покрывает весь эллипс: {:?}", rect);
    }

    #[test]
    fn magic_wand_picks_one_region() {
        let mut app = app_with_blank(40, 40);
        // Левая половина красная, правая — синяя.
        let (w, h) = (40usize, 40usize);
        for y in 0..h {
            for x in 0..20 {
                app.doc.active_layer_mut().set(w, x, y, [255, 0, 0, 255]);
            }
        }
        app.tool = Tool::Wand;
        app.params.tolerance = 8.0;
        app.begin_stroke((10.0, 10.0), false);
        assert!(app.has_sel_mask(), "палочка должна дать маску");
        assert!(app.sel_cover(5, 5) > 200, "красная область выделена");
        assert_eq!(app.sel_cover(30, 10), 0, "синяя область не выделена");
    }

    #[test]
    fn invert_flips_selection_area() {
        let mut app = app_with_blank(20, 20);
        app.selection = Some(SelRect { x: 0.0, y: 0.0, w: 10.0, h: 20.0 });
        app.invert_selection();
        // Теперь выделено всё, кроме левой половины.
        assert!(app.sel_cover(5, 5) == 0, "левая половина больше не выделена");
        assert!(app.sel_cover(15, 5) > 200, "правая половина выделена");
    }

    #[test]
    fn feather_softens_selection_edge() {
        let mut app = app_with_blank(40, 40);
        app.selection = Some(SelRect { x: 10.0, y: 10.0, w: 20.0, h: 20.0 });
        app.feather_selection(3.0);
        // Внутри всё по-прежнему выделено, у края покрытие частичное.
        assert!(app.sel_cover(20, 20) > 200, "центр выделен");
        assert!(app.sel_cover(9, 20) > 0 && app.sel_cover(9, 20) < 255, "край мягкий: {}", app.sel_cover(9, 20));
    }

    #[test]
    fn grow_and_shrink_change_area_size() {
        let mut app = app_with_blank(40, 40);
        app.selection = Some(SelRect { x: 10.0, y: 10.0, w: 20.0, h: 20.0 });
        app.grow_selection(3.0);
        let big = app.selection.unwrap();
        assert_eq!((big.x, big.w), (7.0, 26.0), "расширилось на 3 пикселя: {:?}", big);
        app.grow_selection(-3.0);
        let back = app.selection.unwrap();
        assert_eq!((back.x, back.w), (10.0, 20.0), "сужение вернуло исходный размер: {:?}", back);
    }

    #[test]
    fn clipped_layer_is_cut_by_layer_below() {
        let mut app = app_with_blank(20, 20);
        // Нижний слой: только левая половина.
        for y in 0..20 {
            for x in 0..10 {
                app.doc.active_layer_mut().set(20, x, y, [0, 0, 255, 255]);
            }
        }
        app.add_layer();
        // Верхний красен везде, но прижат — значит виден только слева.
        app.doc.active_layer_mut().fill([255, 0, 0, 255]);
        app.doc.active_layer_mut().meta.clipped = true;
        app.doc.touch();
        app.doc.ensure_composite();
        let c = &app.doc.composite;
        assert_eq!(c[3], 255, "слева красный поверх синего");
        let right = 15 * 20 * 4 + 3;
        assert_eq!(c[right], 255, "справа виден нижний синий слой");
        let mid = 15 * 20 * 4;
        assert_eq!(&c[mid..mid + 3], &[255, 0, 0], "справа синий, а не красный");
    }

    #[test]
    fn unclipped_layer_covers_everything() {
        let mut app = app_with_blank(20, 20);
        for y in 0..20 {
            for x in 0..10 {
                app.doc.active_layer_mut().set(20, x, y, [0, 0, 255, 255]);
            }
        }
        app.add_layer();
        app.doc.active_layer_mut().fill([255, 0, 0, 255]);
        app.doc.touch();
        app.doc.ensure_composite();
        let mid = 15 * 20 * 4;
        assert_eq!(&app.doc.composite[mid..mid + 3], &[255, 0, 0], "свободный слой виден целиком");
    }

    #[test]
    fn delete_selection_respects_ellipse_shape() {
        let mut app = app_with_blank(40, 40);
        app.doc.active_layer_mut().fill([10, 10, 10, 255]);
        app.tool = Tool::EllipseSelect;
        app.begin_select((8.0, 8.0));
        app.move_select((32.0, 32.0));
        app.end_select();
        app.delete_selection();
        let center = app.doc.layers[0].get(40, 20, 20);
        let corner = app.doc.layers[0].get(40, 9, 9);
        assert_eq!(center[3], 0, "центр эллипса очищен");
        assert_eq!(corner[3], 255, "угол вне эллипса не тронут");
    }

    #[test]
    fn project_keeps_clipped_flag() {
        let mut app = app_with_blank(16, 16);
        app.add_layer();
        app.toggle_clip();
        let path = temp_file("clip.tpaint");
        app.save_project(&path).expect("сохранили");
        let mut back = App::new();
        back.open_project(&path).expect("прочитали");
        let _ = std::fs::remove_file(&path);
        assert!(back.doc.layers[1].meta.clipped, "слой остался прижатым");
    }

    #[test]
    fn shadow_appears_under_layer_without_touching_pixels() {
        let mut app = app_with_blank(40, 40);
        for y in 10..18 {
            for x in 10..18 {
                app.doc.active_layer_mut().set(40, x, y, [255, 0, 0, 255]);
            }
        }
        let pixels_before = app.doc.layers[0].pixels.clone();
        app.toggle_fx_shadow();
        assert!(app.doc.layers[0].meta.fx.shadow, "тень включена");
        app.set_layer_meta(0, |m| {
            m.fx.shadow_blur = 0.0;
            m.fx.shadow_dx = 3.0;
            m.fx.shadow_dy = 3.0;
        });
        app.doc.touch();
        app.doc.ensure_composite();
        assert_eq!(app.doc.layers[0].pixels, pixels_before, "эффект неразрушающий");
        let c = &app.doc.composite;
        let at = |x: usize, y: usize| c[(y * 40 + x) * 4 + 3];
        assert!(at(20, 20) > 0, "тень справа снизу от квадрата");
        assert_eq!(at(14, 14), 255, "сам слой остался на месте");
    }

    #[test]
    fn outline_draws_ring_around_layer() {
        let mut app = app_with_blank(40, 40);
        for y in 15..25 {
            for x in 15..25 {
                app.doc.active_layer_mut().set(40, x, y, [0, 128, 255, 255]);
            }
        }
        app.toggle_fx_outline();
        app.set_layer_meta(0, |m| m.fx.outline_size = 2.0);
        app.doc.touch();
        app.doc.ensure_composite();
        let c = &app.doc.composite;
        let o = (13 * 40 + 20) * 4;
        assert!(c[o + 3] > 0, "обводка нарисована");
        let inside = (20 * 40 + 20) * 4;
        assert_eq!(&c[inside..inside + 3], &[0, 128, 255], "внутри сам слой");
    }

    #[test]
    fn layer_tag_cycles_through_colors() {
        let mut app = app_with_blank(16, 16);
        assert_eq!(app.doc.layers[0].meta.tag, 0);
        app.cycle_tag();
        assert_eq!(app.doc.layers[0].meta.tag, 1);
        for _ in 0..5 {
            app.cycle_tag();
        }
        assert_eq!(app.doc.layers[0].meta.tag, 6);
        app.cycle_tag();
        assert_eq!(app.doc.layers[0].meta.tag, 0, "метка сбрасывается в ноль");
    }

    #[test]
    fn rename_layer_applies_or_cancels() {
        let mut app = app_with_blank(16, 16);
        app.begin_rename();
        assert_eq!(app.rename, Some(0));
        app.rename_buf = "Небо".to_string();
        app.finish_rename(true);
        assert_eq!(app.doc.layers[0].meta.name, "Небо");
        app.begin_rename();
        app.rename_buf = "tmp".to_string();
        app.finish_rename(false);
        assert_eq!(app.doc.layers[0].meta.name, "Небо", "отмена не меняет имя");
    }

    #[test]
    fn project_keeps_layer_effects_and_tag() {
        let mut app = app_with_blank(24, 24);
        app.toggle_fx_shadow();
        app.set_layer_meta(0, |m| {
            m.fx.shadow_dx = 7.0;
            m.fx.shadow_color = [10, 20, 30, 255];
            m.fx.outline = true;
            m.fx.outline_size = 3.0;
            m.tag = 4;
        });
        let path = temp_file("fx.tpaint");
        app.save_project(&path).expect("сохранили");
        let mut back = App::new();
        back.open_project(&path).expect("прочитали");
        let _ = std::fs::remove_file(&path);
        let m = &back.doc.layers[0].meta;
        assert!(m.fx.shadow && m.fx.outline, "эффекты сохранились");
        assert_eq!(m.fx.shadow_dx, 7.0);
        assert_eq!(m.fx.shadow_color, [10, 20, 30, 255]);
        assert_eq!(m.fx.outline_size, 3.0);
        assert_eq!(m.tag, 4, "метка сохранилась");
    }

    #[test]
    fn polygon_click_builds_selection_and_enter_closes_it() {
        let mut app = app_with_blank(40, 40);
        app.doc.active_layer_mut().fill([20, 20, 20, 255]);
        app.tool = Tool::Polygon;
        app.poly_click((5.0, 5.0));
        app.poly_click((30.0, 5.0));
        app.poly_click((30.0, 30.0));
        assert!(app.poly_active, "контур набирается");
        assert_eq!(app.poly.len(), 3);
        app.finish_polygon();
        assert!(!app.poly_active && app.poly.is_empty(), "контур замкнут и убран");
        assert!(app.has_sel_mask(), "форма выделения задана маской");
        let s = app.selection.expect("есть границы выделения");
        assert_eq!((s.x, s.y), (5.0, 5.0));
        assert_eq!((s.w, s.h), (25.0, 25.0));
        app.delete_selection();
        let layer = &app.doc.layers[0];
        assert_eq!(layer.get(40, 12, 8)[3], 0, "внутри очищено");
        assert_eq!(layer.get(40, 35, 35)[3], 255, "вне контура осталось");
    }

    #[test]
    fn polygon_click_on_first_point_closes_shape() {
        let mut app = app_with_blank(30, 30);
        app.tool = Tool::Polygon;
        app.poly_click((5.0, 5.0));
        app.poly_click((20.0, 8.0));
        app.poly_click((14.0, 24.0));
        app.poly_click((6.0, 5.0)); // рядом с первой вершиной
        assert!(!app.poly_active, "контур замкнулся кликом по первой точке");
        assert!(app.has_sel_mask());
    }

    #[test]
    fn cancel_polygon_throws_away_draft() {
        let mut app = app_with_blank(20, 20);
        app.tool = Tool::Polygon;
        app.poly_click((2.0, 2.0));
        app.poly_click((10.0, 2.0));
        app.cancel_polygon();
        assert!(app.poly.is_empty() && !app.poly_active);
        assert!(app.selection.is_none(), "выделения не появилось");
        // Слишком короткий контур не даёт выделения.
        app.poly_click((2.0, 2.0));
        app.finish_polygon();
        assert!(app.selection.is_none());
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
            "Привет, Tpaint!",
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
        // Слой с маской: видна только часть «свечения».
        app.add_layer();
        app.doc.layers[4].meta.name = "Свечение (маска)".to_string();
        app.tool = Tool::Brush;
        app.params.size = 60.0;
        app.params.hardness = 0.4;
        app.primary = [255, 210, 120, 190];
        app.begin_stroke((620.0, 130.0), false);
        app.move_stroke((700.0, 130.0), false);
        app.end_stroke();
        app.selection = Some(SelRect { x: 520.0, y: 40.0, w: 240.0, h: 180.0 });
        app.add_mask_from_selection();
        app.edit_mask = false;
        // Направляющие и сетка — чтобы на снимке было видно и их.
        app.guides.push((400.0, false));
        app.guides.push((250.0, true));
        app.show_grid = true;
        app.grid_size = 64.0;
        // Последние два слоя объединяем в папку: «Брызги» и «Мазки».
        app.doc.active = 2;
        app.make_group();
        let path = temp_file("demo.tpaint");
        app.save_project(&path).expect("демо-проект сохранён");
        // и обратно читается
        let back = crate::project::load(&path).expect("демо-проект читается");
        assert_eq!(back.layers.len(), 5);
        assert!(back.layers[1].pixels.iter().any(|v| *v != 0), "текст попал в файл");
        assert!(back.layers[4].mask.is_some(), "маска сохранилась в проекте");
        assert_eq!(back.layers[3].meta.group.as_deref(), Some("Группа 1"), "папка сохранилась");
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
