//! Полотно — 32-битный буфер пикселей в формате 0x00BBGGRR
//! (тот же порядок байт, что у Win32 COLORREF, поэтому GDI-цвета можно писать напрямую).

use std::collections::VecDeque;

/// Белый цвет.
pub const WHITE: u32 = 0x00FF_FFFF;

pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

#[derive(Clone)]
pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub buf: Vec<u32>,
}

impl Canvas {
    pub fn new(width: i32, height: i32) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        Self {
            width: w,
            height: h,
            buf: vec![WHITE; (w as usize) * (h as usize)],
        }
    }

    #[inline]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    #[inline]
    pub fn px(&self, x: i32, y: i32) -> u32 {
        if self.contains(x, y) {
            self.buf[(y as usize) * (self.width as usize) + (x as usize)]
        } else {
            WHITE
        }
    }

    #[inline]
    pub fn set_px(&mut self, x: i32, y: i32, c: u32) {
        if self.contains(x, y) {
            let i = (y as usize) * (self.width as usize) + (x as usize);
            self.buf[i] = c;
        }
    }

    pub fn clear(&mut self, c: u32) {
        self.buf.fill(c);
    }

    /// Снимок состояния (для undo/preview).
    pub fn snapshot(&self) -> Vec<u32> {
        self.buf.clone()
    }

    pub fn restore(&mut self, snap: &[u32]) {
        if snap.len() == self.buf.len() {
            self.buf.copy_from_slice(snap);
        }
    }

    /// Изменение размера с сохранением содержимого (новые пиксели — белые).
    pub fn resize_preserve(&mut self, width: i32, height: i32) {
        let w = width.max(1);
        let h = height.max(1);
        if w == self.width && h == self.height {
            return;
        }
        let mut nb = vec![WHITE; (w as usize) * (h as usize)];
        let cw = w.min(self.width) as usize;
        let ch = h.min(self.height) as usize;
        for y in 0..ch {
            let src = &self.buf[y * (self.width as usize)..y * (self.width as usize) + cw];
            let dst = &mut nb[y * (w as usize)..y * (w as usize) + cw];
            dst.copy_from_slice(src);
        }
        self.width = w;
        self.height = h;
        self.buf = nb;
    }

    /// Залитый диск (r = 0 — одиночный пиксель).
    pub fn disc(&mut self, cx: i32, cy: i32, r: i32, c: u32) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.set_px(cx + dx, cy + dy, c);
                }
            }
        }
    }

    /// Толстая линия Брезенхэма (r — радиус кисти).
    pub fn thick_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, r: i32, c: u32) {
        let mut x = x0;
        let mut y = y0;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.disc(x, y, r, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Прямоугольник: залитый или контурный (толщина контура — r).
    pub fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, filled: bool, r: i32, c: u32) {
        let (xa, xb) = (x0.min(x1), x0.max(x1));
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        if filled {
            for y in ya..=yb {
                self.thick_line(xa, y, xb, y, 0, c);
            }
        } else {
            self.thick_line(xa, ya, xb, ya, r, c);
            self.thick_line(xa, yb, xb, yb, r, c);
            self.thick_line(xa, ya, xa, yb, r, c);
            self.thick_line(xb, ya, xb, yb, r, c);
        }
    }

    /// Эллипс: залитый или контурный (контур — толстыми линиями между
    /// соседними строками, иначе на «пологих» концах получается пунктир).
    pub fn ellipse(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, filled: bool, r: i32, c: u32) {
        let a = (x1 - x0).abs() as f64 / 2.0;
        let b = (y1 - y0).abs() as f64 / 2.0;
        if a < 0.5 || b < 0.5 {
            return;
        }
        let cx = (x0 + x1) as f64 / 2.0;
        let cy = (y0 + y1) as f64 / 2.0;
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        // Предыдущие точки контура: (левая, правая).
        let mut prev: Option<(i32, i32)> = None;
        for yy in ya..=yb {
            let dy = (yy as f64 - cy) / b;
            let t = 1.0 - dy * dy;
            if t < 0.0 {
                continue;
            }
            let xr = (a * t.sqrt()).round() as i32;
            let xl = (cx - xr as f64).round() as i32;
            let xright = (cx + xr as f64).round() as i32;
            if filled {
                self.thick_line(xl, yy, xright, yy, 0, c);
            } else {
                match prev {
                    Some((pl, pr)) => {
                        // dy == 0 не бывает, yy всегда растёт, так что yy - 1 — валидная строка.
                        self.thick_line(pl, yy - 1, xl, yy, r, c);
                        self.thick_line(pr, yy - 1, xright, yy, r, c);
                    }
                    None => {
                        self.disc(xl, yy, r, c);
                        self.disc(xright, yy, r, c);
                    }
                }
                prev = Some((xl, xright));
            }
        }
    }

    /// Заливка области (BFS по пикселям стартового цвета).
    pub fn flood_fill(&mut self, sx: i32, sy: i32, c: u32) {
        if !self.contains(sx, sy) {
            return;
        }
        let target = self.px(sx, sy);
        if target == c {
            return;
        }
        let w = self.width;
        let h = self.height;
        let mut q: VecDeque<u32> = VecDeque::new();
        q.push_back(((sx as u32) << 16) | (sy as u32));
        while let Some(p) = q.pop_front() {
            let x = (p >> 16) as i32;
            let y = (p & 0xFFFF) as i32;
            if !self.contains(x, y) || self.px(x, y) != target {
                continue;
            }
            self.buf[(y as usize) * (w as usize) + (x as usize)] = c;
            if x + 1 < w {
                q.push_back((((x + 1) as u32) << 16) | (y as u32));
            }
            if x > 0 {
                q.push_back((((x - 1) as u32) << 16) | (y as u32));
            }
            if y + 1 < h {
                q.push_back(((x as u32) << 16) | ((y + 1) as u32));
            }
            if y > 0 {
                q.push_back(((x as u32) << 16) | ((y - 1) as u32));
            }
        }
    }

    pub fn save_png(&self, path: &str) -> Result<(), String> {
        let w = self.width as u32;
        let h = self.height as u32;
        let mut img = image::RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let c = self.px(x as i32, y as i32);
                img.put_pixel(x, y, image::Rgb([(c >> 16) as u8, (c >> 8) as u8, c as u8]));
            }
        }
        img.save(path).map_err(|e| e.to_string())
    }

    pub fn load_png(&mut self, path: &str) -> Result<(), String> {
        let img = image::open(path).map_err(|e| e.to_string())?.to_rgb8();
        let w = img.width() as i32;
        let h = img.height() as i32;
        let mut buf = vec![WHITE; (w as usize) * (h as usize)];
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x as u32, y as u32);
                buf[(y as usize) * (w as usize) + (x as usize)] = rgb(p[0], p[1], p[2]);
            }
        }
        self.width = w;
        self.height = h;
        self.buf = buf;
        Ok(())
    }
}