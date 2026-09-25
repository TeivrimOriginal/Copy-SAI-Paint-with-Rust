//! Формат проекта `.tpaint` — все слои вместе с настройками, без потерь.
//!
//! Это то, что обычный PNG не умеет: картинка сохраняется «как есть» —
//! слои, их порядок, видимость, непрозрачность и режимы наложения.
//!
//! Устройство файла (всё little-endian):
//!
//! ```text
//! "TPAINT01"            8 байт — сигнатура и версия
//! u32 version          версия формата (сейчас 3)
//! u32 width, u32 height размер холста
//! u32 layers            количество слоёв
//! u32 active            индекс активного слоя
//! для каждого слоя:
//!   u32 name_len, байты имени (UTF-8)
//!   u8  visible, u8 locked
//!   f32 opacity
//!   u32 blend            индекс в BlendMode::ALL
//!   u32 png_len, байты   пиксели слоя, закодированные как PNG
//!   с версии 2:
//!     u8 has_mask        есть ли маска
//!     если есть: u8 mask_on, u32 png_len, байты — маска как PNG (серый)
//!   с версии 3:
//!     u32 group_len, байты   имя папки слоя (0 — слой вне папки)
//!     u8  group_open         папка раскрыта в панели
//! ```

use crate::doc::{BlendMode, Document, Layer, LayerMeta};

/// Сигнатура файла проекта: имя и версия формата.
pub const MAGIC: &[u8; 8] = b"TPAINT01";
/// Расширение файла проекта (без точки).
pub const EXT: &str = "tpaint";
/// То же с точкой — для подстановки в имя файла.
pub const EXT_DOT: &str = ".tpaint";
/// Текущая версия формата.
pub const VERSION: u32 = 3;

/// Записать документ в файл проекта.
pub fn save(doc: &Document, path: &str) -> Result<(), String> {
    let mut w = Writer::new();
    w.bytes(MAGIC);
    w.u32(VERSION);
    w.u32(doc.width as u32);
    w.u32(doc.height as u32);
    w.u32(doc.layers.len() as u32);
    w.u32(doc.active as u32);
    for layer in &doc.layers {
        let name = layer.meta.name.as_bytes();
        w.u32(name.len() as u32);
        w.bytes(name);
        w.u8(layer.meta.visible as u8);
        w.u8(layer.meta.locked as u8);
        w.f32(layer.meta.opacity);
        w.u32(
            BlendMode::ALL
                .iter()
                .position(|b| *b == layer.meta.blend)
                .unwrap_or(0) as u32,
        );
        // Пиксели слоя — отдельным PNG: сжатие есть, формат знакомый.
        let img = image::RgbaImage::from_raw(doc.width as u32, doc.height as u32, layer.pixels.clone())
            .ok_or_else(|| "неверный размер пикселей слоя".to_string())?;
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        w.u32(png.len() as u32);
        w.bytes(&png);
        // Маска слоя (с версии 2): серый PNG того же размера.
        match &layer.mask {
            Some(mask) => {
                let img = image::GrayImage::from_raw(doc.width as u32, doc.height as u32, mask.clone())
                    .ok_or_else(|| "неверный размер маски слоя".to_string())?;
                let mut png = Vec::new();
                img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                w.u8(1);
                w.u8(layer.mask_on as u8);
                w.u32(png.len() as u32);
                w.bytes(&png);
            }
            None => w.u8(0),
        }
        // С версии 3: имя папки (u32 + UTF-8) и признак раскрытия.
        if VERSION >= 3 {
            match &layer.meta.group {
                Some(g) => {
                    let bytes = g.as_bytes();
                    w.u32(bytes.len() as u32);
                    w.bytes(bytes);
                }
                None => w.u32(0),
            }
            w.u8(layer.meta.group_open as u8);
        }
    }
    std::fs::write(path, &w.buf).map_err(|e| e.to_string())
}

/// Прочитать документ из файла проекта.
pub fn load(path: &str) -> Result<Document, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut r = Reader { buf: &data, pos: 0 };
    let magic = r.take(8)?;
    if magic != MAGIC {
        return Err("это не файл проекта Tpaint".to_string());
    }
    let version = r.u32()?;
    if version > VERSION {
        return Err(format!("файл сохранён более новой версией Tpaint ({} > {})", version, VERSION));
    }
    let width = r.u32()? as usize;
    let height = r.u32()? as usize;
    let count = r.u32()? as usize;
    let active = r.u32()? as usize;
    if width == 0 || height == 0 || width > 32_768 || height > 32_768 {
        return Err(format!("неверный размер холста {}×{}", width, height));
    }
    if count == 0 || count > 512 {
        return Err(format!("неверное число слоёв: {}", count));
    }
    let mut layers = Vec::with_capacity(count);
    for _ in 0..count {
        let nlen = r.u32()? as usize;
        if nlen > 4096 {
            return Err("повреждено имя слоя".to_string());
        }
        let name = String::from_utf8(r.take(nlen)?.to_vec()).map_err(|_| "имя слоя не в UTF-8".to_string())?;
        let visible = r.u8()? != 0;
        let locked = r.u8()? != 0;
        let opacity = r.f32()?.clamp(0.0, 1.0);
        let blend = BlendMode::ALL.get(r.u32()? as usize).copied().unwrap_or(BlendMode::Normal);
        let plen = r.u32()? as usize;
        let png = r.take(plen)?;
        let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .map_err(|e| format!("повреждён слой «{}»: {}", name, e))?
            .to_rgba8();
        if img.width() as usize != width || img.height() as usize != height {
            return Err(format!("размер слоя «{}» не совпадает с холстом", name));
        }
        // Маска слоя (появилась в версии 2).
        let mut mask = None;
        let mut mask_on = true;
        if version >= 2 && r.u8()? != 0 {
            mask_on = r.u8()? != 0;
            let mlen = r.u32()? as usize;
            let mpng = r.take(mlen)?;
            let mimg = image::load_from_memory_with_format(mpng, image::ImageFormat::Png)
                .map_err(|e| format!("повреждена маска слоя «{}»: {}", name, e))?
                .to_luma8();
            if mimg.width() as usize != width || mimg.height() as usize != height {
                return Err(format!("размер маски слоя «{}» не совпадает с холстом", name));
            }
            mask = Some(mimg.into_raw());
        }
        // Папка слоя (с версии 3) — читается после маски, как пишется.
        let mut group = None;
        let mut group_open = true;
        if version >= 3 {
            let glen = r.u32()? as usize;
            if glen > 256 {
                return Err("повреждено имя папки".to_string());
            }
            if glen > 0 {
                let g = r.take(glen)?;
                group = Some(String::from_utf8(g.to_vec()).map_err(|_| "имя папки не в UTF-8".to_string())?);
            }
            group_open = r.u8()? != 0;
        }
        layers.push(Layer {
            meta: LayerMeta { name, visible, locked, opacity, blend, group, group_open },
            pixels: img.into_raw(),
            mask,
            mask_on,
        });
    }
    let mut doc = Document::empty();
    doc.width = width;
    doc.height = height;
    doc.layers = layers;
    doc.active = active.min(doc.layers.len() - 1);
    doc.composite = vec![0; width * height * 4];
    doc.touch();
    Ok(doc)
}

/// Пишет значения в буфер по одному, little-endian.
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }
}

/// Читает значения из буфера с проверкой границ.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.pos + n > self.buf.len() {
            return Err("файл проекта обрезан".to_string());
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("tpaint_test_{}_{}", std::process::id(), name));
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn project_round_trip_keeps_layers_and_pixels() {
        let mut doc = Document::new(24, 16);
        doc.layers[0].meta.name = "Фон".to_string();
        doc.layers[0].meta.opacity = 1.0;
        for x in 0..24 {
            doc.layers[0].pixels[(x * 4) as usize] = 200;
            doc.layers[0].pixels[(x * 4 + 1) as usize] = 100;
        }
        let mut top = Layer::new(24, 16, "Верх");
        top.meta.opacity = 0.42;
        top.meta.blend = BlendMode::Multiply;
        top.meta.visible = false;
        top.pixels[4] = 7;
        doc.layers.push(top);
        doc.active = 1;
        doc.touch();

        let path = tmp_path("roundtrip.tpaint");
        save(&doc, &path).expect("сохранили");
        let back = load(&path).expect("прочитали");
        let _ = std::fs::remove_file(&path);

        assert_eq!((back.width, back.height), (24, 16));
        assert_eq!(back.layers.len(), 2);
        assert_eq!(back.active, 1);
        assert_eq!(back.layers[0].meta.name, "Фон");
        assert_eq!(back.layers[0].pixels[8], 200, "пиксели фона сохранились (R)");
        assert_eq!(back.layers[0].pixels[9], 100, "пиксели фона сохранились (G)");
        assert_eq!(back.layers[0].pixels[11], 0, "прозрачность фона сохранилась");
        assert_eq!(back.layers[1].meta.name, "Верх");
        assert!((back.layers[1].meta.opacity - 0.42).abs() < 0.001);
        assert_eq!(back.layers[1].meta.blend, BlendMode::Multiply);
        assert!(!back.layers[1].meta.visible, "видимость сохранилась");
        assert_eq!(back.layers[1].pixels[4], 7, "пиксель верхнего слоя сохранён");
    }

    #[test]
    fn foreign_file_is_rejected() {
        let path = tmp_path("foreign.bin");
        std::fs::write(&path, b"not a tpaint file at all").unwrap();
        let err = match load(&path) {
            Ok(_) => panic!("чужой файл не должен открываться"),
            Err(e) => e,
        };
        let _ = std::fs::remove_file(&path);
        assert!(err.contains("не файл проекта"), "непонятная ошибка: {}", err);
    }

    #[test]
    fn truncated_file_is_rejected() {
        let mut doc = Document::new(8, 8);
        doc.layers[0].meta.name = "Фон".to_string();
        let path = tmp_path("cut.tpaint");
        save(&doc, &path).expect("сохранили");
        let mut data = std::fs::read(&path).unwrap();
        data.truncate(data.len() / 2);
        std::fs::write(&path, &data).unwrap();
        let err = match load(&path) {
            Ok(_) => panic!("обрезанный файл не должен открываться"),
            Err(e) => e,
        };
        let _ = std::fs::remove_file(&path);
        assert!(!err.is_empty(), "повреждённый файл должен отвергаться");
    }

    #[test]
    fn transparent_layer_stays_transparent() {
        // Пустой слой после round trip должен остаться полностью прозрачным.
        let mut doc = Document::new(8, 8);
        doc.layers[0].pixels.fill(0);
        let path = tmp_path("empty.tpaint");
        save(&doc, &path).expect("сохранили");
        let back = load(&path).expect("прочитали");
        let _ = std::fs::remove_file(&path);
        assert!(back.layers[0].pixels.iter().all(|v| *v == 0));
    }
}
