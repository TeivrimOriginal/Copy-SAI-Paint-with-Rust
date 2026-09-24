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
}

#[derive(Clone)]
pub struct Layer {
    pub meta: LayerMeta,
    pub pixels: Vec<u8>,
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
            },
            pixels: vec![0; w * h * 4],
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
}

pub struct History {
    pub undo: Vec<Action>,
    pub redo: Vec<Action>,
    pub limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), limit }
    }

    pub fn push(&mut self, a: Action) {
        self.undo.push(a);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
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
            for i in 0..n {
                let a = px[i * 4 + 3] as f32;
                if a == 0.0 {
                    continue;
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
}
