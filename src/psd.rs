//! Чтение и запись PSD (Photoshop Document) — плоского, без слоёв.
//!
//! Формат примитивный: заголовок, блок цветового режима, блок ресурсов,
//! пустой блок слоёв и картинка в конце файла. Данные каналов идут плоско
//! (сначала все красные, потом зелёные и так далее) и сжимаются либо без
//! сжатия, либо PackBits (RLE) — так их пишет сам Photoshop.
//!
//! Слои и маски не читаем: импортируется итоговая картинка документа, ровно
//! то, что Photoshop показывает при открытии. Экспорт кладёт композицию всех
//! слоёв одним плоским изображением.

/// Распакованное изображение в RGBA8.
pub struct PsdImage {
    pub width: usize,
    pub height: usize,
    /// Пиксели RGBA8, построчно сверху вниз.
    pub pixels: Vec<u8>,
}

/// Максимальный размер стороны: файл не должен съесть всю память.
const MAX_SIDE: usize = 30_000;

/// Упаковка PackBits: серии повторов и литералы, как в PSD.
pub fn packbits(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() + src.len() / 128 + 8);
    let mut i = 0;
    while i < src.len() {
        // Ищем серию не менее трёх одинаковых байт.
        let mut run = 1;
        while i + run < src.len() && run < 128 && src[i + run] == src[i] {
            run += 1;
        }
        if run >= 3 {
            out.push((257 - run) as u8);
            out.push(src[i]);
            i += run;
            continue;
        }
        // Иначе копируем литерал до следующей серии.
        let start = i;
        while i < src.len() {
            let mut r = 1;
            while i + r < src.len() && r < 3 && src[i + r] == src[i] {
                r += 1;
            }
            if r >= 3 || i - start >= 128 {
                break;
            }
            i += 1;
        }
        let n = i - start;
        out.push((n - 1) as u8);
        out.extend_from_slice(&src[start..i]);
    }
    out
}

/// Распаковка PackBits. Возвращает ошибку, если данных не хватило.
pub fn unpackbits(src: &[u8], want: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(want);
    let mut i = 0;
    while out.len() < want {
        if i >= src.len() {
            return Err("данные RLE оборвались".to_string());
        }
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let len = n as usize + 1;
            if i + len > src.len() {
                return Err("не хватило байтов в RLE".to_string());
            }
            out.extend_from_slice(&src[i..i + len]);
            i += len;
        } else if n != -128 {
            let len = (1 - n as i32) as usize;
            if i >= src.len() {
                return Err("не хватило байта повтора в RLE".to_string());
            }
            let v = src[i];
            i += 1;
            out.extend(std::iter::repeat(v).take(len));
        }
    }
    out.truncate(want);
    Ok(out)
}

/// Читает PSD и возвращает плоское изображение RGBA.
pub fn read(bytes: &[u8]) -> Result<PsdImage, String> {
    let mut r = Reader { b: bytes, p: 0 };
    let sig = r.take(4)?;
    if &sig != b"8BPS" {
        return Err("это не PSD (нет подписи 8BPS)".to_string());
    }
    let version = r.u16()?;
    if version != 1 {
        return Err(format!("версия PSD {} не поддерживается", version));
    }
    let _reserved = r.take(6)?;
    let channels = r.u16()? as usize;
    let height = r.u32()? as usize;
    let width = r.u32()? as usize;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("размер {}×{} неприемлем", width, height));
    }
    if depth != 8 {
        return Err(format!("глубина {} бит не поддерживается (нужно 8)", depth));
    }
    if mode != 3 {
        return Err(format!("режим {} не поддерживается (нужен RGB)", mode));
    }
    if channels < 3 || channels > 4 {
        return Err(format!("{} каналов не поддерживается (нужно 3 или 4)", channels));
    }
    // Блоки «цветовой режим», «ресурсы» и «слои с масками» пропускаем по длине.
    for _ in 0..3 {
        let len = r.u32()? as usize;
        r.skip(len)?;
    }
    let compression = r.u16()?;
    let n = width * height;
    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(channels);
    match compression {
        0 => {
            for _ in 0..channels {
                planes.push(r.take(n)?.to_vec());
            }
        }
        1 => {
            // Сначала идут размеры сжатых строк: по height на каждый канал.
            let mut counts = Vec::with_capacity(channels * height);
            for _ in 0..channels * height {
                counts.push(r.u16()? as usize);
            }
            let mut at = r.p;
            for c in 0..channels {
                let mut plane = Vec::with_capacity(n);
                for row in 0..height {
                    let start = at;
                    let len = counts[c * height + row];
                    at += len;
                    if at > bytes.len() {
                        return Err("данные RLE вышли за конец файла".to_string());
                    }
                    plane.extend_from_slice(&unpackbits(&bytes[start..at], width)?);
                }
                planes.push(plane);
            }
        }
        other => return Err(format!("сжатие {} не поддерживается", other)),
    }
    // Каналы кладём в RGBA; лишние (CMYK и прочее) отброшены выше.
    let mut pixels = vec![0u8; n * 4];
    for x in 0..n {
        pixels[x * 4] = planes[0][x];
        pixels[x * 4 + 1] = planes[1][x];
        pixels[x * 4 + 2] = planes[2][x];
        pixels[x * 4 + 3] = if channels == 4 { planes[3][x] } else { 255 };
    }
    Ok(PsdImage { width, height, pixels })
}

/// Пишет PSD с одним плоским изображением RGBA (сжатие PackBits).
pub fn write(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), width * height * 4);
    let mut out: Vec<u8> = Vec::with_capacity(width * height + 1024);
    out.extend_from_slice(b"8BPS");
    out.extend_from_slice(&1u16.to_be_bytes()); // версия
    out.extend_from_slice(&[0u8; 6]); // зарезервировано
    out.extend_from_slice(&4u16.to_be_bytes()); // каналы: R, G, B, A
    out.extend_from_slice(&(height as u32).to_be_bytes());
    out.extend_from_slice(&(width as u32).to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes()); // глубина
    out.extend_from_slice(&3u16.to_be_bytes()); // режим RGB
    out.extend_from_slice(&0u32.to_be_bytes()); // блок цветового режима пуст
    out.extend_from_slice(&0u32.to_be_bytes()); // ресурсы пусты
    out.extend_from_slice(&0u32.to_be_bytes()); // слои и маски пусты
    out.extend_from_slice(&1u16.to_be_bytes()); // сжатие RLE
    let n = width * height;
    let mut rows: Vec<Vec<u8>> = Vec::with_capacity(4 * height);
    for c in 0..4 {
        for y in 0..height {
            let mut line = Vec::with_capacity(width);
            for x in 0..width {
                line.push(rgba[(y * width + x) * 4 + c]);
            }
            rows.push(packbits(&line));
        }
    }
    for r in &rows {
        out.extend_from_slice(&(r.len() as u16).to_be_bytes());
    }
    for r in &rows {
        out.extend_from_slice(r);
    }
    let _ = n;
    out
}

/// Чтение с проверками — маленький помощник с понятными ошибками.
struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.p + n > self.b.len() {
            return Err("файл неожиданно обрывается".to_string());
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        if self.p + n > self.b.len() {
            return Err("блок длиннее файла".to_string());
        }
        self.p += n;
        Ok(())
    }

    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packbits_round_trip() {
        let cases: Vec<Vec<u8>> = vec![
            vec![1, 2, 3, 4, 5],
            vec![7; 300],
            vec![0, 0, 0, 1, 2, 0, 0, 0, 0, 9],
            (0..255u8).collect(),
            (0..1000u32).map(|i| (i % 7) as u8).collect(),
        ];
        for src in cases {
            let packed = packbits(&src);
            let back = unpackbits(&packed, src.len()).expect("распаковалось");
            assert_eq!(back, src, "раунд-трип для {} байт", src.len());
        }
    }

    #[test]
    fn packbits_never_exceeds_row_limit() {
        // Серия ровно 128 одинаковых байт должна уместиться в один байт-код.
        let src = vec![9u8; 128];
        let packed = packbits(&src);
        assert_eq!(packed.len(), 2, "повтор: код + значение");
        assert_eq!(unpackbits(&packed, 128).unwrap(), src);
    }

    #[test]
    fn psd_round_trip_keeps_pixels() {
        let (w, h) = (7, 5);
        let mut px = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                px[i] = (x * 30) as u8;
                px[i + 1] = (y * 50) as u8;
                px[i + 2] = 128;
                px[i + 3] = if (x + y) % 3 == 0 { 200 } else { 255 };
            }
        }
        let file = write(w, h, &px);
        assert_eq!(&file[0..4], b"8BPS", "подпись формата");
        let img = read(&file).expect("прочитали свой PSD");
        assert_eq!((img.width, img.height), (w, h));
        assert_eq!(img.pixels, px, "пиксели совпали");
    }

    #[test]
    fn psd_reads_three_channels_as_opaque() {
        // Собираем PSD с тремя каналами вручную — как пишут без альфы.
        let (w, h) = (2, 2);
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"8BPS");
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&[0u8; 6]);
        out.extend_from_slice(&3u16.to_be_bytes());
        out.extend_from_slice(&(h as u32).to_be_bytes());
        out.extend_from_slice(&(w as u32).to_be_bytes());
        out.extend_from_slice(&8u16.to_be_bytes());
        out.extend_from_slice(&3u16.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes()); // без сжатия
        for c in 0..3u8 {
            for i in 0..(w * h) {
                out.push(c * 60 + i as u8);
            }
        }
        let img = read(&out).expect("прочитали PSD без альфы");
        assert_eq!(&img.pixels[0..4], &[0, 60, 120, 255], "плоскости идут по каналам");
        assert_eq!(&img.pixels[4..8], &[1, 61, 121, 255], "второй пиксель по той же логике");
    }

    #[test]
    fn psd_rejects_foreign_and_broken_files() {
        assert!(read(b"not a psd at all........").is_err(), "чужая подпись");
        assert!(read(b"8BPS").is_err(), "обрезанный файл");
        let mut bad = write(2, 2, &[0u8; 16]);
        bad[23] = 1; // младший байт глубины: получается 1 вместо 8
        assert!(read(&bad).is_err(), "глубина не 8");
    }
}
