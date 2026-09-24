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
        let scale = PxScale::from(size);
        let sf = font.as_scaled(scale);
        let gid = sf.glyph_id(ch);
        let adv = sf.h_advance(gid);
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
                    let a = ATLAS_SIZE as f32;
                    Some(GlyphEntry {
                        u: [x0 as f32 / a, y0 as f32 / a, (x0 + gw) as f32 / a, (y0 + gh) as f32 / a],
                        w: gw as f32,
                        h: gh as f32,
                        ox: b.min.x - PAD as f32,
                        // Растеризация идёт вверх, экранный Y — вниз.
                        oy: -(b.min.y + h as f32) - PAD as f32,
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
                    out.push((
                        [pen + g.ox, y + g.oy, pen + g.ox + g.w, y + g.oy + g.h],
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

fn load_font(names: &[&str]) -> Option<FontVec> {
    for n in names {
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
}
