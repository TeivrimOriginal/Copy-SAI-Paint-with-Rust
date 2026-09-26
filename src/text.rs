//! Текст: загрузка системного шрифта, растеризация глифов в атлас, выдача квадов.
//!
//! Шрифты берутся из системной папки Windows (с возможностью переопределить
//! файлом в `fonts/` рядом с программой), поэтому в репозитории нет
//! бинарных шрифтов с непонятной лицензией.

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use std::collections::HashMap;

/// Размер текстового атласа (в пикселях).
pub const ATLAS_SIZE: usize = 2048;
const PAD: i32 = 2;
/// Глифы рисуются в атлас в два раза крупнее нужного и затем уменьшаются
/// при выводе. Шрифт растеризуется без хинтинга, поэтому в мелком кегле
/// тонкие штрихи иначе расплываются; удвоение и фильтрация при уменьшении
/// дают ровное сглаживание без «лесенки».
const SS: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Weight {
    Regular,
    Bold,
}

#[derive(Clone, Copy)]
struct GlyphEntry {
    /// uv в атласе: (x0, y0, x1, y1) в пикселях
    u: [f32; 4],
    /// размер глифа в пикселях
    w: f32,
    h: f32,
    /// смещение пера относительно левого верхнего угла строки
    ox: f32,
    oy: f32,
    adv: f32,
}

pub struct Fonts {
    regular: FontVec,
    bold: Option<FontVec>,
    pub atlas: Vec<u8>,
    /// растёт при растеризации новых глифов — по нему GPU перезаливает атлас
    pub atlas_rev: u64,
    pen_x: usize,
    pen_y: usize,
    row_h: usize,
    cache: HashMap<(u8, u16, char), Option<GlyphEntry>>,
    line_cache: HashMap<u16, f32>,
}

impl Fonts {
    pub fn load() -> Result<Self, String> {
        let regular = load_font(&["segoeui.ttf", "arial.ttf", "tahoma.ttf", "verdana.ttf"])
            .ok_or_else(|| "не найден шрифт (segoeui.ttf / arial.ttf)".to_string())?;
        let bold = load_font(&["segoeuib.ttf", "arialbd.ttf", "tahomabd.ttf"]);
        Ok(Self {
            regular,
            bold,
            atlas: vec![0; ATLAS_SIZE * ATLAS_SIZE],
            atlas_rev: 0,
            pen_x: PAD as usize,
            pen_y: 0,
            row_h: 0,
            cache: HashMap::new(),
            line_cache: HashMap::new(),
        })
    }

    fn font(&self, weight: Weight) -> &FontVec {
        match weight {
            Weight::Regular => &self.regular,
            Weight::Bold => self.bold.as_ref().unwrap_or(&self.regular),
        }
    }

    /// Высота строки для кегля — для вертикального выравнивания в панелях.
    pub fn line_height(&mut self, size: f32) -> f32 {
        let k = (size * 4.0) as u16;
        if let Some(v) = self.line_cache.get(&k) {
            return *v;
        }
        let h = self.regular.as_scaled(PxScale::from(size)).height();
        self.line_cache.insert(k, h);
        h
    }

    /// Растеризует строку прямо в буфер слоя (инструмент «Текст»).
    ///
    /// (x, y) — начало пера, y — **базовая линия**: так же, как в `Ui::text`,
    /// поэтому предпросмотр на экране и результат в слое совпадают.
    pub fn draw_to_layer(
        &self,
        layer: &mut crate::doc::Layer,
        w: usize,
        h: usize,
        s: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [u8; 4],
        opacity: f32,
        weight: Weight,
    ) {
        let alpha = (color[3] as f32 / 255.0) * opacity.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let font = self.font(weight);
        let sf = font.as_scaled(PxScale::from(size));
        let mut pen = x;
        for ch in s.chars() {
            let gid = sf.glyph_id(ch);
            let adv = sf.h_advance(gid);
            let Some(outline) = font.outline_glyph(sf.scaled_glyph(ch)) else {
                pen += adv;
                continue;
            };
            let b = outline.px_bounds();
            let gw = (b.max.x - b.min.x).ceil().max(0.0) as usize;
            let gh = (b.max.y - b.min.y).ceil().max(0.0) as usize;
            if gw == 0 || gh == 0 {
                pen += adv;
                continue;
            }
            // callback отдаёт координаты 0-based внутри рамки глифа, ось Y вниз.
            let mut cov = vec![0f32; gw * gh];
            outline.draw(|gx, gy, c| {
                if (gx as usize) < gw && (gy as usize) < gh {
                    cov[gy as usize * gw + gx as usize] = c;
                }
            });
            // Рамка глифа относительно пера: левый край b.min.x, верхний край
            // b.min.y (отрицательный — выше базовой линии). У запятых и «у»
            // рамка уходит ниже линии, поэтому смещение у каждого знака своё.
            let left = pen + b.min.x.floor();
            let top = y + b.min.y.floor();
            for cy in 0..gh {
                let ty = top + cy as f32;
                if ty < 0.0 || ty as usize >= h {
                    continue;
                }
                for cx in 0..gw {
                    let a = alpha * cov[cy * gw + cx];
                    if a <= 0.002 {
                        continue;
                    }
                    let tx = left + cx as f32;
                    if tx < 0.0 || tx as usize >= w {
                        continue;
                    }
                    layer.blend(w, tx as usize, ty as usize, [
                        color[0], color[1], color[2], (a.min(1.0) * 255.0).round() as u8,
                    ]);
                }
            }
            pen += adv;
        }
    }

    /// Растеризует строку вертикально: глифы идут сверху вниз, каждый
    /// выровнен по центру колонки. Точка (x, y) — верх колонки.
    pub fn draw_vertical_to_layer(
        &self,
        layer: &mut crate::doc::Layer,
        w: usize,
        h: usize,
        s: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [u8; 4],
        opacity: f32,
        weight: Weight,
    ) {
        let alpha = (color[3] as f32 / 255.0) * opacity.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let font = self.font(weight);
        let sf = font.as_scaled(PxScale::from(size));
        // Шаг строки — высота знакоместа: буквы стоят ровно, без зазора.
        let step = (sf.ascent() - sf.descent()).max(1.0);
        let mut pen = y;
        for ch in s.chars() {
            let gid = sf.glyph_id(ch);
            let adv = sf.h_advance(gid);
            let Some(outline) = font.outline_glyph(sf.scaled_glyph(ch)) else {
                pen += step;
                continue;
            };
            let b = outline.px_bounds();
            let gw = (b.max.x - b.min.x).ceil().max(0.0) as usize;
            let gh = (b.max.y - b.min.y).ceil().max(0.0) as usize;
            if gw == 0 || gh == 0 {
                pen += step;
                continue;
            }
            let mut cov = vec![0f32; gw * gh];
            outline.draw(|gx, gy, c| {
                if (gx as usize) < gw && (gy as usize) < gh {
                    cov[gy as usize * gw + gx as usize] = c;
                }
            });
            // Глиф центрируем по колонке, а базовую линию ставим на pen.
            let baseline = pen + sf.ascent();
            let left = x + b.min.x.floor() - (adv + b.min.x - b.max.x) / 2.0;
            let top = baseline + b.min.y.floor();
            for cy in 0..gh {
                let ty = top + cy as f32;
                if ty < 0.0 || ty as usize >= h {
                    continue;
                }
                for cx in 0..gw {
                    let a = alpha * cov[cy * gw + cx];
                    if a <= 0.002 {
                        continue;
                    }
                    let tx = left + cx as f32;
                    if tx < 0.0 || tx as usize >= w {
                        continue;
                    }
                    layer.blend(w, tx as usize, ty as usize, [
                        color[0], color[1], color[2], (a.min(1.0) * 255.0).round() as u8,
                    ]);
                }
            }
            pen += step;
        }
    }

    /// Высота букв над базовой линией — нужна, чтобы поставить строку
    /// по верхнему краю, как это делает инструмент «Текст».
    pub fn ascent(&self, size: f32) -> f32 {
        self.font(Weight::Regular).as_scaled(PxScale::from(size)).ascent()
    }

    /// Шаг строки по вертикали: столько занимает одна буква в колонке.
    pub fn vertical_step(&self, size: f32, weight: Weight) -> f32 {
        let sf = self.font(weight).as_scaled(PxScale::from(size));
        (sf.ascent() - sf.descent()).max(1.0)
    }

    pub fn text_width(&mut self, s: &str, size: f32, weight: Weight) -> f32 {
        let mut w = 0.0;
        for ch in s.chars() {
            w += self.advance(ch, size, weight);
        }
        w
    }

    fn advance(&self, ch: char, size: f32, weight: Weight) -> f32 {
        let sf = self.font(weight).as_scaled(PxScale::from(size));
        let gid = sf.glyph_id(ch);
        sf.h_advance(gid)
    }

    /// Растеризует глиф в атлас (с кэшем) и возвращает его описание.
    fn glyph(&mut self, ch: char, size: f32, weight: Weight) -> Option<GlyphEntry> {
        let key = (weight as u8, (size * 4.0) as u16, ch);
        if let Some(v) = self.cache.get(&key) {
            return *v;
        }
        let font = self.font(weight);
        // Растеризуем крупнее (SS), а на экран выводим уменьшенным: фильтр
        // текстуры усредняет соседние тексели и штрих получается ровным.
        let scale = PxScale::from(size * SS);
        let sf = font.as_scaled(scale);
        let gid = sf.glyph_id(ch);
        // Ширина буквы остаётся в экранных единицах — атлас тут ни при чём.
        let adv = font.as_scaled(PxScale::from(size)).h_advance(gid);
        let glyph = sf.scaled_glyph(ch);
        let entry = match font.outline_glyph(glyph) {
            None => Some(GlyphEntry { u: [0.0; 4], w: 0.0, h: 0.0, ox: 0.0, oy: 0.0, adv }),
            Some(o) => {
                let b = o.px_bounds();
                let w = (b.max.x - b.min.x).ceil() as usize;
                let h = (b.max.y - b.min.y).ceil() as usize;
                if w == 0 || h == 0 || w >= ATLAS_SIZE || h >= ATLAS_SIZE {
                    Some(GlyphEntry { u: [0.0; 4], w: 0.0, h: 0.0, ox: 0.0, oy: 0.0, adv })
                } else {
                    let gw = w + PAD as usize * 2;
                    let gh = h + PAD as usize * 2;
                    if self.pen_x + gw > ATLAS_SIZE {
                        self.pen_x = PAD as usize;
                        self.pen_y += self.row_h + 1;
                    }
                    if self.pen_y + gh > ATLAS_SIZE {
                        // Атлас переполнен — начинаем заново (набор символов статичен).
                        self.atlas.iter_mut().for_each(|v| *v = 0);
                        // Кэш тоже чистый: иначе глифы указывали бы на чужие ячейки.
                        self.cache.clear();
                        self.pen_x = PAD as usize;
                        self.pen_y = 0;
                    }
                    let x0 = self.pen_x;
                    let y0 = self.pen_y;
                    let mut buf = vec![0u8; gw * gh];
                    o.draw(|x, y, cov| {
                        let px = x as usize + PAD as usize;
                        let py = y as usize + PAD as usize;
                        if px < gw && py < gh {
                            let i = py * gw + px;
                            buf[i] = (cov * 255.0).round() as u8;
                        }
                    });
                    for y in 0..gh {
                        let dst = (y0 + y) * ATLAS_SIZE + x0;
                        let src = &buf[y * gw..y * gw + gw];
                        let row = &mut self.atlas[dst..dst + gw];
                        for (d, s) in row.iter_mut().zip(src) {
                            if *s > 0 {
                                *d = *s;
                            }
                        }
                    }
                    self.pen_x += gw + 1;
                    self.row_h = self.row_h.max(gh);
                    // Ячейка атласа и экранный квад: uv на всю ячейку,
                    // размер и смещение — в экранных пикселях, то есть
                    // разделены на SS.
                    let a = ATLAS_SIZE as f32;
                    Some(GlyphEntry {
                        u: [x0 as f32 / a, y0 as f32 / a, (x0 + gw) as f32 / a, (y0 + gh) as f32 / a],
                        w: gw as f32 / SS,
                        h: gh as f32 / SS,
                        ox: b.min.x / SS - PAD as f32 / SS,
                        // Растеризация идёт вверх, экранный Y — вниз.
                        oy: -(b.min.y + h as f32) / SS - PAD as f32 / SS,
                        adv,
                    })
                }
            }
        };
        self.cache.insert(key, entry);
        self.atlas_rev = self.atlas_rev.wrapping_add(1);
        entry
    }

    /// Раскладка строки: возвращает список квадов (x, y — пера).
    ///
    /// Координаты квадов округляются до целых пикселей: атлас и экран имеют
    /// один масштаб, поэтому дробное положение заставляет билинейную фильтрацию
    /// смешивать соседние тексели — буквы получаются мыльными. Ширина глифа уже
    /// целая, так что размер не меняется, округляется только положение.
    pub fn layout(
        &mut self,
        s: &str,
        x: f32,
        y: f32,
        size: f32,
        weight: Weight,
        out: &mut Vec<([f32; 4], [f32; 4], f32)>,
    ) -> f32 {
        let mut pen = x;
        for ch in s.chars() {
            if let Some(g) = self.glyph(ch, size, weight) {
                if g.w > 0.0 && g.h > 0.0 {
                    let gx = (pen + g.ox).round();
                    let gy = (y + g.oy).round();
                    out.push((
                        [gx, gy, gx + g.w, gy + g.h],
                        g.u,
                        0.0,
                    ));
                }
                pen += g.adv;
            }
        }
        pen - x
    }
}

/// Растеризует строку в слой — точка входа для инструмента «Текст».
/// (x, y) — левый верхний угол строки (как при клике мышью), базовая линия
/// вычисляется по высоте букв. Шрифт подгружается заново: текст на холсте
/// рисуется редко, а держать вторую копию шрифта в состоянии незачем.
pub fn draw_text(
    layer: &mut crate::doc::Layer,
    w: usize,
    h: usize,
    s: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [u8; 4],
    opacity: f32,
    bold: bool,
) {
    let fonts = match Fonts::load() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Не удалось загрузить шрифт: {}", e);
            return;
        }
    };
    let baseline = y + fonts.ascent(size);
    fonts.draw_to_layer(
        layer, w, h, s, x, baseline, size, color, opacity,
        if bold { Weight::Bold } else { Weight::Regular },
    );
}

/// Растеризует строку вертикально: буквы идут столбиком сверху вниз,
/// (x, y) — центр верхней буквы.
pub fn draw_text_vertical(
    layer: &mut crate::doc::Layer,
    w: usize,
    h: usize,
    s: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [u8; 4],
    opacity: f32,
    bold: bool,
) {
    let fonts = match Fonts::load() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Не удалось загрузить шрифт: {}", e);
            return;
        }
    };
    fonts.draw_vertical_to_layer(
        layer, w, h, s, x, y, size, color, opacity,
        if bold { Weight::Bold } else { Weight::Regular },
    );
}

fn load_font(names: &[&str]) -> Option<FontVec> {    for n in names {
        let mut tried = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                tried.push(dir.join("fonts").join(n));
            }
        }
        tried.push(std::path::PathBuf::from("fonts").join(n));
        tried.push(std::path::PathBuf::from("C:\\Windows\\Fonts").join(n));
        for p in tried {
            if let Some(b) = std::fs::read(&p).ok() {
                if let Ok(f) = FontVec::try_from_vec(b) {
                    return Some(f);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Глиф должен растеризоваться в НЕ сплошной прямоугольник:
    /// у буквы «А» внутри есть пустоты.
    #[test]
    fn glyph_is_not_solid_block() {
        let mut f = Fonts::load().expect("шрифт");
        let g = f.glyph('А', 16.0, Weight::Regular).expect("глиф");
        assert!(g.w > 2.0 && g.h > 2.0, "глиф слишком мал: {}x{}", g.w, g.h);
        let (x0, y0, x1, y1) = (
            (g.u[0] * ATLAS_SIZE as f32) as usize,
            (g.u[1] * ATLAS_SIZE as f32) as usize,
            (g.u[2] * ATLAS_SIZE as f32) as usize,
            (g.u[3] * ATLAS_SIZE as f32) as usize,
        );
        let mut filled = 0;
        let mut total = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                total += 1;
                if f.atlas[y * ATLAS_SIZE + x] > 8 {
                    filled += 1;
                }
            }
        }
        let ratio = filled as f32 / total.max(1) as f32;
        assert!(ratio < 0.85, "глиф слишком залит: {:.2}", ratio);
        assert!(ratio > 0.01, "глиф пустой: {:.2}", ratio);
    }

    #[test]
    fn draw_text_paints_pixels_into_layer() {
        use crate::doc::Layer;
        let (w, h) = (120usize, 60usize);
        let mut l = Layer::new(w, h, "тест");
        // Якорь — левый верхний угол строки (10, 10), кегль 32.
        draw_text(&mut l, w, h, "AB", 10.0, 10.0, 32.0, [255, 0, 0, 255], 1.0, false);
        let painted = l.pixels.chunks(4).filter(|p| p[3] > 0).count();
        assert!(painted > 30, "текст должен нарисоваться, нарисовано пикселей: {}", painted);
        // Ничего не должно быть выше якоря и левее него.
        let left = (0..w).flat_map(|x| (0..h).map(move |y| (x, y))).filter(|(x, y)| *x < 10 && l.get(w, *x, *y)[3] > 0).count();
        let above = (0..w).flat_map(|x| (0..h).map(move |y| (x, y))).filter(|(x, y)| *y < 10 && l.get(w, *x, *y)[3] > 0).count();
        assert_eq!(left, 0, "левее якоря пусто");
        assert_eq!(above, 0, "выше якоря пусто");
    }

    #[test]
    fn glyphs_share_one_baseline() {
        use crate::doc::Layer;
        let (w, h) = (160usize, 60usize);
        // «A,у» — буква, запятая и буква с хвостом: базовая линия у них одна.
        let mut l = Layer::new(w, h, "тест");
        draw_text(&mut l, w, h, "A,у", 10.0, 10.0, 32.0, [0, 0, 0, 255], 1.0, false);
        // Границы буквы «A» по колонке 14.
        let a_rows: Vec<usize> = (0..h).filter(|y| l.get(w, 14, *y)[3] > 40).collect();
        assert!(!a_rows.is_empty(), "буква A нарисована");
        let (a_top, a_bottom) = (*a_rows.first().unwrap(), *a_rows.last().unwrap());
        let baseline = a_bottom + 1;
        // Запятая сидит на базовой линии: её верх заметно ниже верха буквы,
        // а низ уходит ниже линии.
        let comma_rows: Vec<usize> = (0..h).filter(|y| l.get(w, 26, *y)[3] > 40).collect();
        assert!(!comma_rows.is_empty(), "запятая нарисована");
        let (c_top, c_bottom) = (*comma_rows.first().unwrap(), *comma_rows.last().unwrap());
        assert!(c_top > a_top + 5, "запятая ниже верха буквы: {} против {}", c_top, a_top);
        assert!(c_bottom >= baseline, "хвост запятой уходит под базовую линию: {} / {}", c_bottom, baseline);
        // У «у» хвост есть ниже базовой линии — ищем его в третьем глифе.
        let tail_cols: Vec<usize> = (28..80)
            .filter(|x| (baseline..h).filter(|y| l.get(w, *x, *y)[3] > 40).count() > 0)
            .collect();
        assert!(!tail_cols.is_empty(), "у «у» есть хвост ниже базовой линии");
    }
}
