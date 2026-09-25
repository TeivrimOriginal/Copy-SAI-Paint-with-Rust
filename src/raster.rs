//! Растеризация: мягкая кисть, линии, фигуры, заливка с допуском.
//! Всё рисуется в пиксельный буфер слоя (RGBA8) методом source-over.

use crate::doc::Layer;

/// Прямоугольник выделения в координатах холста.
#[derive(Clone, Copy, Debug)]
pub struct SelRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl SelRect {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self {
            x: x0.min(x1),
            y: y0.min(y1),
            w: (x1 - x0).abs(),
            h: (y1 - y0).abs(),
        }
    }

    pub fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.x + self.w && p.1 >= self.y && p.1 < self.y + self.h
    }

    /// Прямоугольник, обрезанный по границам холста, в пикселях.
    /// None, если выделение целиком вне холста.
    fn clipped(&self, w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
        let x0 = self.x.max(0.0).floor() as usize;
        let y0 = self.y.max(0.0).floor() as usize;
        let x1 = ((self.x + self.w).ceil().max(0.0) as usize).min(w);
        let y1 = ((self.y + self.h).ceil().max(0.0) as usize).min(h);
        if x0 >= x1 || y0 >= y1 {
            None
        } else {
            Some((x0, y0, x1, y1))
        }
    }
}

/// Прямоугольный кусок слоя RGBA8 — буфер обмена и «плавающий» фрагмент.
#[derive(Clone)]
pub struct RectData {
    pub w: usize,
    pub h: usize,
    pub pixels: Vec<u8>,
}

impl RectData {
    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }
}

/// Копирует прямоугольник слоя в отдельный буфер (буфер обмена).
pub fn extract_rect(layer: &Layer, w: usize, h: usize, r: SelRect) -> RectData {
    let Some((x0, y0, x1, y1)) = r.clipped(w, h) else {
        return RectData { w: 0, h: 0, pixels: Vec::new() };
    };
    let (rw, rh) = (x1 - x0, y1 - y0);
    let mut pixels = vec![0u8; rw * rh * 4];
    for y in 0..rh {
        let src = ((y0 + y) * w + x0) * 4;
        let dst = y * rw * 4;
        pixels[dst..dst + rw * 4].copy_from_slice(&layer.pixels[src..src + rw * 4]);
    }
    RectData { w: rw, h: rh, pixels }
}

/// Делает область выделения полностью прозрачной (вырезание, удаление).
/// Пишет напрямую, потому что обычный blend прозрачным цветом не стирает.
pub fn clear_rect(layer: &mut Layer, w: usize, h: usize, r: SelRect) {
    let Some((x0, y0, x1, y1)) = r.clipped(w, h) else { return };
    for y in y0..y1 {
        let i = (y * w + x0) * 4;
        layer.pixels[i..i + (x1 - x0) * 4].fill(0);
    }
}

/// Рисует буфер на слой по левому верхнему углу (x, y) с обрезкой по краям.
pub fn blit_rect(layer: &mut Layer, w: usize, h: usize, data: &RectData, x: f32, y: f32) {
    if data.w == 0 || data.h == 0 {
        return;
    }
    let ox = x.floor() as i32;
    let oy = y.floor() as i32;
    for dy in 0..data.h as i32 {
        let ty = oy + dy;
        if ty < 0 || ty as usize >= h {
            continue;
        }
        for dx in 0..data.w as i32 {
            let tx = ox + dx;
            if tx < 0 || tx as usize >= w {
                continue;
            }
            let s = ((dy as usize) * data.w + dx as usize) * 4;
            let c = [data.pixels[s], data.pixels[s + 1], data.pixels[s + 2], data.pixels[s + 3]];
            layer.blend(w, tx as usize, ty as usize, c);
        }
    }
}

/// Отпечаток кисти: круг с мягким краем. hardness = 1 — резкий край, 0 — очень мягкий.
/// Форма отпечатка кисти: круг, эллипс или квадрат.

/// Маска эллипса, вписанного в прямоугольник. Край мягкий в один пиксель,
/// чтобы граница выделения не выглядела ступенькой.
pub fn ellipse_mask(w: usize, h: usize, r: SelRect) -> Vec<u8> {
    let mut m = vec![0u8; w * h];
    let x0 = r.x.max(0.0).floor() as i64;
    let y0 = r.y.max(0.0).floor() as i64;
    let x1 = ((r.x + r.w).ceil() as i64).min(w as i64);
    let y1 = ((r.y + r.h).ceil() as i64).min(h as i64);
    let cx = (r.x + r.w * 0.5) as f32;
    let cy = (r.y + r.h * 0.5) as f32;
    let rx = (r.w * 0.5).abs().max(0.5);
    let ry = (r.h * 0.5).abs().max(0.5);
    for y in y0..y1 {
        for x in x0..x1 {
            // Нормированное расстояние: 1 — ровно на границе эллипса.
            let dx = (x as f32 + 0.5 - cx) / rx;
            let dy = (y as f32 + 0.5 - cy) / ry;
            let d = (dx * dx + dy * dy).sqrt();
            // Сглаженный переход в один пиксель по толщине границы.
            let edge = 1.0 / (rx.min(ry) * 2.0).max(1.0);
            let v = ((1.0 - d) / edge + 0.5).clamp(0.0, 1.0);
            m[y as usize * w + x as usize] = (v * 255.0).round() as u8;
        }
    }
    m
}

/// Маска многоугольника: заливка по чётному числу пересечений (scanline).
/// По вертикали берём 4 подстроки, края по горизонтали считаем точно,
/// поэтому наклонные стороны не получаются «лесенкой».
pub fn polygon_mask(w: usize, h: usize, pts: &[(f32, f32)]) -> Vec<u8> {
    let mut acc = vec![0u16; w * h];
    if pts.len() < 3 {
        return vec![0u8; w * h];
    }
    const SUB: u16 = 4;
    const ONE: f32 = 16.0; // полное перекрытие одной подстроки
    let mut xs: Vec<f32> = Vec::with_capacity(pts.len());
    for row in 0..h {
        for s in 0..SUB {
            let yc = row as f32 + (s as f32 + 0.5) / SUB as f32;
            xs.clear();
            for i in 0..pts.len() {
                let (x0, y0) = pts[i];
                let (x1, y1) = pts[(i + 1) % pts.len()];
                // Полуоткрытое правило: общую вершину считаем один раз.
                if (y0 <= yc) != (y1 <= yc) {
                    let t = (yc - y0) / (y1 - y0);
                    xs.push(x0 + t * (x1 - x0));
                }
            }
            if xs.len() < 2 {
                continue;
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let line = &mut acc[row * w..(row + 1) * w];
            let mut i = 0;
            while i + 1 < xs.len() {
                span_add(line, xs[i], xs[i + 1], ONE);
                i += 2;
            }
        }
    }
    let scale = (SUB as f32) * ONE;
    acc.iter()
        .map(|v| ((*v as f32 / scale) * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect()
}

/// Прибавляет к строке маски отрезок [x0, x1) с частичным покрытием краёв.
fn span_add(dst: &mut [u16], x0: f32, x1: f32, one: f32) {
    let w = dst.len() as f32;
    let (a, b) = (x0.clamp(0.0, w), x1.clamp(0.0, w));
    if b <= a {
        return;
    }
    let ia = a.floor() as usize;
    let ib = (b.ceil() as usize).min(dst.len());
    for x in ia..ib {
        let cov = (b - x as f32).min(1.0) - (a - x as f32).max(0.0);
        if cov > 0.0 {
            dst[x] += (cov * one) as u16;
        }
    }
}

/// «Волшебная палочка»: заливает маску пикселями слоя, близкими по цвету
/// к образцу. `contiguous` — только соприкасающиеся области, иначе все
/// подходящие по всей ширине допуска.
pub fn wand(
    layer: &Layer,
    mask: &mut [u8],
    w: usize,
    h: usize,
    sx: i32,
    sy: i32,
    tolerance: i32,
    contiguous: bool,
) {
    if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
        return;
    }
    let target = layer.get(w, sx as usize, sy as usize);
    let tol = tolerance.clamp(0, 255) as i32;
    let near = |px: [u8; 4]| -> bool {
        // Прозрачные пиксели не притягивают к себе схожие: иначе палочка
        // выделила бы весь пустой фон вокруг картинки.
        if (px[3] == 0) != (target[3] == 0) {
            return false;
        }
        if px[3] == 0 {
            return true;
        }
        let mut sum = 0i32;
        for c in 0..3 {
            let d = px[c] as i32 - target[c] as i32;
            sum += d * d;
        }
        let da = (px[3] as i32 - target[3] as i32) / 3;
        sum + da * da <= tol * tol * 3
    };
    if !contiguous {
        for y in 0..h {
            for x in 0..w {
                if near(layer.get(w, x, y)) {
                    mask[y * w + x] = 255;
                }
            }
        }
        return;
    }
    let mut stack = vec![(sx as usize, sy as usize)];
    let mut seen = vec![false; w * h];
    while let Some((x, y)) = stack.pop() {
        let i = y * w + x;
        if seen[i] {
            continue;
        }
        seen[i] = true;
        if !near(layer.get(w, x, y)) {
            continue;
        }
        mask[i] = 255;
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }
}

/// Границы ненулевой части маски — выделение должно иметь рамку.
pub fn mask_bounds(mask: &[u8], w: usize, h: usize) -> Option<crate::raster::SelRect> {
    let (mut x0, mut y0) = (usize::MAX, usize::MAX);
    let (mut x1, mut y1) = (0usize, 0usize);
    let mut any = false;
    for y in 0..h.min(mask.len() / w.max(1)) {
        for x in 0..w {
            if mask[y * w + x] > 8 {
                any = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if !any {
        return None;
    }
    Some(crate::raster::SelRect::new(x0 as f32, y0 as f32, (x1 + 1) as f32, (y1 + 1) as f32))
}

/// Мягкий край маски выделения: размытие покрытия заданной силы.
pub fn mask_blur(mask: &mut [u8], w: usize, h: usize, px: f32) {
    let r = px.round().max(1.0) as i64;
    let src = mask.to_vec();
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut n) = (0u32, 0u32);
            for dy in -r..=r {
                let sy = y as i64 + dy;
                if sy < 0 || sy >= h as i64 {
                    continue;
                }
                for dx in -r..=r {
                    let sx = x as i64 + dx;
                    if sx < 0 || sx >= w as i64 {
                        continue;
                    }
                    if (dx * dx + dy * dy) as f32 > (r * r + r) as f32 {
                        continue;
                    }
                    sum += src[sy as usize * w + sx as usize] as u32;
                    n += 1;
                }
            }
            if n > 0 {
                mask[y * w + x] = (sum / n) as u8;
            }
        }
    }
}

/// Тень слоя: размытая альфа, сдвинутая и покрашенная. Возвращает готовый
/// буфер RGBA, который накладывается под слой.
pub fn layer_shadow(w: usize, h: usize, alpha: &[u8], dx: i32, dy: i32, blur: f32, color: [u8; 4], opacity: f32) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    if blur <= 0.0 {
        // Без размытия тень — просто сдвиг альфы.
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let sx = x - dx;
                let sy = y - dy;
                if sx < 0 || sy < 0 || sx >= w as i32 || sy >= h as i32 {
                    continue;
                }
                let a = alpha[sy as usize * w + sx as usize];
                if a == 0 {
                    continue;
                }
                let i = (y as usize * w + x as usize) * 4;
                buf[i..i + 3].copy_from_slice(&color[0..3]);
                buf[i + 3] = ((a as f32 / 255.0) * opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
        return buf;
    }
    // Размываем альфу слоя, затем сдвигаем — так тень мягче по краю.
    let mut a = alpha.to_vec();
    mask_blur(&mut a, w, h, blur);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let sx = x - dx;
            let sy = y - dy;
            if sx < 0 || sy < 0 || sx >= w as i32 || sy >= h as i32 {
                continue;
            }
            let v = a[sy as usize * w + sx as usize];
            if v == 0 {
                continue;
            }
            let i = (y as usize * w + x as usize) * 4;
            buf[i..i + 3].copy_from_slice(&color[0..3]);
            buf[i + 3] = ((v as f32 / 255.0) * opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    buf
}

/// Обводка слоя: альфа, расширенная на `size`, из которой вычтена исходная —
/// остаётся только кольцо вокруг рисунка.
pub fn layer_outline(w: usize, h: usize, alpha: &[u8], size: f32, color: [u8; 4], opacity: f32) -> Vec<u8> {
    let mut grown = alpha.to_vec();
    mask_grow(&mut grown, w, h, size.max(1.0));
    let mut buf = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let ring = (grown[i] as i32 - alpha[i] as i32).clamp(0, 255) as u8;
        if ring == 0 {
            continue;
        }
        let o = i * 4;
        buf[o..o + 3].copy_from_slice(&color[0..3]);
        buf[o + 3] = ((ring as f32 / 255.0) * opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    buf
}

/// Свечение слоя: размытая альфа минус сама альфа — ореол вокруг рисунка.
/// Возвращает буфер RGBA, который накладывается под слой.
pub fn layer_glow(
    w: usize,
    h: usize,
    alpha: &[u8],
    blur: f32,
    color: [u8; 4],
    opacity: f32,
) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    if blur <= 0.0 {
        return buf;
    }
    let mut halo = alpha.to_vec();
    mask_blur(&mut halo, w, h, blur);
    for i in 0..w * h {
        // Оставляем только то, что оказалось за пределами рисунка.
        let ring = halo[i].saturating_sub(alpha[i]);
        if ring == 0 {
            continue;
        }
        let o = i * 4;
        buf[o..o + 3].copy_from_slice(&color[0..3]);
        buf[o + 3] = ((ring as f32 / 255.0) * opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    buf
}

/// Наложение одного буфера RGBA на другой по правилам source-over.
pub fn over(dst: &mut [u8], w: usize, h: usize, src: &[u8], opacity: f32) {
    let op = opacity.clamp(0.0, 1.0);
    for i in 0..w * h {
        let sa = src[i * 4 + 3] as f32 / 255.0 * op;
        if sa <= 0.0 {
            continue;
        }
        let o = i * 4;
        let da = dst[o + 3] as f32 / 255.0;
        let oa = sa + da * (1.0 - sa);
        if oa <= 0.0 {
            continue;
        }
        for c in 0..3 {
            dst[o + c] = ((src[o + c] as f32 * sa + dst[o + c] as f32 * da * (1.0 - sa)) / oa)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        dst[o + 3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

/// Аффинное преобразование точки: x' = m[0]x + m[1]y + m[2], y' = m[3]x + m[4]y + m[5].
/// Ими описываются отражения и повороты симметрии.
#[derive(Clone, Copy, Debug)]
pub struct SymXform {
    pub m: [f32; 6],
}

impl SymXform {
    pub fn apply(&self, p: (f32, f32)) -> (f32, f32) {
        (self.m[0] * p.0 + self.m[1] * p.1 + self.m[2], self.m[3] * p.0 + self.m[4] * p.1 + self.m[5])
    }
}

/// Преобразования симметрии для мазка. Первое — тождественное (сам мазок),
/// дальше идут его отражения. `sides` — число лучей радиальной симметрии.
pub fn symmetry_xforms(sym: crate::tools::Symmetry, sides: u32, w: usize, h: usize) -> Vec<SymXform> {
    use crate::tools::Symmetry;
    let id = SymXform { m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0] };
    let (fw, fh) = (w as f32, h as f32);
    match sym {
        Symmetry::Off => vec![id],
        // Поворот на 180° вокруг центра холста.
        Symmetry::Center => vec![id, SymXform { m: [-1.0, 0.0, fw, 0.0, -1.0, fh] }],
        // Отражение относительно вертикали через центр.
        Symmetry::Vertical => vec![id, SymXform { m: [-1.0, 0.0, fw, 0.0, 1.0, 0.0] }],
        // Отражение относительно горизонтали через центр.
        Symmetry::Horizontal => vec![id, SymXform { m: [1.0, 0.0, 0.0, 0.0, -1.0, fh] }],
        Symmetry::Radial => {
            let n = sides.clamp(2, 12);
            let (cx, cy) = (fw * 0.5, fh * 0.5);
            let mut out = Vec::with_capacity((n * 2) as usize);
            for k in 0..n {
                let a = std::f32::consts::TAU * k as f32 / n as f32;
                let (s, c) = a.sin_cos();
                // Поворот вокруг центра: сначала сдвиг в начало, поворот, сдвиг назад.
                let rot = SymXform {
                    m: [
                        c,
                        -s,
                        cx - c * cx + s * cy,
                        s,
                        c,
                        cy - s * cx - c * cy,
                    ],
                };
                out.push(rot);
                // Отражение намазка по вертикали: x' = w - (поворот x).
                // Знаки первых двух коэффициентов меняются, свободный член —
                // наоборот, прибавляет ширину холста.
                out.push(SymXform {
                    m: [-rot.m[0], -rot.m[1], fw - rot.m[2], rot.m[3], rot.m[4], rot.m[5]],
                });
            }
            out
        }
    }
}

/// Расширение (плюс) или сужение (минус) маски выделения.
pub fn mask_grow(mask: &mut [u8], w: usize, h: usize, px: f32) {
    let r = px.abs().round().max(1.0) as i64;
    let src = mask.to_vec();
    let at = |x: i64, y: i64| -> u8 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            0
        } else {
            src[y as usize * w + x as usize]
        }
    };
    for y in 0..h {
        for x in 0..w {
            let c = src[y * w + x];
            // Расширение: пустые пиксели рядом с краем становятся выделенными.
            // Сужение (эрозия): выделенный пиксель убирается, если рядом есть
            // хотя бы один невыделенный — так отступает край.
            let cange = px > 0.0;
            let check_inside = if cange { c <= 8 } else { c > 128 };
            if !check_inside {
                continue;
            }
            let mut other = false;
            'outer: for dy in -r..=r {
                for dx in -r..=r {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let v = at(x as i64 + dx, y as i64 + dy);
                    if if cange { v > 8 } else { v <= 128 } {
                        other = true;
                        break 'outer;
                    }
                }
            }
            if other {
                mask[y * w + x] = if cange { 255 } else { 0 };
            }
        }
    }
}

/// Очищает выделение в слое с учётом покрытия маски.
pub fn clear_rect_mask(layer: &mut Layer, w: usize, h: usize, mask: &[u8]) {
    for y in 0..h {
        for x in 0..w {
            let cov = mask[y * w + x];
            if cov == 0 {
                continue;
            }
            let i = (y * w + x) * 4;
            let a = layer.pixels[i + 3];
            if a == 0 {
                continue;
            }
            layer.pixels[i + 3] = ((a as u32 * (255 - cov as u32) + 127) / 255) as u8;
        }
    }
}

/// Заполняет слой цветом там, где выделение, с учётом покрытия маски.
pub fn fill_mask_cover(layer: &mut Layer, w: usize, h: usize, mask: &[u8], color: [u8; 4]) {
    for y in 0..h {
        for x in 0..w {
            let cov = mask[y * w + x];
            if cov == 0 {
                continue;
            }
            let a = ((color[3] as u32 * cov as u32 + 127) / 255) as u8;
            layer.blend(w, x, y, [color[0], color[1], color[2], a]);
        }
    }
}

/// Форма отпечатка кисти: круг, эллипс или квадрат.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Round,
    Ellipse,
    Square,
}

/// Ставит dab'ы вдоль дуги Катмулла-Рома от `p1` к `p2` с шагом не больше
/// `step` пикселей. Дуга снимает угловатость на поворотах и не оставляет
/// пробелов, когда мышь движется быстро: точки берутся на кривой, а не на
/// отрезке — как в настоящей кисти.
///
/// `put(x, y)` ставит один отпечаток, поэтому одна и та же математика годится
/// и для пикселей слоя, и для градации маски.
pub fn stamp_curve<F: FnMut(f32, f32)>(
    mut put: F,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    step: f32,
) {
    let step = step.max(0.35);
    let chord = ((p2.0 - p1.0).powi(2) + (p2.1 - p1.1).powi(2)).sqrt();
    let n = ((chord / step).ceil() as usize + 1).min(512).max(2);
    let eval = |t: f32| -> (f32, f32) {
        let t2 = t * t;
        let t3 = t2 * t;
        // Базис Катмулла-Рома: кривая проходит через p1 и p2, касания в них
        // задаются направлениями p1−p0 и p3−p2.
        (
            (-0.5 * t3 + t2 - 0.5 * t) * p0.0
                + (1.5 * t3 - 2.5 * t2 + 1.0) * p1.0
                + (-1.5 * t3 + 2.0 * t2 + 0.5 * t) * p2.0
                + (0.5 * t3 - 0.5 * t2) * p3.0,
            (-0.5 * t3 + t2 - 0.5 * t) * p0.1
                + (1.5 * t3 - 2.5 * t2 + 1.0) * p1.1
                + (-1.5 * t3 + 2.0 * t2 + 0.5 * t) * p2.1
                + (0.5 * t3 - 0.5 * t2) * p3.1,
        )
    };
    // Отпечатки ставим через равные расстояния ПО ДЛИНЕ дуги, а не через
    // равные приращения параметра: на повороте иначе краска ложится гуще,
    // и между dab'ами появляются зазоры.
    let samples: Vec<(f32, f32)> = (0..=n).map(|i| eval(i as f32 / n as f32)).collect();
    let mut acc = vec![0.0f32; samples.len()];
    for i in 1..samples.len() {
        let d = ((samples[i].0 - samples[i - 1].0).powi(2) + (samples[i].1 - samples[i - 1].1).powi(2)).sqrt();
        acc[i] = acc[i - 1] + d;
    }
    let total = acc[samples.len() - 1];
    if total <= 0.0 {
        put(samples[0].0, samples[0].1);
        return;
    }
    let count = (total / step).ceil().max(1.0) as usize;
    let mut seg = 0usize;
    for i in 0..=count {
        let target = total * i as f32 / count as f32;
        // Последний dab' равен последней точке дуги: за seg'е не выходим.
        if target >= total {
            let p = samples[samples.len() - 1];
            put(p.0, p.1);
            continue;
        }
        while seg + 1 < acc.len() - 1 && acc[seg + 1] < target {
            seg += 1;
        }
        let d = acc[seg + 1] - acc[seg];
        let t = if d > 1e-6 { (target - acc[seg]) / d } else { 0.0 };
        let a = samples[seg];
        let b = samples[seg + 1];
        put(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    }
}
/// Расстояние от центра отпечатка в «единицах радиуса»: 1 — это край.
/// Круг — обычное расстояние, квадрат — модуль большей координаты,
/// эллипс — расстояние, нормированное по полуосям 1.9 : 1 (шире по X).
#[inline]
fn shape_dist(shape: Shape, dx: f32, dy: f32) -> f32 {
    match shape {
        Shape::Round => (dx * dx + dy * dy).sqrt(),
        Shape::Square => dx.abs().max(dy.abs()),
        Shape::Ellipse => {
            const A: f32 = 1.9;
            (dx * dx / (A * A) + dy * dy).sqrt()
        }
    }
}

/// Отпечаток кисти заданной формы.
#[allow(clippy::too_many_arguments)]
pub fn stamp_shape(
    layer: &mut Layer,
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    color: [u8; 4],
    opacity: f32,
    shape: Shape,
) {
    let r = radius.max(0.5);
    let hardness = hardness.clamp(0.0, 1.0);
    let inner = r * hardness;
    // Полуоси по X: у эллипса он в 1.9 раза больше радиуса, поэтому
    // границы считаем по форме, иначе эллипс обрезался бы по кругу.
    let ax = match shape {
        Shape::Ellipse => r * 1.9,
        _ => r,
    };
    let x0 = ((cx - ax).floor().max(0.0)) as i64;
    let x1 = ((cx + ax).ceil().min((w - 1) as f32)) as i64;
    let y0 = ((cy - r).floor().max(0.0)) as i64;
    let y1 = ((cy + r).ceil().min((h - 1) as f32)) as i64;
    let base_a = (color[3] as f32 / 255.0) * opacity.clamp(0.0, 1.0);
    if base_a <= 0.0 {
        return;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = shape_dist(shape, dx, dy);
            let cover = if d <= inner {
                1.0
            } else if d >= r {
                continue;
            } else {
                let t = (d - inner) / (r - inner).max(0.0001);
                let t = 1.0 - t;
                t * t * (3.0 - 2.0 * t) // smoothstep
            };
            let a = (base_a * cover).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let c = [color[0], color[1], color[2], (a * 255.0).round() as u8];
            layer.blend(w, x as usize, y as usize, c);
        }
    }
}

pub fn stamp(
    layer: &mut Layer,
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    color: [u8; 4],
    opacity: f32,
) {
    stamp_shape(layer, w, h, cx, cy, radius, hardness, color, opacity, Shape::Round);
}

/// Линия кистью: отпечатки с шагом ~0.25 радиуса — без разрывов и «ступенек».
#[allow(clippy::too_many_arguments)]
pub fn brush_line(
    layer: &mut Layer,
    w: usize,
    h: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    hardness: f32,
    color: [u8; 4],
    opacity: f32,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let dist = (dx * dx + dy * dy).sqrt();
    let step = (radius * 0.25).max(0.35);
    let n = (dist / step).ceil().max(1.0);
    let n = n.min(4000.0);
    for i in 0..=n as i32 {
        let t = i as f32 / n;
        stamp(layer, w, h, x0 + dx * t, y0 + dy * t, radius, hardness, color, opacity);
    }
}

/// Линия кистью заданной формы — отпечатки идут с шагом в четверть радиуса.
#[allow(clippy::too_many_arguments)]
pub fn brush_line_shape(
    layer: &mut Layer,
    w: usize,
    h: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    hardness: f32,
    color: [u8; 4],
    opacity: f32,
    shape: Shape,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let dist = (dx * dx + dy * dy).sqrt();
    let step = (radius * 0.25).max(0.35);
    let n = (dist / step).ceil().max(1.0).min(4000.0);
    for i in 0..=n as i32 {
        let t = i as f32 / n;
        stamp_shape(layer, w, h, x0 + dx * t, y0 + dy * t, radius, hardness, color, opacity, shape);
    }
}

/// Линейный градиент от цвета `a` к цвету `b` вдоль отрезка (x0,y0)→(x1,y1).
/// Мягкость 0..=1 размывает края (как мягкая кисть).
#[allow(clippy::too_many_arguments)]
pub fn gradient(
    layer: &mut Layer,
    w: usize,
    h: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    a: [u8; 4],
    b: [u8; 4],
    opacity: f32,
    softness: f32,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    // Минимум 1 px: иначе «жёсткий» градиент рисует еле заметную полосу
    // (в отсчёте от центра пикселя половина уходит в соседний ряд).
    let soft = (softness.clamp(0.0, 1.0) * 0.5 * len).max(1.0);
    let pad = soft + 1.0;
    let minx = ((x0.min(x1) - pad).floor().max(0.0)) as usize;
    let maxx = ((x0.max(x1) + pad).ceil().min((w - 1) as f32)) as usize;
    let miny = ((y0.min(y1) - pad).floor().max(0.0)) as usize;
    let maxy = ((y0.max(y1) + pad).ceil().min((h - 1) as f32)) as usize;
    let alpha_a = (a[3] as f32 / 255.0) * opacity.clamp(0.0, 1.0);
    let alpha_b = (b[3] as f32 / 255.0) * opacity.clamp(0.0, 1.0);
    for y in miny..=maxy {
        for x in minx..=maxx {
            let px = x as f32 + 0.5 - x0;
            let py = y as f32 + 0.5 - y0;
            // проекция на ось градиента, нормализованная в 0..=1 вдоль отрезка
            let t = ((px * dx + py * dy) / (len * len)).clamp(0.0, 1.0);
            // расстояние до прямой отрезка
            let perp = (px * dy - py * dx).abs() / len;
            // Ширину полосы задаёт расстояние до прямой отрезка; торцы плоские,
            // как у линейного градиента в графических редакторах.
            let cover = 1.0 - (perp / soft).clamp(0.0, 1.0);
            if cover <= 0.003 {
                continue;
            }
            let col = [
                (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t).round() as u8,
                (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t).round() as u8,
                (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t).round() as u8,
            ];
            let alpha = (alpha_a + (alpha_b - alpha_a) * t) * cover;
            let c = [col[0], col[1], col[2], (alpha * 255.0).round() as u8];
            layer.blend(w, x, y, c);
        }
    }
}

/// Контур или заливка прямоугольника.
#[allow(clippy::too_many_arguments)]
pub fn rect(
    layer: &mut Layer,
    w: usize,
    h: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    filled: bool,
    radius: f32,
    hardness: f32,
    color: [u8; 4],
    opacity: f32,
) {
    let (ax, bx) = (x0.min(x1), x0.max(x1));
    let (ay, by) = (y0.min(y1), y0.max(y1));
    if filled {
        let y0i = (ay.floor().max(0.0)) as usize;
        let y1i = (by.ceil().min((h - 1) as f32)) as usize;
        let x0i = (ax.floor().max(0.0)) as usize;
        let x1i = (bx.ceil().min((w - 1) as f32)) as usize;
        let c = [color[0], color[1], color[2], (color[3] as f32 * opacity).round() as u8];
        for y in y0i..=y1i.min(h - 1) {
            for x in x0i..=x1i.min(w - 1) {
                layer.blend(w, x, y, c);
            }
        }
        return;
    }
    let r = radius.max(0.5);
    brush_line(layer, w, h, ax, ay, bx, ay, r, hardness, color, opacity);
    brush_line(layer, w, h, ax, by, bx, by, r, hardness, color, opacity);
    brush_line(layer, w, h, ax, ay, ax, by, r, hardness, color, opacity);
    brush_line(layer, w, h, bx, ay, bx, by, r, hardness, color, opacity);
}

/// Контур или заливка эллипса (по двум крайним точкам, как в CSP).
#[allow(clippy::too_many_arguments)]
pub fn ellipse(
    layer: &mut Layer,
    w: usize,
    h: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    filled: bool,
    radius: f32,
    hardness: f32,
    color: [u8; 4],
    opacity: f32,
) {
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let a = ((x1 - x0).abs() * 0.5).max(0.5);
    let b = ((y1 - y0).abs() * 0.5).max(0.5);
    if filled {
        let y0i = (cy - b).floor().max(0.0) as usize;
        let y1i = (cy + b).ceil().min((h - 1) as f32) as usize;
        let c = [color[0], color[1], color[2], (color[3] as f32 * opacity).round() as u8];
        for y in y0i..=y1i.min(h - 1) {
            let dy = (y as f32 + 0.5 - cy) / b;
            let t = 1.0 - dy * dy;
            if t < 0.0 {
                continue;
            }
            let xr = a * t.sqrt();
            let xa = ((cx - xr).round().max(0.0)) as usize;
            let xb = ((cx + xr).round().min((w - 1) as f32)) as usize;
            for x in xa..=xb.min(w - 1) {
                layer.blend(w, x, y, c);
            }
        }
        return;
    }
    // Контур: обходим по углу, густота шага зависит от радиуса кисти.
    let perimeter = std::f32::consts::PI * (3.0 * (a + b) - ((a * b * 3.0).sqrt() + (2.0 * a * b).sqrt()));
    let steps = ((perimeter / ((radius * 0.25).max(0.35))).ceil().max(32.0)).min(8000.0);
    let mut px = cx + a;
    let mut py = cy;
    for i in 0..=steps as i32 {
        let t = i as f32 / steps;
        let ang = t * std::f32::consts::PI * 2.0;
        let x = cx + a * ang.cos();
        let y = cy + b * ang.sin();
        if i > 0 {
            let d = ((x - px) * (x - px) + (y - py) * (y - py)).sqrt();
            if d >= (radius * 0.25).max(0.35) {
                brush_line(layer, w, h, px, py, x, y, radius.max(0.5), hardness, color, opacity);
                px = x;
                py = y;
            }
        }
    }
}

// --- фильтры слоя (коррекция) ---

/// Размытие по Гауссу, приближённое тремя проходами «ящика».
/// Радиус в пикселях, края не растягиваются — прозрачное остаётся прозрачным.
pub fn blur(layer: &mut Layer, w: usize, h: usize, radius: f32) {
    let r = radius.round().max(0.0) as i64;
    if r <= 0 {
        return;
    }
    let mut a = std::mem::take(&mut layer.pixels);
    let mut b = vec![0u8; a.len()];
    for _ in 0..3 {
        // по X
        for y in 0..h {
            for x in 0..w {
                let (mut sr, mut sg, mut sb, mut sa, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for d in -r..=r {
                    let sx = x as i64 + d;
                    if sx < 0 || sx >= w as i64 {
                        continue;
                    }
                    let i = (y * w + sx as usize) * 4;
                    // цвет берём взвешенным по прозрачности, иначе края темнеют
                    let al = a[i + 3] as u32;
                    sr += a[i] as u32 * al;
                    sg += a[i + 1] as u32 * al;
                    sb += a[i + 2] as u32 * al;
                    sa += al;
                    n += 1;
                }
                let o = (y * w + x) * 4;
                if sa == 0 || n == 0 {
                    b[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                } else {
                    b[o] = (sr / sa) as u8;
                    b[o + 1] = (sg / sa) as u8;
                    b[o + 2] = (sb / sa) as u8;
                    b[o + 3] = (sa / n) as u8;
                }
            }
        }
        // по Y
        for y in 0..h {
            for x in 0..w {
                let (mut sr, mut sg, mut sb, mut sa, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for d in -r..=r {
                    let sy = y as i64 + d;
                    if sy < 0 || sy >= h as i64 {
                        continue;
                    }
                    let i = (sy as usize * w + x) * 4;
                    let al = b[i + 3] as u32;
                    sr += b[i] as u32 * al;
                    sg += b[i + 1] as u32 * al;
                    sb += b[i + 2] as u32 * al;
                    sa += al;
                    n += 1;
                }
                let o = (y * w + x) * 4;
                if sa == 0 || n == 0 {
                    a[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                } else {
                    a[o] = (sr / sa) as u8;
                    a[o + 1] = (sg / sa) as u8;
                    a[o + 2] = (sb / sa) as u8;
                    a[o + 3] = (sa / n) as u8;
                }
            }
        }
    }
    layer.pixels = a;
}

/// Резкость: нерезкое маскирование (вычитание размытого из оригинала).
pub fn sharpen(layer: &mut Layer, w: usize, h: usize, amount: f32) {
    let a = amount.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let original = layer.pixels.clone();
    let mut soft = layer.clone();
    blur(&mut soft, w, h, 1.0);
    for i in 0..layer.pixels.len() / 4 {
        let o = i * 4;
        for c in 0..3 {
            let v = original[o + c] as f32 + (original[o + c] as f32 - soft.pixels[o + c] as f32) * a * 2.0;
            layer.pixels[o + c] = v.clamp(0.0, 255.0) as u8;
        }
    }
}

/// Яркость, контраст, насыщенность и оттенок одним проходом.
/// Все значения в диапазоне -1..=1 (оттенок — -1..=1, то есть ±180°).
pub fn adjust(layer: &mut Layer, brightness: f32, contrast: f32, saturation: f32, hue: f32) {
    if brightness.abs() < 0.001 && contrast.abs() < 0.001 && saturation.abs() < 0.001 && hue.abs() < 0.001 {
        return;
    }
    // Контраст: коэффициент, дающий 0.5 серого без изменения среднего.
    let c = (contrast.clamp(-1.0, 1.0) * 2.0).exp();
    let b = brightness.clamp(-1.0, 1.0) * 255.0;
    let s = saturation.clamp(-1.0, 1.0);
    let hshift = hue.clamp(-1.0, 1.0) * 180.0;
    for p in layer.pixels.chunks_exact_mut(4) {
        if p[3] == 0 {
            continue;
        }
        let mut r = p[0] as f32;
        let mut g = p[1] as f32;
        let mut bl = p[2] as f32;
        // оттенок через поворот в HSV
        if hshift.abs() > 0.01 {
            let (mut hh, ss, vv) = rgb_to_hsv_f(r, g, bl);
            hh = (hh + hshift / 360.0).rem_euclid(1.0);
            // hsv_to_rgb_f работает в 0..=1, а каналы слоя — в 0..=255
            let (nr, ng, nb) = hsv_to_rgb_f(hh, ss, vv);
            r = nr * 255.0;
            g = ng * 255.0;
            bl = nb * 255.0;
        }
        // насыщенность
        if s.abs() > 0.01 {
            let lum = 0.2126 * r + 0.7152 * g + 0.0722 * bl;
            r = lum + (r - lum) * (1.0 + s);
            g = lum + (g - lum) * (1.0 + s);
            bl = lum + (bl - lum) * (1.0 + s);
        }
        // контраст и яркость
        r = (r - 128.0) * c + 128.0 + b;
        g = (g - 128.0) * c + 128.0 + b;
        bl = (bl - 128.0) * c + 128.0 + b;
        p[0] = r.clamp(0.0, 255.0) as u8;
        p[1] = g.clamp(0.0, 255.0) as u8;
        p[2] = bl.clamp(0.0, 255.0) as u8;
    }
}

fn rgb_to_hsv_f(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d <= 0.0 {
        0.0
    } else if mx == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if mx == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let s = if mx <= 0.0 { 0.0 } else { d / mx };
    (h, s, mx / 255.0)
}

fn hsv_to_rgb_f(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match (i as i32) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Один мазок по маске слоя: смешивает покрытие (0..255) с целевым
/// значением в радиусе кисти. value = 255 открывает маску, 0 закрывает.
#[allow(clippy::too_many_arguments)]
pub fn mask_stamp_shape(
    mask: &mut [u8],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    value: u8,
    opacity: f32,
    shape: Shape,
) {
    let r = radius.max(0.5);
    let ax = match shape {
        Shape::Ellipse => r * 1.9,
        _ => r,
    };
    let x0 = (cx - ax).floor().max(0.0) as usize;
    let y0 = (cy - r).floor().max(0.0) as usize;
    let x1 = ((cx + ax).ceil() as i64 + 1).min(w as i64).max(0) as usize;
    let y1 = ((cy + r).ceil() as i64 + 1).min(h as i64).max(0) as usize;
    let op = opacity.clamp(0.0, 1.0);
    let hard = hardness.clamp(0.0, 1.0);
    for y in y0..y1 {
        for x in x0..x1 {
            let d = shape_dist(shape, x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if d > r {
                continue;
            }
            // Мягкий край: 1 в центре, 0 на краю, hardness поднимает «плечо».
            let t = (d / r).clamp(0.0, 1.0);
            let edge = (t - hard) / (1.0 - hard).max(0.01);
            let cov = ((1.0 - edge) * op).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let i = y * w + x;
            mask[i] = (mask[i] as f32 * (1.0 - cov) + value as f32 * cov).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Мазок по маске вдоль отрезка: dab'ы ставятся с шагом в четверть радиуса.
#[allow(clippy::too_many_arguments)]
pub fn mask_line_shape(
    mask: &mut [u8],
    w: usize,
    h: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    hardness: f32,
    value: u8,
    opacity: f32,
    shape: Shape,
) {
    let dist = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
    let step = (radius * 0.25).max(0.6);
    let n = (dist / step).ceil().max(1.0) as usize;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        mask_stamp_shape(
            mask, w, h, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, radius, hardness, value, opacity, shape,
        );
    }
}

/// Переносит прямоугольник буфера на место с поворотом и масштабом
/// (как свободная трансформация в Clip Studio). Билинейная интерполяция,
/// углы поворота считаются по четырём точкам: (sx, sy) — левый верхний
/// угол исходного прямоугольника, (dx0, dy0)… — куда его растянуть.
pub fn blit_transformed(
    layer: &mut Layer,
    w: usize,
    h: usize,
    data: &RectData,
    pts: [(f32, f32); 4],
) {
    if data.w == 0 || data.h == 0 {
        return;
    }
    // Обратное аффинное преобразование: из экранных координат в исходные.
    let (p0, p1, _p2, p3) = (pts[0], pts[1], pts[2], pts[3]);
    let (ux, uy) = (p1.0 - p0.0, p1.1 - p0.1);
    let (vx, vy) = (p3.0 - p0.0, p3.1 - p0.1);
    let det = ux * vy - vx * uy;
    if det.abs() < 1e-4 {
        return;
    }
    let (fw, fh) = (data.width() as f32, data.height() as f32);
    // Границы результата — по углам.
    let minx = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let miny = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let maxx = (pts.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil() as i64).min(w as i64).max(0) as usize;
    let maxy = (pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i64).min(h as i64).max(0) as usize;
    for ty in miny..maxy {
        for tx in minx..maxx {
            let px = tx as f32 + 0.5 - p0.0;
            let py = ty as f32 + 0.5 - p0.1;
            // Параметры исходного прямоугольника: 0..1 по каждой оси
            let a = (px * vy - vx * py) / det;
            let b = (ux * py - px * uy) / det;
            if a < 0.0 || b < 0.0 || a > 1.0 || b > 1.0 {
                continue;
            }
            let sx = a * fw - 0.5;
            let sy = b * fh - 0.5;
            let (x0, y0) = (sx.floor().max(0.0) as usize, sy.floor().max(0.0) as usize);
            let (x1, y1) = ((sx + 1.0).ceil().min(fw) as usize, (sy + 1.0).ceil().min(fh) as usize);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let (mut r, mut g, mut bl, mut al) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = (yy * data.width() + xx) * 4;
                    let a = data.pixels[i + 3] as f32;
                    // Усредняем с умножением на альфу, иначе у прозрачных
                    // соседей цвет уезжает в чёрный и получается тёмный кант.
                    r += data.pixels[i] as f32 * a;
                    g += data.pixels[i + 1] as f32 * a;
                    bl += data.pixels[i + 2] as f32 * a;
                    al += a;
                }
            }
            let n = ((x1 - x0) * (y1 - y0)) as f32;
            if al <= 0.0 {
                continue;
            }
            let c = [
                (r / al).round().clamp(0.0, 255.0) as u8,
                (g / al).round().clamp(0.0, 255.0) as u8,
                (bl / al).round().clamp(0.0, 255.0) as u8,
                (al / n).round().clamp(0.0, 255.0) as u8,
            ];
            layer.blend(w, tx, ty, c);
        }
    }
}

/// Заливка области. tolerance — допуск по цвету, contiguous — только
/// соприкасающиеся области (false — заменить все совпадающие пиксели).
pub fn flood_fill(
    layer: &mut Layer,
    w: usize,
    h: usize,
    sx: i32,
    sy: i32,
    color: [u8; 4],
    opacity: f32,
    tolerance: u8,
    contiguous: bool,
) {
    if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
        return;
    }
    let idx = (sy as usize * w + sx as usize) * 4;
    let target = [
        layer.pixels[idx],
        layer.pixels[idx + 1],
        layer.pixels[idx + 2],
        layer.pixels[idx + 3],
    ];
    if target == color && color[3] as f32 * opacity >= 254.0 {
        return;
    }
    let nc = [color[0], color[1], color[2], (color[3] as f32 * opacity).round() as u8];
    if nc[3] == 0 {
        return;
    }
    let tol = tolerance as u32;
    let close = |a: [u8; 4], b: [u8; 4]| -> bool {
        if !contiguous {
            return false;
        }
        let d = (a[0] as i32 - b[0] as i32).abs()
            + (a[1] as i32 - b[1] as i32).abs()
            + (a[2] as i32 - b[2] as i32).abs()
            + (a[3] as i32 - b[3] as i32).abs();
        d <= tol as i32 * 4
    };

    if !contiguous {
        for i in 0..(w * h) {
            let p = [layer.pixels[i * 4], layer.pixels[i * 4 + 1], layer.pixels[i * 4 + 2], layer.pixels[i * 4 + 3]];
            let d = (p[0] as i32 - target[0] as i32).abs()
                + (p[1] as i32 - target[1] as i32).abs()
                + (p[2] as i32 - target[2] as i32).abs()
                + (p[3] as i32 - target[3] as i32).abs();
            if d <= tol as i32 * 4 {
                layer.set(w, i % w, i / w, nc);
            }
        }
        return;
    }

    let mut stack = vec![(sx, sy)];
    let mut seen = vec![false; w * h];
    while let Some((x, y)) = stack.pop() {
        if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
            continue;
        }
        let i = y as usize * w + x as usize;
        if seen[i] {
            continue;
        }
        let p = [layer.pixels[i * 4], layer.pixels[i * 4 + 1], layer.pixels[i * 4 + 2], layer.pixels[i * 4 + 3]];
        if p != target && !close(p, target) {
            continue;
        }
        seen[i] = true;
        layer.set(w, x as usize, y as usize, nc);
        stack.push((x + 1, y));
        stack.push((x - 1, y));
        stack.push((x, y + 1));
        stack.push((x, y - 1));
    }
}

/// Цвет пикселя (для пипетки).
pub fn pick(layer: &Layer, w: usize, h: usize, x: i32, y: i32) -> [u8; 4] {
    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
        [0, 0, 0, 0]
    } else {
        layer.get(w, x as usize, y as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пустой буфер прямоугольника для тестов трансформации.
    fn rect_data(w: usize, h: usize) -> RectData {
        RectData { w, h, pixels: vec![0u8; w * h * 4] }
    }
    use crate::doc::Layer;

    fn empty(w: usize, h: usize) -> Layer {
        Layer::new(w, h, "тест")
    }

    #[test]
    fn extract_clear_and_blit_round_trip() {
        let (w, h) = (16, 16);
        let mut l = empty(w, h);
        // красим красный квадрат 8×8 в позиции (4,4)
        rect(&mut l, w, h, 4.0, 4.0, 12.0, 12.0, true, 0.5, 1.0, [255, 0, 0, 255], 1.0);
        let sel = SelRect::new(4.0, 4.0, 12.0, 12.0);
        let data = extract_rect(&l, w, h, sel);
        assert_eq!((data.width(), data.height()), (8, 8));
        assert_eq!(&data.pixels[0..4], &[255, 0, 0, 255], "буфер содержит вырезанное");

        clear_rect(&mut l, w, h, sel);
        assert_eq!(l.get(w, 8, 8)[3], 0, "выделение очищено");
        assert_eq!(l.get(w, 1, 1)[3], 0, "снаружи всё равно пусто");

        // вставляем в другое место — квадрат появляется там же
        blit_rect(&mut l, w, h, &data, 0.0, 0.0);
        assert_eq!(l.get(w, 3, 3), [255, 0, 0, 255]);
    }

    #[test]
    fn selection_ops_clip_to_canvas() {
        let (w, h) = (8, 8);
        let mut l = empty(w, h);
        rect(&mut l, w, h, -5.0, -5.0, 20.0, 20.0, true, 0.5, 1.0, [0, 255, 0, 255], 1.0);
        // выделение целиком вне холста — буфер пустой, очистка безопасна
        let outside = SelRect::new(100.0, 100.0, 110.0, 110.0);
        let data = extract_rect(&l, w, h, outside);
        assert_eq!(data.width(), 0);
        clear_rect(&mut l, w, h, outside);
        assert_eq!(l.get(w, 4, 4), [0, 255, 0, 255], "слой не тронут");

        // выделение, вылезающее за края, обрезается и не паникует
        let over = SelRect::new(-4.0, -4.0, 4.0, 4.0);
        let d2 = extract_rect(&l, w, h, over);
        assert_eq!((d2.width(), d2.height()), (4, 4));
        let probe = extract_rect(&l, w, h, SelRect::new(6.0, 6.0, 30.0, 30.0));
        assert_eq!((probe.width(), probe.height()), (2, 2));
    }

    #[test]
    fn blur_spreads_pixels_and_keeps_alpha_edges() {
        let (w, h) = (16, 8);
        let mut l = empty(w, h);
        l.set(w, 8, 4, [255, 255, 255, 255]);
        blur(&mut l, w, h, 2.0);
        // белый центр размазался в соседей
        assert!(l.get(w, 7, 4)[3] > 0, "слева от центра появился пиксель");
        assert!(l.get(w, 9, 4)[3] > 0, "справа от центра появился пиксель");
        // дальние пиксели не тронуты
        assert_eq!(l.get(w, 0, 0)[3], 0, "угол остался прозрачным");
        // размытие не выходит за края и не затирает полностью прозрачное
        assert_eq!(l.get(w, 15, 7)[3], 0);
    }

    #[test]
    fn adjust_brightness_contrast_and_saturation() {
        let (w, h) = (8, 8);
        let mut l = empty(w, h);
        l.fill([128, 128, 128, 255]);
        adjust(&mut l, 0.2, 0.0, 0.0, 0.0);
        assert!(l.get(w, 0, 0)[0] > 170, "ярче: {:?}", l.get(w, 0, 0));

        let mut l2 = empty(w, h);
        l2.fill([128, 128, 128, 255]);
        adjust(&mut l2, 0.0, -1.0, 0.0, 0.0);
        assert!(l2.get(w, 0, 0)[0] < 140, "контраст ниже: {:?}", l2.get(w, 0, 0));

        // насыщенность: серый не меняется, чистый красный при -1 становится серым
        let mut l3 = empty(w, h);
        l3.fill([255, 0, 0, 255]);
        adjust(&mut l3, 0.0, 0.0, -1.0, 0.0);
        let p = l3.get(w, 0, 0);
        assert!(p[1] > 30 && p[2] > 30, "красный стал серым: {:?}", p);
    }

    #[test]
    fn adjust_hue_rotates_color() {
        let (w, h) = (4, 4);
        let mut l = empty(w, h);
        l.fill([255, 0, 0, 255]);
        adjust(&mut l, 0.0, 0.0, 0.0, 2.0 / 3.0);
        let p = l.get(w, 0, 0);
        // +120° от красного — зелёный
        assert!(p[1] > 180 && p[0] < 80, "ожидался зелёный: {:?}", p);
    }

    #[test]
    fn sharpen_increases_local_contrast() {
        let (w, h) = (16, 8);
        let mut l = empty(w, h);
        l.set(w, 8, 4, [255, 255, 255, 255]);
        let flat_before = l.get(w, 7, 4).clone();
        sharpen(&mut l, w, h, 1.0);
        let after = l.get(w, 7, 4);
        assert!(
            after[0] > flat_before[0].max(0) || after[0] == 0,
            "рядом с точкой стало контрастнее: {:?} -> {:?}", flat_before, after
        );
    }

    #[test]
    fn adjustment_skips_fully_transparent_pixels() {
        let (w, h) = (4, 4);
        let mut l = empty(w, h);
        l.pixels.iter_mut().for_each(|v| *v = 0);
        adjust(&mut l, 1.0, 1.0, 1.0, 1.0);
        assert!(l.pixels.iter().all(|v| *v == 0), "прозрачное не должно просыпаться");
    }

    #[test]
    fn stamp_curve_fills_line_without_gaps() {
        let mut pts: Vec<(f32, f32)> = Vec::new();
        stamp_curve(
            |x, y| pts.push((x, y)),
            (0.0, 0.0), (0.0, 0.0), (30.0, 0.0), (30.0, 0.0),
            2.0,
        );
        assert!(pts.len() > 10, "слишком мало точек: {}", pts.len());
        // Шаг между соседними точками не превышает заданный.
        for w in pts.windows(2) {
            let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            assert!(d <= 2.01, "разрыв в мазке: {}", d);
        }
        assert!((pts.first().unwrap().1 - 0.0).abs() < 0.001, "начинается в p1");
        assert!((pts.last().unwrap().0 - 30.0).abs() < 0.001, "заканчивается в p2");
    }

    #[test]
    fn stamp_curve_passes_through_control_points() {
        let mut hits: Vec<(f32, f32)> = Vec::new();
        stamp_curve(
            |x, y| hits.push((x, y)),
            (0.0, 0.0), (10.0, 10.0), (10.0, 30.0), (40.0, 30.0),
            0.5,
        );
        let near = |p: (f32, f32)| hits.iter().any(|h| (h.0 - p.0).hypot(h.1 - p.1) < 0.6);
        assert!(near((10.0, 10.0)), "дуга не проходит через начало: {:?}", hits);
        assert!(near((10.0, 30.0)), "дуга не проходит через конец: {:?}", hits);
    }

    #[test]
    fn brush_shapes_paint_different_footprints() {
        let (w, h) = (40, 40);
        let color = [0, 0, 0, 255];
        // Круг: углы пусты, центр закрашен.
        let mut round = empty(w, h);
        stamp_shape(&mut round, w, h, 20.0, 20.0, 10.0, 1.0, color, 1.0, Shape::Round);
        assert!(round.get(w, 20, 20)[3] > 200, "центр круга закрашен");
        assert_eq!(round.get(w, 12, 12)[3], 0, "угол круга пуст");

        // Квадрат: угол квадрата закрашен.
        let mut sq = empty(w, h);
        stamp_shape(&mut sq, w, h, 20.0, 20.0, 10.0, 1.0, color, 1.0, Shape::Square);
        assert!(sq.get(w, 12, 12)[3] > 200, "угол квадрата закрашен: {:?}", sq.get(w, 12, 12));

        // Эллипс: шире, чем круг, по горизонтали.
        let mut el = empty(w, h);
        stamp_shape(&mut el, w, h, 20.0, 20.0, 10.0, 1.0, color, 1.0, Shape::Ellipse);
        assert!(el.get(w, 20, 20)[3] > 200, "центр эллипса закрашен");
        assert!(
            el.get(w, 9, 20)[3] > 0 && round.get(w, 9, 20)[3] == 0,
            "эллипс тянется вбок дальше круга"
        );
    }

    #[test]
    fn blit_transformed_scales_and_offsets() {
        let (w, h) = (24, 24);
        let mut l = empty(w, h);
        let mut data = rect_data(2, 2);
        // Один красный пиксель слева сверху, остальное прозрачное.
        data.pixels[0..4].copy_from_slice(&[255, 0, 0, 255]);
        // Растягиваем 2×2 в прямоугольник 4×4 со сдвигом (10, 10).
        let pts = [(10.0, 10.0), (14.0, 10.0), (14.0, 14.0), (10.0, 14.0)];
        blit_transformed(&mut l, w, h, &data, pts);
        let px = l.get(w, 11, 11);
        // Четверть площади красная: цвет без тёмного канта, альфа ~64.
        assert!(px[0] > 240 && px[1] == 0 && px[3] > 40, "левый верхний угол красный: {:?}", px);
        let far = l.get(w, 13, 13);
        assert!(far[0] < 60, "правый нижний угол — прозрачная часть: {:?}", far);
        assert_eq!(l.get(w, 5, 5)[3], 0, "вне результата пусто");
    }

    #[test]
    fn blit_transformed_rotates_content() {
        let (w, h) = (32, 32);
        let mut l = empty(w, h);
        let mut data = rect_data(2, 2);
        data.pixels[0..4].copy_from_slice(&[255, 0, 0, 255]);
        // Поворот на 90° по часовой вокруг центра (16, 16): красный угол
        // исходника (левый верхний) должен оказаться вверху результата.
        let pts = [
            (17.0, 15.0),
            (17.0, 17.0),
            (15.0, 17.0),
            (15.0, 15.0),
        ];
        blit_transformed(&mut l, w, h, &data, pts);
        let top = l.get(w, 16, 15);
        assert!(top[0] > 150 && top[3] > 20, "красный оказался сверху: {:?}", top);
        let bottom = l.get(w, 16, 16);
        assert!(bottom[3] < 60, "внизу должно быть пусто: {:?}", bottom);
    }

    #[test]
    fn sel_rect_normalizes_drag_direction() {
        // тянем справа налево снизу вверх — рамка всё равно с началом в левом верху
        let s = SelRect::new(30.0, 40.0, 10.0, 20.0);
        assert_eq!((s.x, s.y, s.w, s.h), (10.0, 20.0, 20.0, 20.0));
        assert!(s.contains((15.0, 25.0)));
        assert!(!s.contains((5.0, 25.0)), "слева от рамки — мимо");
        assert!(!s.contains((15.0, 45.0)), "снизу — мимо (нижняя граница исключена)");
    }

    #[test]
    fn stamp_paints_inside_and_nothing_outside() {
        let (w, h) = (40, 40);
        let mut l = empty(w, h);
        stamp(&mut l, w, h, 20.0, 20.0, 6.0, 1.0, [255, 0, 0, 255], 1.0);
        assert_eq!(l.get(w, 20, 20), [255, 0, 0, 255]);
        assert_eq!(l.get(w, 20, 12)[3], 0, "пиксель вне круга не тронут");
        assert_eq!(l.get(w, 5, 5)[3], 0);
    }

    #[test]
    fn soft_edge_is_transparent_in_center_of_falloff() {
        let (w, h) = (40, 40);
        let mut l = empty(w, h);
        // hardness 0.0: в центре покрытие почти полное, к краю падает до нуля
        stamp(&mut l, w, h, 20.0, 20.0, 8.0, 0.0, [0, 0, 0, 255], 1.0);
        assert!(l.get(w, 20, 20)[3] > 200, "центр мягкой кисти почти непрозрачен");
        assert!(l.get(w, 27, 20)[3] < 120, "край мягкой кисти прозрачен");
    }

    #[test]
    fn brush_line_is_continuous() {
        let (w, h) = (60, 20);
        let mut l = empty(w, h);
        brush_line(&mut l, w, h, 5.0, 10.0, 55.0, 10.0, 2.0, 1.0, [0, 0, 0, 255], 1.0);
        for x in 5..=55 {
            assert!(l.get(w, x, 10)[3] > 0, "разрыв в x={x}");
        }
    }

    #[test]
    fn ellipse_outline_is_closed() {
        let (w, h) = (80, 80);
        let mut l = empty(w, h);
        ellipse(&mut l, w, h, 10.0, 10.0, 70.0, 70.0, false, 2.0, 1.0, [0, 0, 0, 255], 1.0);
        // углы пустые, середина сторон закрашена
        assert_eq!(l.get(w, 10, 10)[3], 0);
        assert!(l.get(w, 40, 10)[3] > 0, "верх не нарисован");
        assert!(l.get(w, 40, 70)[3] > 0, "низ не нарисован");
        assert!(l.get(w, 10, 40)[3] > 0, "лево не нарисовано");
        assert!(l.get(w, 70, 40)[3] > 0, "право не нарисовано");
        assert_eq!(l.get(w, 40, 40)[3], 0, "центр не должен быть закрашен");
    }

    #[test]
    fn flood_fill_respects_a_wall() {
        let (w, h) = (10, 10);
        let mut l = empty(w, h);
        l.fill([255, 255, 255, 255]);
        // вертикальная стена посередине
        for y in 0..h {
            l.set(w, 5, y, [0, 0, 0, 255]);
        }
        flood_fill(&mut l, w, h, 1, 1, [255, 0, 0, 255], 1.0, 0, true);
        assert_eq!(l.get(w, 1, 1), [255, 0, 0, 255], "левая половина не залита");
        assert_eq!(l.get(w, 8, 1), [255, 255, 255, 255], "заливка прошла сквозь стену");
        assert_eq!(l.get(w, 5, 1), [0, 0, 0, 255], "стена повреждена");
    }

    #[test]
    fn gradient_goes_from_first_to_second_color() {
        let (w, h) = (80, 40);
        let mut l = empty(w, h);
        // резкий градиент: мягкость 0 -> цвета должны различаться вдоль оси
        gradient(&mut l, w, h, 5.0, 20.0, 75.0, 20.0, [255, 0, 0, 255], [0, 0, 255, 255], 1.0, 0.0);
        // смотрим строго на ось: y = 20.5 в координатах центров пикселей
        let left = l.get(w, 8, 20);
        let right = l.get(w, 72, 20);
        assert!(left[0] > left[2], "слева должен быть красный: {left:?}");
        assert!(right[2] > right[0], "справа должен быть синий: {right:?}");
        // вне полосы градиента пусто
        assert_eq!(l.get(w, 40, 2)[3], 0, "выше оси должно быть пусто: {:?}", l.get(w, 40, 2));
        assert_eq!(l.get(w, 40, 38)[3], 0, "ниже оси должно быть пусто");
    }

    #[test]
    fn gradient_softness_widens_the_band() {
        let (w, h) = (80, 80);
        let mut hard = empty(w, h);
        let mut soft = empty(w, h);
        gradient(&mut hard, w, h, 40.0, 20.0, 40.0, 60.0, [0, 0, 0, 255], [255, 255, 255, 255], 1.0, 0.0);
        gradient(&mut soft, w, h, 40.0, 20.0, 40.0, 60.0, [0, 0, 0, 255], [255, 255, 255, 255], 1.0, 1.0);
        // жёсткий — узкая полоса в несколько пикселей, мягкий — широкая
        assert!(soft.get(w, 40, 30)[3] > 0, "мягкий градиент должен быть широким");
        assert_eq!(hard.get(w, 40, 12)[3], 0, "жёсткий градиент не должен доходить до y=12");
        assert!(soft.get(w, 40, 12)[3] > 0, "мягкий градиент должен доходить до y=12");
    }

    #[test]
    fn non_contiguous_fill_replaces_everywhere() {
        let (w, h) = (10, 10);
        let mut l = empty(w, h);
        l.fill([255, 255, 255, 255]);
        for y in 0..h {
            l.set(w, 5, y, [0, 0, 0, 255]);
        }
        flood_fill(&mut l, w, h, 1, 1, [0, 255, 0, 255], 1.0, 0, false);
        assert_eq!(l.get(w, 1, 1), [0, 255, 0, 255]);
        assert_eq!(l.get(w, 8, 1), [0, 255, 0, 255], "вторая область не заменена");
    }

    #[test]
    fn polygon_mask_fills_inside_and_skips_outside() {
        let (w, h) = (20, 20);
        // Треугольник: вершины (2,2), (18,2), (2,18).
        let m = polygon_mask(w, h, &[(2.0, 2.0), (18.0, 2.0), (2.0, 18.0)]);
        assert_eq!(m[4 * w + 4], 255, "внутри треугольника выбрано");
        assert_eq!(m[16 * w + 16], 0, "вне треугольника пусто");
        assert_eq!(m[10 * w + 1], 0, "слева от гипотенузы пусто");
        // Наклонная сторона (x + y = 20) режется в полпикселя, без лесенки.
        assert_eq!(m[10 * w + 8], 255, "заведомо внутри");
        let edge = m[10 * w + 9];
        assert!(edge > 0 && edge < 255, "край сглажен: {}", edge);
    }

    #[test]
    fn polygon_mask_needs_three_points() {
        let m = polygon_mask(8, 8, &[(1.0, 1.0), (5.0, 5.0)]);
        assert!(m.iter().all(|v| *v == 0), "две вершины — не фигура");
    }

    #[test]
    fn polygon_mask_handles_concave_shape() {
        // Вогнутый многоугольник: срез сверху справа — снаружи фигуры.
        let pts = [(2.0, 2.0), (18.0, 4.0), (10.0, 10.0), (2.0, 18.0)];
        let m = polygon_mask(20, 20, &pts);
        assert_eq!(m[6 * 20 + 4], 255, "левая часть внутри");
        assert_eq!(m[6 * 20 + 13], 255, "правый карман внутри");
        assert_eq!(m[3 * 20 + 17], 0, "над срезом снаружи");
        assert_eq!(m[14 * 20 + 6], 0, "справа от вогнутой стороны снаружи");
    }
}
