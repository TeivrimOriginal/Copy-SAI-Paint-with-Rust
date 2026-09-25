//! Цветовой выбор: SV-квад, полосы тона и прозрачности, поля RGB и HEX.
//!
//! Состояние цвета живёт в приложении как RGBA, а HSV считается на лету —
//! так виджеты квадрата и полосы не расходятся с реальным цветом кисти.

use crate::ui::{rgba, theme, Rect, Ui};

const ID_SV: u32 = 900_001;
const ID_HUE: u32 = 900_002;
const ID_ALPHA: u32 = 900_003;

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8]
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let mut h = 0.0;
    if d > 1e-6 {
        h = if mx == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if mx == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } / 6.0;
    }
    let s = if mx > 1e-6 { d / mx } else { 0.0 };
    (h, s, mx)
}

pub fn to_hex(c: [u8; 4]) -> String {
    format!("{:02X}{:02X}{:02X}{:02X}", c[0], c[1], c[2], c[3])
}

pub fn from_hex(s: &str) -> Option<[u8; 4]> {
    let t: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    match t.len() {
        6 => u32::from_str_radix(&t, 16).ok().map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]),
        8 => u32::from_str_radix(&t, 16)
            .ok()
            .map(|v| [(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8]),
        _ => None,
    }
}

/// Палитра по умолчанию (тёмная и светлая).
pub const PALETTE: [[u8; 4]; 20] = [
    [0, 0, 0, 255],
    [64, 64, 64, 255],
    [128, 128, 128, 255],
    [192, 192, 192, 255],
    [255, 255, 255, 255],
    [255, 0, 0, 255],
    [255, 128, 0, 255],
    [255, 220, 0, 255],
    [140, 220, 60, 255],
    [40, 190, 120, 255],
    [0, 160, 220, 255],
    [40, 110, 230, 255],
    [90, 80, 230, 255],
    [170, 70, 220, 255],
    [235, 80, 170, 255],
    [120, 60, 40, 255],
    [190, 120, 70, 255],
    [240, 220, 180, 255],
    [90, 30, 30, 255],
    [30, 40, 90, 255],
];

/// Рисует полноценный выбор цвета в прямоугольнике `r`.
/// Возвращает true, если цвет изменился.
pub fn color_picker(ui: &mut Ui, r: Rect, color: &mut [u8; 4], hex_buf: &mut String) -> bool {
    let mut changed = false;
    let (h, s, v) = rgb_to_hsv(color[0], color[1], color[2]);
    let a = color[3] as f32 / 255.0;

    let bar_w = 16.0;
    let gap = 6.0;
    let sv_w_full = (r[2] - r[0] - bar_w * 2.0 - gap * 2.0).max(40.0);
    // Под палитру и поля R/G/B/HEX резервируем место: в невысоком окне
    // квад S/V уменьшается, чтобы колорпикер не обрезался снизу.
    const RESERVE: f32 = 44.0 + 30.0;
    let avail = (r[3] - r[1] - RESERVE).max(60.0);
    let sv_w = sv_w_full.min(avail);
    let sv_r = [r[0], r[1], r[0] + sv_w, r[1] + sv_w];

    // --- SV-квад: сетка цветов ---
    let steps = 24;
    let cw = sv_r[2] - sv_r[0];
    let ch = sv_r[3] - sv_r[1];
    for iy in 0..steps {
        for ix in 0..steps {
            let sx = (ix as f32 + 0.5) / steps as f32;
            let sy = (iy as f32 + 0.5) / steps as f32;
            let c = hsv_to_rgb(h, sx, 1.0 - sy);
            ui.quad(
                [sv_r[0] + cx(ix, steps, cw), sv_r[1] + cy(iy, steps, ch), sv_r[0] + cx(ix + 1, steps, cw), sv_r[1] + cy(iy + 1, steps, ch)],
                rgba([c[0], c[1], c[2], 255]),
            );
        }
    }
    ui.frame(sv_r, theme::BORDER, 1.0);
    // Маркер
    let mx = sv_r[0] + s * (sv_r[2] - sv_r[0]);
    let my = sv_r[1] + (1.0 - v) * (sv_r[3] - sv_r[1]);
    ui.ring(mx, my, 5.0, [1.0, 1.0, 1.0, 0.95], 1.5);
    ui.ring(mx, my, 5.0, [0.0, 0.0, 0.0, 0.55], 1.0);
    if let Some((nx, ny)) = ui.drag_with(ID_SV, sv_r) {
        let c = hsv_to_rgb(h, nx, 1.0 - ny);
        color[0] = c[0];
        color[1] = c[1];
        color[2] = c[2];
        changed = true;
    }

    // --- полоса тона ---
    let hue_r = [sv_r[2] + gap, sv_r[1], sv_r[2] + gap + bar_w, sv_r[3]];
    let hsteps = 24;
    for i in 0..hsteps {
        let c = hsv_to_rgb(i as f32 / hsteps as f32, 1.0, 1.0);
        ui.quad(
            [hue_r[0], hue_r[1] + cy(i, hsteps, hue_r[3] - hue_r[1]), hue_r[2], hue_r[1] + cy(i + 1, hsteps, hue_r[3] - hue_r[1])],
            rgba([c[0], c[1], c[2], 255]),
        );
    }
    ui.frame(hue_r, theme::BORDER, 1.0);
    let hy = hue_r[1] + h * (hue_r[3] - hue_r[1]);
    ui.quad([hue_r[0] - 1.0, hy - 1.5, hue_r[2] + 1.0, hy + 1.5], [1.0, 1.0, 1.0, 0.9]);
    if let Some((_, ny)) = ui.drag_with(ID_HUE, hue_r) {
        let c = hsv_to_rgb(ny, s.max(0.0001), v.max(0.0001));
        color[0] = c[0];
        color[1] = c[1];
        color[2] = c[2];
        changed = true;
    }

    // --- полоса прозрачности ---
    let a_r = [hue_r[2] + gap, sv_r[1], hue_r[2] + gap + bar_w, sv_r[3]];
    ui.checker(a_r, 6.0);
    let asteps = 20;
    for i in 0..asteps {
        let al = (i as f32 + 0.5) / asteps as f32;
        ui.quad(
            [a_r[0], a_r[1] + cy(i, asteps, a_r[3] - a_r[1]), a_r[2], a_r[1] + cy(i + 1, asteps, a_r[3] - a_r[1])],
            rgba([color[0], color[1], color[2], (al * 255.0) as u8]),
        );
    }
    ui.frame(a_r, theme::BORDER, 1.0);
    let ay = a_r[1] + a * (a_r[3] - a_r[1]);
    ui.quad([a_r[0] - 1.0, ay - 1.5, a_r[2] + 1.0, ay + 1.5], [1.0, 1.0, 1.0, 0.9]);
    if let Some((_, ny)) = ui.drag_with(ID_ALPHA, a_r) {
        color[3] = (ny * 255.0).round() as u8;
        changed = true;
    }

    // --- палитра ---
    // Ширина палитры считается по всей панели, а не по квадрату S/V:
    // так она остаётся в десяти колонках, даже если квад уменьшился.
    let pal_y = sv_r[3] + 8.0;
    let cell = ((sv_w_full + 2.0) / 10.0 - 2.0).clamp(10.0, 18.0);
    for (i, c) in PALETTE.iter().enumerate() {
        let px = r[0] + (i % 10) as f32 * (cell + 2.0);
        let py = pal_y + (i / 10) as f32 * (cell + 2.0);
        let sw = [px, py, px + cell, py + cell];
        if ui.list_row(sw, false, ui.mouse.0 >= sw[0] && ui.mouse.0 < sw[2] && ui.mouse.1 >= sw[1] && ui.mouse.1 < sw[3]) {
            *color = *c;
            changed = true;
        }
        ui.swatch(sw, *c);
    }

    // --- поля ввода: R G B и HEX ---
    // R/G/B занимают левую часть строки, HEX — всю оставшуюся ширину,
    // чтобы поля не наезжали друг на друга в узкой панели.
    let fy = pal_y + cell * 2.0 + 10.0;
    let fw = ((sv_w_full * 0.62) - 8.0) / 3.0;
    let fh = 20.0;
    for (i, (lbl, val)) in [("R", color[0]), ("G", color[1]), ("B", color[2])]
        .iter()
        .enumerate()
    {
        let fr = [r[0] + i as f32 * (fw + 4.0), fy, r[0] + i as f32 * (fw + 4.0) + fw, fy + fh];
        let mut v = *val as f32;
        if ui.value_field(fr, lbl, &mut v, 0.0, 255.0, 0.5) {
            color[i] = v.round() as u8;
            changed = true;
        }
    }

    let hex_r = [r[0] + 3.0 * (fw + 4.0), fy, r[2], fy + fh];
    let before = hex_buf.clone();
    ui.text_field(hex_r, hex_buf, |c| c.is_ascii_hexdigit());
    // Пока поле в фокусе — не форматируем, иначе невозможно печатать.
    if !ui.focus.is_some() {
        *hex_buf = to_hex(*color);
    }
    let _ = before;
    if !ui.is_dragging(ID_SV) {
        if let Some(parsed) = from_hex(hex_buf) {
            if parsed != *color {
                *color = parsed;
                changed = true;
            }
        }
    }
    changed
}

fn cx(i: i32, steps: i32, w: f32) -> f32 {
    i as f32 * w / steps as f32
}

fn cy(i: i32, steps: i32, h: f32) -> f32 {
    i as f32 * h / steps as f32
}
