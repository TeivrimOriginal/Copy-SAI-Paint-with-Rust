//! Растеризация: мягкая кисть, линии, фигуры, заливка с допуском.
//! Всё рисуется в пиксельный буфер слоя (RGBA8) методом source-over.

use crate::doc::Layer;

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
