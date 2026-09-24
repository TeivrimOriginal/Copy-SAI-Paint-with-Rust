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
    let r = radius.max(0.5);
    let hardness = hardness.clamp(0.0, 1.0);
    let inner = r * hardness;
    let x0 = ((cx - r).floor().max(0.0)) as i64;
    let x1 = ((cx + r).ceil().min((w - 1) as f32)) as i64;
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
            let d = (dx * dx + dy * dy).sqrt();
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
}
