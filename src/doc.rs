//! Документ: слои, композитинг, история действий.
//!
//! Пиксели хранятся как RGBA8 (4 байта на пиксель, прямой порядок каналов),
//! непрозрачность слоя применяется при composites, а не при рисовании —
//! так переключение прозрачности слоя работает мгновенно и без потери данных.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
}

impl BlendMode {
    pub const ALL: [BlendMode; 5] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Add,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "Обычный",
            BlendMode::Multiply => "Умножение",
            BlendMode::Screen => "Экран",
            BlendMode::Overlay => "Наложение",
            BlendMode::Add => "Добавить",
        }
    }

    /// Смешивание цветов в float (0..=255): src — новый слой, dst — подложка.
    fn mix(self, src: [f32; 3], dst: [f32; 3]) -> [f32; 3] {
        match self {
            BlendMode::Normal => src,
            BlendMode::Multiply => [
                src[0] * dst[0] / 255.0,
                src[1] * dst[1] / 255.0,
                src[2] * dst[2] / 255.0,
            ],
            BlendMode::Screen => [
                255.0 - (255.0 - src[0]) * (255.0 - dst[0]) / 255.0,
                255.0 - (255.0 - src[1]) * (255.0 - dst[1]) / 255.0,
                255.0 - (255.0 - src[2]) * (255.0 - dst[2]) / 255.0,
            ],
            BlendMode::Overlay => {
                let ch = |s: f32, d: f32| {
                    if d < 128.0 {
                        2.0 * s * d / 255.0
                    } else {
                        255.0 - 2.0 * (255.0 - s) * (255.0 - d) / 255.0
                    }
                };
                [ch(src[0], dst[0]), ch(src[1], dst[1]), ch(src[2], dst[2])]
            }
            BlendMode::Add => [
                (src[0] + dst[0]).min(255.0),
                (src[1] + dst[1]).min(255.0),
                (src[2] + dst[2]).min(255.0),
            ],
        }
    }
}

/// Свойства слоя (без пикселей) — для истории и панели слоёв.
#[derive(Clone, PartialEq, Debug)]
pub struct LayerMeta {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    /// Имя группы-папки, к которой относится слой. None — слой вне папки.
    pub group: Option<String>,
    /// Папка раскрыта в панели слоёв (хранится на первом слое группы).
    pub group_open: bool,
}

#[derive(Clone)]
pub struct Layer {
    pub meta: LayerMeta,
    pub pixels: Vec<u8>,
    /// Маска слоя: 8 бит покрытия на пиксель. None — маски нет.
    pub mask: Option<Vec<u8>>,
    /// Маска включена в композитинг (можно временно выключить).
    pub mask_on: bool,
}

impl Layer {
    pub fn new(w: usize, h: usize, name: &str) -> Self {
        Self {
            meta: LayerMeta {
                name: name.to_string(),
                visible: true,
                locked: false,
                opacity: 1.0,
                blend: BlendMode::Normal,
                group: None,
                group_open: true,
            },
            pixels: vec![0; w * h * 4],
            mask: None,
            mask_on: true,
        }
    }

    #[inline]
    pub fn idx(&self, w: usize, x: usize, y: usize) -> usize {
        (y * w + x) * 4
    }

    #[inline]
    pub fn get(&self, w: usize, x: usize, y: usize) -> [u8; 4] {
        let i = self.idx(w, x, y);
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Пиксельное наложение «источника на слой» (source-over, 8-битный).
    /// Пиксели полностью прозрачного цвета не затрагиваются — как в CSP:
    /// рисование цветом с alpha = 0 ничего не меняет.
    #[inline]
    pub fn blend(&mut self, w: usize, x: usize, y: usize, c: [u8; 4]) {
        if c[3] == 0 {
            return;
        }
        let i = self.idx(w, x, y);
        let a = c[3] as u32;
        if a == 255 {
            self.pixels[i] = c[0];
            self.pixels[i + 1] = c[1];
            self.pixels[i + 2] = c[2];
            self.pixels[i + 3] = 255;
            return;
        }
        let da = self.pixels[i + 3] as u32;
        if da == 0 {
            self.pixels[i] = c[0];
            self.pixels[i + 1] = c[1];
            self.pixels[i + 2] = c[2];
            self.pixels[i + 3] = c[3];
            return;
        }
        let inv = 255 - a;
        let out_a = a + da * inv / 255;
        let ch = |s: u8, d: u8| -> u8 {
            if out_a == 0 {
                0
            } else {
                ((s as u32 * a * 255 + d as u32 * da * inv) / (out_a * 255)) as u8
            }
        };
        let nc = [ch(c[0], self.pixels[i]), ch(c[1], self.pixels[i + 1]), ch(c[2], self.pixels[i + 2])];
        self.pixels[i] = nc[0];
        self.pixels[i + 1] = nc[1];
        self.pixels[i + 2] = nc[2];
        self.pixels[i + 3] = out_a as u8;
    }

    #[inline]
    pub fn set(&mut self, w: usize, x: usize, y: usize, c: [u8; 4]) {
        if x >= w {
            return;
        }
        let i = self.idx(w, x, y);
        self.pixels[i] = c[0];
        self.pixels[i + 1] = c[1];
        self.pixels[i + 2] = c[2];
        self.pixels[i + 3] = c[3];
    }

    pub fn fill(&mut self, c: [u8; 4]) {
        for p in self.pixels.chunks_exact_mut(4) {
            p.copy_from_slice(&c);
        }
    }
}

/// Действие истории. Пиксельные изменения хранят снимок только затронутого слоя.
#[derive(Clone)]
pub enum Action {
    Pixels {
        layer: usize,
        before: Vec<u8>,
        after: Vec<u8>,
    },
    LayerAdd {
        index: usize,
    },
    LayerDelete {
        index: usize,
        layer: Box<Layer>,
    },
    LayerMove {
        from: usize,
        to: usize,
    },
    LayerMeta {
        index: usize,
        before: LayerMeta,
        after: LayerMeta,
    },
    /// Маска слоя до/после (None — маски не было или её удалили).
    Mask {
        layer: usize,
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
    },
    /// Объединение слоёв: верхний удалён, нижний изменён.
    /// Отмена возвращает оба.
    Merge {
        top_index: usize,
        top: Box<Layer>,
        under_index: usize,
        under_before: Vec<u8>,
    },
    /// Документ целиком: поворот, отражение, изменение размера холста.
    Document {
        before: Box<Document>,
        after: Box<Document>,
    },
}

/// Шаг истории вместе с названием — его показывает панель истории.
#[derive(Clone)]
pub struct Step {
    pub name: String,
    pub action: Action,
}

pub struct History {
    /// Сделанные шаги: их длина — текущее состояние документа.
    pub steps: Vec<Step>,
    /// Отменённые шаги, ждущие повтора.
    pub undone: Vec<Step>,
    pub limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self { steps: Vec::new(), undone: Vec::new(), limit }
    }

    /// Записывает шаг. limit — 0, чтобы история не росла (в тестах документов).
    pub fn push(&mut self, name: &str, a: Action) {
        if self.limit == 0 {
            return;
        }
        self.steps.push(Step { name: name.to_string(), action: a });
        if self.steps.len() > self.limit {
            self.steps.remove(0);
        }
        self.undone.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.steps.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Текущее состояние: сколько шагов уже применено.
    pub fn position(&self) -> usize {
        self.steps.len()
    }

    /// Отменяет последний шаг и возвращает его.
    pub fn undo(&mut self) -> Option<Step> {
        let s = self.steps.pop()?;
        self.undone.push(s.clone());
        Some(s)
    }

    /// Повторяет последний отменённый шаг и возвращает его.
    pub fn redo(&mut self) -> Option<Step> {
        let s = self.undone.pop()?;
        self.steps.push(s.clone());
        Some(s)
    }

    /// Перематывает историю в состояние `target` и возвращает шаги по
    /// порядку применения: сначала отмены, затем повторы. Каждый шаг нужно
    /// применить «назад», если длина steps уменьшилась, иначе — «вперёд».
    pub fn goto(&mut self, target: usize) -> Vec<(Step, bool)> {
        let mut out = Vec::new();
        while self.steps.len() > target {
            if let Some(s) = self.undo() {
                out.push((s, false));
            } else {
                break;
            }
        }
        while self.steps.len() < target {
            if let Some(s) = self.redo() {
                out.push((s, true));
            } else {
                break;
            }
        }
        out
    }
}

pub struct Document {
    pub width: usize,
    pub height: usize,
    pub layers: Vec<Layer>,
    pub active: usize,
    /// Результат наложения слоёв (RGBA8) — то, что рисуется на экране.
    pub composite: Vec<u8>,
    /// Счётчик изменений: пересчитывать композитинг только при изменении.
    pub rev: u64,
    pub shown_rev: u64,
}

impl Clone for Document {
    fn clone(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            layers: self.layers.clone(),
            active: self.active,
            // Композит не нужен: он пересчитается по слоям.
            composite: Vec::new(),
            rev: self.rev,
            shown_rev: u64::MAX,
        }
    }
}

impl Document {
    pub fn new(w: usize, h: usize) -> Self {
        let mut doc = Self {
            width: w,
            height: h,
            layers: vec![Layer::new(w, h, "Фон")],
            active: 0,
            composite: vec![0; w * h * 4],
            rev: 0,
            shown_rev: u64::MAX,
        };
        doc.touch();
        doc
    }

    /// Пустой документ без слоёв — сборка из файла проекта.
    pub fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            layers: Vec::new(),
            active: 0,
            composite: Vec::new(),
            rev: 0,
            shown_rev: u64::MAX,
        }
    }

    pub fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

    pub fn active_layer(&self) -> &Layer {
        &self.layers[self.active]
    }

    pub fn active_layer_mut(&mut self) -> &mut Layer {
        let i = self.active;
        &mut self.layers[i]
    }

    pub fn layer_name(&self, i: usize) -> String {
        let base = self.layers[i].meta.name.clone();
        let n = self
            .layers
            .iter()
            .filter(|l| l.meta.name == base)
            .count();
        if n > 1 {
            let same_before = self.layers[..i].iter().filter(|l| l.meta.name == base).count();
            format!("{} {}", base, same_before + 1)
        } else {
            base
        }
    }

    /// Наложение слоёв снизу вверх. Слои хранятся снизу вверх: layers[0] — нижний.
    pub fn recompose(&mut self) {
        let n = self.width * self.height;
        for p in self.composite.chunks_exact_mut(4) {
            p.copy_from_slice(&[0, 0, 0, 0]);
        }
        let out = &mut self.composite;
        for layer in self.layers.iter() {
            if !layer.meta.visible || layer.meta.opacity <= 0.0 {
                continue;
            }
            let op = layer.meta.opacity.clamp(0.0, 1.0);
            let mode = layer.meta.blend;
            let px = &layer.pixels;
            // Маска умножает прозрачность слоя, а её выключение даёт
            // «слой целиком», как переключатель маски в Clip Studio.
            let mask = match (&layer.mask, layer.mask_on) {
                (Some(m), true) => Some(m.as_slice()),
                _ => None,
            };
            for i in 0..n {
                let mut a = px[i * 4 + 3] as f32;
                if a == 0.0 {
                    continue;
                }
                if let Some(m) = mask {
                    a *= m[i] as f32 / 255.0;
                    if a <= 0.0 {
                        continue;
                    }
                }
                let sa = (a / 255.0) * op;
                let src = [px[i * 4] as f32, px[i * 4 + 1] as f32, px[i * 4 + 2] as f32];
                let dst = [out[i * 4] as f32, out[i * 4 + 1] as f32, out[i * 4 + 2] as f32];
                let da = out[i * 4 + 3] as f32 / 255.0;
                let blended = mode.mix(src, dst);
                let oa = sa + da * (1.0 - sa);
                for c in 0..3 {
                    let v = if oa > 0.0 {
                        (blended[c] * sa + dst[c] * da * (1.0 - sa)) / oa
                    } else {
                        0.0
                    };
                    out[i * 4 + c] = v.clamp(0.0, 255.0) as u8;
                }
                out[i * 4 + 3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
        self.shown_rev = self.rev;
    }

    pub fn ensure_composite(&mut self) {
        if self.shown_rev != self.rev {
            self.recompose();
        }
    }

    // --- операции со слоями ---

    pub fn add_layer(&mut self, name: &str) -> usize {
        let i = self.layers.len();
        self.layers.push(Layer::new(self.width, self.height, name));
        self.active = i;
        self.touch();
        i
    }

    pub fn duplicate_layer(&mut self, index: usize) -> usize {
        let mut l = self.layers[index].clone();
        l.meta.name = format!("{} копия", l.meta.name);
        self.layers.insert(index + 1, l);
        self.active = index + 1;
        self.touch();
        index + 1
    }

    pub fn delete_layer(&mut self, index: usize) -> Option<Layer> {
        if self.layers.len() <= 1 || index >= self.layers.len() {
            return None;
        }
        let l = self.layers.remove(index);
        if self.active >= self.layers.len() {
            self.active = self.layers.len() - 1;
        } else if self.active > index {
            self.active -= 1;
        }
        self.touch();
        Some(l)
    }

    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return;
        }
        let l = self.layers.remove(from);
        self.layers.insert(to, l);
        self.active = to;
        self.touch();
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        let old_h = self.height;
        if w == self.width && h == self.height {
            return;
        }
        let old_w = self.width;
        for l in self.layers.iter_mut() {
            let mut np = vec![0u8; w * h * 4];
            let cw = old_w.min(w);
            let ch = old_h.min(h);
            for y in 0..ch {
                for x in 0..cw {
                    let s = (y * old_w + x) * 4;
                    let d = (y * w + x) * 4;
                    np[d..d + 4].copy_from_slice(&l.pixels[s..s + 4]);
                }
            }
            l.pixels = np;
        }
        self.width = w;
        self.height = h;
        self.composite = vec![0; w * h * 4];
        self.touch();
    }

    /// Обрезает холст по прямоугольнику (x0, y0, w, h) в координатах холста.
    /// Содержимое сдвигается, всё за пределами обрезки теряется.
    pub fn crop(&mut self, x0: usize, y0: usize, w: usize, h: usize) {
        let x0 = x0.min(self.width);
        let y0 = y0.min(self.height);
        let w = w.clamp(1, self.width.saturating_sub(x0));
        let h = h.clamp(1, self.height.saturating_sub(y0));
        if w == self.width && h == self.height && x0 == 0 && y0 == 0 {
            return;
        }
        for l in self.layers.iter_mut() {
            let mut np = vec![0u8; w * h * 4];
            for y in 0..h.min(self.height - y0) {
                for x in 0..w.min(self.width - x0) {
                    let s = ((y + y0) * self.width + (x + x0)) * 4;
                    let d = (y * w + x) * 4;
                    np[d..d + 4].copy_from_slice(&l.pixels[s..s + 4]);
                }
            }
            l.pixels = np;
        }
        self.width = w;
        self.height = h;
        self.composite = vec![0; w * h * 4];
        self.touch();
    }

    /// Прямоугольник непрозрачного содержимого по видимым слоям.
    /// None, если всё прозрачное.
    pub fn content_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let (mut x0, mut y0) = (usize::MAX, usize::MAX);
        let (mut x1, mut y1) = (0usize, 0usize);
        let mut any = false;
        for l in self.layers.iter() {
            if !l.meta.visible || l.meta.opacity <= 0.0 {
                continue;
            }
            for y in 0..self.height {
                for x in 0..self.width {
                    if l.get(self.width, x, y)[3] == 0 {
                        continue;
                    }
                    any = true;
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if any {
            Some((x0, y0, x1 - x0, y1 - y0))
        } else {
            None
        }
    }

    /// Отражает слой: по горизонтали (слева направо) или по вертикали.
    pub fn flip_layer(&mut self, index: usize, horizontal: bool) {
        let (w, h) = (self.width, self.height);
        let l = &mut self.layers[index];
        let src = std::mem::take(&mut l.pixels);
        let mut out = vec![0u8; src.len()];
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = if horizontal { (w - 1 - x, y) } else { (x, h - 1 - y) };
                let s = (sy * w + sx) * 4;
                let d = (y * w + x) * 4;
                out[d..d + 4].copy_from_slice(&src[s..s + 4]);
            }
        }
        l.pixels = out;
        self.touch();
    }

    /// Поворачивает весь документ на 90° (четверти = 1 — по часовой).
    pub fn rotate(&mut self, quarters: i32) {
        let q = quarters.rem_euclid(4);
        if q == 0 {
            return;
        }
        let (ow, oh) = (self.width, self.height);
        // На нечётных поворотах стороны меняются местами, поэтому шаг строки
        // результата считаем уже по новой ширине.
        let (nw, nh) = if q % 2 == 1 { (oh, ow) } else { (ow, oh) };
        for l in self.layers.iter_mut() {
            let mut out = vec![0u8; nw * nh * 4];
            for y in 0..oh {
                for x in 0..ow {
                    let s = (y * ow + x) * 4;
                    let (dx, dy) = match q {
                        1 => (oh - 1 - y, x),
                        2 => (ow - 1 - x, oh - 1 - y),
                        _ => (y, ow - 1 - x),
                    };
                    let d = (dy * nw + dx) * 4;
                    out[d..d + 4].copy_from_slice(&l.pixels[s..s + 4]);
                }
            }
            l.pixels = out;
        }
        self.width = nw;
        self.height = nh;
        self.composite = vec![0; nw * nh * 4];
        self.touch();
    }

    /// Отражает весь документ: по горизонтали или по вертикали.
    pub fn mirror(&mut self, horizontal: bool) {
        let (w, h) = (self.width, self.height);
        for l in self.layers.iter_mut() {
            let src = std::mem::take(&mut l.pixels);
            let mut out = vec![0u8; src.len()];
            for y in 0..h {
                for x in 0..w {
                    let (sx, sy) = if horizontal { (w - 1 - x, y) } else { (x, h - 1 - y) };
                    let s = (sy * w + sx) * 4;
                    let d = (y * w + x) * 4;
                    out[d..d + 4].copy_from_slice(&src[s..s + 4]);
                }
            }
            l.pixels = out;
        }
        self.composite = vec![0; w * h * 4];
        self.touch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_white(w: usize, h: usize) -> Document {
        let mut d = Document::new(w, h);
        d.layers[0].fill([255, 255, 255, 255]);
        d.touch();
        d
    }

    #[test]
    fn composite_respects_opacity_and_visibility() {
        let mut d = doc_white(4, 4);
        let top = d.add_layer("верхний");
        d.layers[top].fill([0, 0, 0, 255]);
        d.touch();
        d.recompose();
        // непрозрачный чёрный поверх белого
        assert_eq!(d.composite[0], 0);
        assert_eq!(d.composite[3], 255);

        // непрозрачность 50% -> средний серый
        d.layers[top].meta.opacity = 0.5;
        d.touch();
        d.recompose();
        let v = d.composite[0];
        assert!((120..=136).contains(&v), "серый получился {v}");

        // скрытый слой не влияет
        d.layers[top].meta.opacity = 1.0;
        d.layers[top].meta.visible = false;
        d.touch();
        d.recompose();
        assert_eq!(d.composite[0], 255);
    }

    #[test]
    fn resize_preserves_content() {
        let mut d = doc_white(10, 10);
        d.layers[0].set(10, 2, 3, [255, 0, 0, 255]);
        d.resize(20, 20);
        assert_eq!(d.layers[0].pixels.len(), 20 * 20 * 4);
        assert_eq!(d.layers[0].get(20, 2, 3), [255, 0, 0, 255]);
    }

    #[test]
    fn layer_operations_keep_history_shape() {
        let mut d = doc_white(4, 4);
        let i = d.add_layer("a");
        assert_eq!(i, 1);
        let removed = d.delete_layer(1).expect("слой удаляется");
        assert_eq!(removed.meta.name, "a");
        assert_eq!(d.layers.len(), 1);
        // последний слой удалять нельзя
        assert!(d.delete_layer(0).is_none());
    }

    #[test]
    fn crop_moves_content_and_drops_the_rest() {
        let mut d = doc_white(10, 8);
        d.layers[0].set(10, 5, 2, [255, 0, 0, 255]);
        d.crop(4, 1, 4, 4);
        assert_eq!((d.width, d.height), (4, 4));
        // пиксель (5,2) стал (1,1)
        assert_eq!(d.layers[0].get(4, 1, 1), [255, 0, 0, 255]);
        assert_eq!(d.layers[0].pixels.len(), 4 * 4 * 4);
    }

    #[test]
    fn crop_clamps_to_canvas() {
        let mut d = doc_white(10, 10);
        // обрезка с выходом за края не должна паниковать и не должна выйти за холст
        d.crop(8, 8, 100, 100);
        assert_eq!((d.width, d.height), (2, 2));
        d.crop(0, 0, 0, 0);
        assert_eq!((d.width, d.height), (1, 1), "пустой размер недопустим");
    }

    #[test]
    fn content_bounds_covers_visible_layers() {
        // Прозрачный фон: иначе белый фон занял бы весь холст.
        let mut d = Document::new(20, 20);
        let top = d.add_layer("верх");
        d.layers[top].set(20, 5, 6, [0, 255, 0, 255]);
        d.layers[top].set(20, 9, 11, [0, 255, 0, 255]);
        let b = d.content_bounds().expect("есть содержимое");
        assert_eq!(b, (5, 6, 5, 6), "границы непрозрачного: {:?}", b);
        // скрытый и пустой слой в расчёт не входит
        d.layers[top].meta.visible = false;
        d.layers[top].pixels.fill(0);
        assert!(d.content_bounds().is_none(), "видимых пикселей нет");
        d.layers[top].meta.visible = true;
        d.layers[top].set(20, 1, 1, [0, 0, 255, 255]);
        assert_eq!(d.content_bounds().unwrap(), (1, 1, 1, 1), "вернулись один пиксель");
    }

    #[test]
    fn content_bounds_is_empty_for_blank_document() {
        let d = Document::new(10, 10);
        assert!(d.content_bounds().is_none(), "пустой документ без содержимого");
    }

    #[test]
    fn flip_layer_mirrors_pixels() {
        let mut d = doc_white(4, 1);
        d.layers[0].pixels.fill(0);
        d.layers[0].set(4, 0, 0, [10, 0, 0, 255]);
        d.flip_layer(0, true);
        assert_eq!(d.layers[0].get(4, 3, 0), [10, 0, 0, 255], "пиксель переехал вправо");
        d.flip_layer(0, true);
        assert_eq!(d.layers[0].get(4, 0, 0), [10, 0, 0, 255], "двойное отражение вернуло на место");
    }

    #[test]
    fn history_goto_rewinds_and_replays() {
        let mut d = doc_white(4, 4);
        let mut h = History::new(10);
        for i in 0..3u8 {
            let before = d.layers[0].pixels.clone();
            d.layers[0].set(4, i as usize, 0, [i * 40, 0, 0, 255]);
            let after = d.layers[0].pixels.clone();
            h.push("мазок", Action::Pixels { layer: 0, before, after });
        }
        assert_eq!(h.position(), 3);
        // откатываемся в начало
        let steps = h.goto(0);
        assert_eq!(steps.len(), 3);
        assert!(steps.iter().all(|(_, forward)| !forward), "все шаги назад");
        assert_eq!(h.position(), 0);
        assert!(h.can_redo());
        // и возвращаемся вперёд
        let steps = h.goto(2);
        assert_eq!(steps.len(), 2);
        assert!(steps.iter().all(|(_, forward)| *forward), "все шаги вперёд");
        assert_eq!(h.position(), 2);
    }

    #[test]
    fn history_keeps_names_and_limit() {
        let mut d = doc_white(2, 2);
        let mut h = History::new(2);
        let before = d.layers[0].pixels.clone();
        h.push("Мазок", Action::Pixels { layer: 0, before: before.clone(), after: before });
        h.push("Слой", Action::LayerAdd { index: 1 });
        h.push("Слой", Action::LayerAdd { index: 2 });
        assert_eq!(h.steps.len(), 2, "лимит истории");
        assert_eq!(h.steps[0].name, "Слой", "первый вытеснен");
        // новое действие сбрасывает повтор
        h.undo();
        assert!(h.can_redo());
        h.push("Новое", Action::LayerAdd { index: 3 });
        assert!(!h.can_redo(), "после нового действия повтор сброшен");
    }

    #[test]
    fn rotate_document_turns_pixels_and_swaps_size() {
        let mut d = Document::new(4, 2);
        d.layers[0].pixels.fill(0);
        d.layers[0].set(4, 0, 0, [255, 0, 0, 255]);
        d.rotate(1);
        assert_eq!((d.width, d.height), (2, 4), "размеры поменялись");
        // пиксель (0,0) верхнего левого угла после поворота по часовой
        // оказывается в правом верхнем углу: (1, 0)
        assert_eq!(d.layers[0].get(2, 1, 0), [255, 0, 0, 255]);
        d.rotate(3);
        assert_eq!((d.width, d.height), (4, 2), "обратный поворот вернул размер");
        assert_eq!(d.layers[0].get(4, 0, 0), [255, 0, 0, 255], "четыре поворота — исходный пиксель");
    }

    #[test]
    fn rotate_document_four_times_is_identity() {
        let mut d = doc_white(3, 5);
        d.layers[0].set(3, 2, 4, [1, 2, 3, 255]);
        let before = d.layers[0].pixels.clone();
        for _ in 0..4 {
            d.rotate(1);
        }
        assert_eq!(d.layers[0].pixels, before, "четыре поворота не меняют картинку");
        assert_eq!((d.width, d.height), (3, 5));
    }

    #[test]
    fn mirror_document_moves_pixels() {
        let mut d = Document::new(4, 1);
        d.layers[0].pixels.fill(0);
        d.layers[0].set(4, 0, 0, [9, 9, 9, 255]);
        d.mirror(true);
        assert_eq!(d.layers[0].get(4, 3, 0), [9, 9, 9, 255]);
        d.mirror(true);
        assert_eq!(d.layers[0].get(4, 0, 0), [9, 9, 9, 255]);
    }

    #[test]
    fn mask_cuts_out_layer_pixels() {
        let mut d = doc_white(4, 4);
        // Маска открыта только слева: правый столбец скрыт.
        let mut mask = vec![255u8; 16];
        for y in 0..4 {
            for x in 0..4 {
                mask[y * 4 + x] = if x < 2 { 255 } else { 0 };
            }
        }
        d.layers[0].mask = Some(mask);
        d.recompose();
        assert_eq!(d.composite[0], 255, "слева слой виден");
        assert_eq!(d.composite[3 * 4 + 3], 0, "справа маска закрывает слой");
        // Выключенная маска показывает слой целиком.
        d.layers[0].mask_on = false;
        d.recompose();
        assert_eq!(d.composite[3 * 4 + 3], 255, "выключенная маска не влияет");
    }

    #[test]
    fn mask_survives_document_clone() {
        let mut d = doc_white(4, 4);
        d.layers[0].mask = Some(vec![7; 16]);
        d.layers[0].mask_on = false;
        let c = d.clone();
        assert_eq!(c.layers[0].mask.as_ref().unwrap()[5], 7);
        assert!(!c.layers[0].mask_on);
    }

    #[test]
    fn history_can_be_disabled() {
        let mut d = doc_white(2, 2);
        let mut h = History::new(0);
        h.push("Мазок", Action::LayerAdd { index: 0 });
        assert!(!h.can_undo(), "при нулевом лимите история пустая");
    }
}
