//! Чтение и запись PSD (Photoshop Document) — со слоями, масками и папками.
//!
//! Формат примитивный: заголовок, блок цветового режима, блок ресурсов,
//! блок слоёв и масок и итоговая картинка в конце файла. Данные каналов идут
//! плоско (сначала все красные, потом зелёные и так далее) и сжимаются либо без
//! сжатия, либо PackBits (RLE) — так их пишет сам Photoshop.
//!
//! Слои читаются целиком: кадр, каналы, маска, непрозрачность, видимость,
//! режим наложения и секция группы. Итоговая картинка документа тоже доступна
//! (`merged`) — ею заполняется холст, когда слоёв в файле нет.

use crate::doc::BlendMode;

/// Распакованное изображение в RGBA8.
pub struct PsdImage {
    pub width: usize,
    pub height: usize,
    /// Пиксели RGBA8, построчно сверху вниз.
    pub pixels: Vec<u8>,
}

/// Что за слой по секции PSD: обычный, раскрытая папка, закрытая папка или
/// служебная граница группы.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerKind {
    Pixel,
    GroupOpen,
    GroupClosed,
    /// Служебный слой: в Photoshop им группа заканчивается снизу.
    Bounding,
}

/// Маска слоя PSD: покрытие 0..=255, где 255 — слой открыт.
pub struct PsdMask {
    pub left: i32,
    pub top: i32,
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
    /// Маска выключена в самом файле.
    pub disabled: bool,
}

/// Слой PSD: кадр, каналы, маска и настройки наложения.
pub struct PsdLayer {
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    /// Непрозрачность 0..=255.
    pub opacity: u8,
    pub blend: BlendMode,
    /// Ключ режима наложения из файла — по нему видно, чем заменили незнакомый.
    pub blend_key: [u8; 4],
    /// Слой прижат к нижнему (обрезка по альфе нижнего).
    pub clipped: bool,
    pub left: i32,
    pub top: i32,
    pub width: usize,
    pub height: usize,
    /// Плоскости каналов по идентификаторам PSD: -1 — альфа, 0..2 — RGB.
    pub planes: Vec<(i16, Vec<u8>)>,
    pub mask: Option<PsdMask>,
}

impl PsdLayer {
    /// Плоскость канала; пустая, если канала нет.
    pub fn plane(&self, id: i16) -> &[u8] {
        self.planes
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, d)| d.as_slice())
            .unwrap_or(&[])
    }

    /// Пиксель слоя в RGBA по координатам внутри кадра слоя.
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        if self.planes.is_empty() || self.width == 0 || x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let i = y * self.width + x;
        let at = |id: i16, def: u8| self.plane(id).get(i).copied().unwrap_or(def);
        [at(0, 0), at(1, 0), at(2, 0), at(-1, 255)]
    }
}

/// PSD целиком: слои (снизу вверх, как в файле) и итоговая картинка.
pub struct PsdFile {
    pub width: usize,
    pub height: usize,
    pub layers: Vec<PsdLayer>,
    pub merged: PsdImage,
}

/// Режим наложения по четырёхбайтовому ключу PSD. Незнакомые режимы
/// (цветовые, градиентные) становятся обычными — их ключ остаётся в слое.
pub fn blend_of(key: &[u8]) -> BlendMode {
    match key {
        b"mul " => BlendMode::Multiply,
        b"scrn" => BlendMode::Screen,
        b"over" => BlendMode::Overlay,
        b"add " | b"lin " => BlendMode::Add,
        b"dark" => BlendMode::Darken,
        b"lite" => BlendMode::Lighten,
        b"diff" => BlendMode::Difference,
        b"div " => BlendMode::ColorDodge,
        b"idiv" => BlendMode::ColorBurn,
        _ => BlendMode::Normal,
    }
}

/// Обратная задача: ключ PSD для нашего режима наложения.
pub fn blend_key(b: BlendMode) -> [u8; 4] {
    match b {
        BlendMode::Normal => *b"norm",
        BlendMode::Multiply => *b"mul ",
        BlendMode::Screen => *b"scrn",
        BlendMode::Overlay => *b"over",
        BlendMode::Add => *b"lin ",
        BlendMode::Darken => *b"dark",
        BlendMode::Lighten => *b"lite",
        BlendMode::Difference => *b"diff",
        BlendMode::ColorDodge => *b"div ",
        BlendMode::ColorBurn => *b"idiv",
    }
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

/// Читает PSD и возвращает плоское изображение RGBA (без слоёв).
pub fn read(bytes: &[u8]) -> Result<PsdImage, String> {
    read_file(bytes).map(|f| f.merged)
}

/// Читает PSD целиком: слои, маски и итоговую картинку документа.
pub fn read_file(bytes: &[u8]) -> Result<PsdFile, String> {
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
    // Блок цветового режима и блок ресурсов пропускаем по длине.
    for _ in 0..2 {
        let len = r.u32()? as usize;
        r.skip(len)?;
    }
    // Блок слоёв и масок разбираем, а не пропускаем: слои — самое ценное.
    let lm_len = r.u32()? as usize;
    if r.p + lm_len > bytes.len() {
        return Err("блок слоёв длиннее файла".to_string());
    }
    let lm_end = r.p + lm_len;
    let layers = read_layers(&mut r, lm_end)?;
    // Всё, что осталось в блоке (глобальная маска, доп. блоки), пропускаем.
    r.p = lm_end;
    let merged = read_merged(&mut r, channels, width, height)?;
    Ok(PsdFile { width, height, layers, merged })
}

/// Читает секцию слоёв внутри блока слоёв и масок (до `end` включительно).
fn read_layers(r: &mut Reader, end: usize) -> Result<Vec<PsdLayer>, String> {
    if r.p + 4 > end {
        return Ok(Vec::new());
    }
    let len = r.u32()? as usize;
    if len == 0 {
        return Ok(Vec::new());
    }
    let li_end = (r.p + len).min(end);
    // Минус означает, что первый альфа-канал хранит прозрачность итога.
    let count = r.i16()?;
    let n = count.unsigned_abs() as usize;
    if n > 4096 {
        return Err(format!("{} слоёв — слишком много", n));
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if r.p >= li_end {
            break;
        }
        out.push(read_layer(r)?);
    }
    r.p = li_end;
    Ok(out)
}

/// Читает одну запись слоя и её каналы.
fn read_layer(r: &mut Reader) -> Result<PsdLayer, String> {
    let top = r.i32()?;
    let left = r.i32()?;
    let bottom = r.i32()?;
    let right = r.i32()?;
    let width = (right - left).max(0) as usize;
    let height = (bottom - top).max(0) as usize;
    let nch = r.u16()? as usize;
    if nch > 64 {
        return Err("слишком много каналов у слоя".to_string());
    }
    // Идентификаторы каналов перечислены в записи, а сами плоскости идут
    // сразу после неё — в том же порядке, поэтому длины тут только читаем.
    let mut ids = Vec::with_capacity(nch);
    for _ in 0..nch {
        ids.push(r.i16()?);
        let _len = r.u32()?;
    }
    let _sig = r.take(4)?; // "8BIM"
    let key = r.take(4)?;
    let opacity = r.take(1)?[0];
    let clipping = r.take(1)?[0];
    let flags = r.take(1)?[0];
    let _filler = r.take(1)?;
    let extra = r.u32()? as usize;
    let extra_end = r.p + extra;
    if extra_end > r.b.len() {
        return Err("служебные данные слоя длиннее файла".to_string());
    }
    let mask = read_mask(r)?;
    let ranges = r.u32()? as usize;
    r.skip(ranges)?;
    let mut name = read_pascal(r)?;
    let mut kind = LayerKind::Pixel;
    // Дополнительные блоки: пока читаем Unicode-имя и тип секции группы.
    while r.p + 12 <= extra_end {
        let key4 = r.take(4)?;
        let len = r.u32()? as usize;
        if len == 0 {
            break;
        }
        if len > extra_end - r.p {
            break;
        }
        if &key4 == b"luni" {
            let nchars = r.u32()? as usize;
            if nchars.saturating_mul(2) <= len - 4 {
                let raw = r.take(nchars * 2)?;
                name = decode_utf16(raw);
            } else {
                r.p += len;
            }
        } else if &key4 == b"lsct" {
            let v = r.u32()?;
            kind = match v {
                1 => LayerKind::GroupOpen,
                2 => LayerKind::GroupClosed,
                3 => LayerKind::Bounding,
                _ => LayerKind::Pixel,
            };
            r.p += len.saturating_sub(4);
        } else {
            r.skip(len)?;
        }
    }
    r.p = extra_end;
    // Каналы: у каждого свой метод сжатия, дальше идут плоскости по порядку.
    let mut planes = Vec::with_capacity(nch);
    for id in ids {
        let comp = r.u16()?;
        if width == 0 || height == 0 {
            planes.push((id, Vec::new()));
            continue;
        }
        planes.push((id, read_plane(r, width, height, comp)?));
    }
    let mut blend_key = [0u8; 4];
    blend_key.copy_from_slice(key);
    Ok(PsdLayer {
        name,
        kind,
        visible: flags & 0x02 == 0,
        opacity,
        blend: blend_of(key),
        blend_key,
        clipped: clipping != 0,
        left,
        top,
        width,
        height,
        planes,
        mask,
    })
}

/// Читает маску слоя: кадр, цвет «по умолчанию», флаги и плоскость покрытия.
fn read_mask(r: &mut Reader) -> Result<Option<PsdMask>, String> {
    let len = r.u32()? as usize;
    if len == 0 {
        return Ok(None);
    }
    let end = r.p + len;
    if end > r.b.len() {
        return Ok(None);
    }
    let top = r.i32()?;
    let left = r.i32()?;
    let bottom = r.i32()?;
    let right = r.i32()?;
    let _default = r.take(1)?[0];
    let flags = r.take(1)?[0];
    let width = (right - left).max(0) as usize;
    let height = (bottom - top).max(0) as usize;
    // Между заголовком и картинкой маски может стоять подпись "8BIM" с
    // параметрами. Проверяем её на месте: у пустой маски её нет вовсе.
    if r.p + 8 <= end && &r.b[r.p..r.p + 4] == b"8BIM" {
        r.p += 4;
        let params = r.u32()? as usize;
        r.skip(params.min(end.saturating_sub(r.p)))?;
    }
    let mut data = Vec::new();
    if width > 0 && height > 0 && r.p + 2 <= end {
        let comp = r.u16()?;
        data = read_plane(r, width, height, comp)?;
    }
    r.p = end;
    Ok(Some(PsdMask {
        left,
        top,
        width,
        height,
        data,
        disabled: flags & 0x02 != 0,
    }))
}

/// Плоскость `w`×`h` байт: без сжатия или PackBits по строкам.
fn read_plane(r: &mut Reader, w: usize, h: usize, comp: u16) -> Result<Vec<u8>, String> {
    let want = w * h;
    match comp {
        0 => Ok(r.take(want)?.to_vec()),
        1 => {
            // Сначала идут размеры сжатых строк: по одному на строку.
            let mut counts = Vec::with_capacity(h);
            for _ in 0..h {
                counts.push(r.u16()? as usize);
            }
            let mut out = Vec::with_capacity(want);
            for len in counts {
                if r.p + len > r.b.len() {
                    return Err("строка канала вышла за конец файла".to_string());
                }
                out.extend_from_slice(&unpackbits(&r.b[r.p..r.p + len], w)?);
                r.p += len;
            }
            Ok(out)
        }
        other => Err(format!("сжатие {} не поддерживается", other)),
    }
}

/// Итоговая картинка документа в конце файла.
fn read_merged(
    r: &mut Reader,
    channels: usize,
    width: usize,
    height: usize,
) -> Result<PsdImage, String> {
    let compression = r.u16()?;
    let n = width * height;
    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(channels);
    for _ in 0..channels {
        planes.push(read_plane(r, width, height, compression)?);
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

/// Строка-имя слоя: длина одним байтом, затем символы, всё до кратности 4.
fn read_pascal(r: &mut Reader) -> Result<String, String> {
    let n = r.take(1)?[0] as usize;
    let raw = r.take(n)?;
    let pad = (4 - (n + 1) % 4) % 4;
    r.skip(pad)?;
    Ok(raw.iter().map(|&c| c as char).collect())
}

/// UTF-16BE в строку — так Photoshop хранит настоящее имя слоя.
fn decode_utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&units)
}

/// Пишет PSD с одним плоским изображением RGBA (сжатие PackBits).
pub fn write(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), width * height * 4);
    let mut out: Vec<u8> = Vec::with_capacity(width * height + 1024);
    write_header(&mut out, width, height, 4);
    out.extend_from_slice(&0u32.to_be_bytes()); // блок цветового режима пуст
    out.extend_from_slice(&0u32.to_be_bytes()); // ресурсы пусты
    out.extend_from_slice(&0u32.to_be_bytes()); // слои и маски пусты
    write_merged(&mut out, width, height, rgba);
    out
}

/// Слой для записи в PSD. Порядок — как в документе: слой 0 нижний.
pub struct OutLayer<'a> {
    pub name: &'a str,
    /// Пиксели слоя на весь холст, RGBA8.
    pub rgba: &'a [u8],
    /// Маска слоя, 8 бит покрытия на пиксель холста.
    pub mask: Option<&'a [u8]>,
    pub opacity: u8,
    pub visible: bool,
    pub blend: BlendMode,
    /// Имя папки, в которой лежит слой.
    pub group: Option<&'a str>,
    /// Слой является шапкой своей папки.
    pub group_header: bool,
    pub group_open: bool,
}

/// Пишет PSD со слоями: у каждого кадр по содержимому, маска, непрозрачность,
/// видимость, режим наложения и имя; папки становятся секциями групп. В конце
/// идёт итоговая картинка документа — как и требует Photoshop.
pub fn write_layers(width: usize, height: usize, composite: &[u8], layers: &[OutLayer]) -> Vec<u8> {
    assert_eq!(composite.len(), width * height * 4);
    let mut out: Vec<u8> = Vec::with_capacity(width * height + 4096);
    write_header(&mut out, width, height, 4);
    out.extend_from_slice(&0u32.to_be_bytes()); // блок цветового режима пуст
    out.extend_from_slice(&0u32.to_be_bytes()); // ресурсы пусты

    // Записи слоёв строим в порядке документа: слой 0 — нижний, и так же
    // их ждёт Photoshop.
    struct Rec<'a> {
        kind: LayerKind,
        layer: Option<&'a OutLayer<'a>>,
    }
    let mut recs: Vec<Rec> = Vec::new();
    // Идём снизу вверх; начало новой папки требует служебный слой-границу.
    let mut group: Option<&str> = None;
    for l in layers.iter() {
        if l.group != group {
            if l.group.is_some() {
                recs.push(Rec { kind: LayerKind::Bounding, layer: None });
            }
            group = l.group;
        }
        let kind = if l.group_header {
            if l.group_open { LayerKind::GroupOpen } else { LayerKind::GroupClosed }
        } else {
            LayerKind::Pixel
        };
        // У слоя-шапки может быть своё содержимое, а в PSD у папки каналов нет.
        // Поэтому содержимое пишем обычным слоем, а шапку — пустой папкой.
        if kind != LayerKind::Pixel && has_content(l.rgba) {
            recs.push(Rec { kind: LayerKind::Pixel, layer: Some(l) });
        }
        recs.push(Rec { kind, layer: Some(l) });
    }

    let mut body: Vec<u8> = Vec::new();
    for rec in &recs {
        let (Some(l), LayerKind::Pixel) = (rec.layer, rec.kind) else {
            // Служебные слои групп: без каналов, только имя и тип секции.
            // Имя слоя-папки в PSD — это имя самой папки.
            let name = match rec.layer {
                Some(l) => l.group.unwrap_or(l.name),
                None => "Группа",
            };
            let head = if rec.kind == LayerKind::Bounding { 3u32 } else { 1u32 };
            body.extend_from_slice(&0i32.to_be_bytes()); // кадр пустой
            body.extend_from_slice(&0i32.to_be_bytes());
            body.extend_from_slice(&0i32.to_be_bytes());
            body.extend_from_slice(&0i32.to_be_bytes());
            body.extend_from_slice(&0u16.to_be_bytes()); // ни одного канала
            body.extend_from_slice(b"8BIM");
            // У слоя-папки сохраняем режим, непрозрачность и маску, у
            // служебной границы группы — обычный режим и скрытый флаг.
            let (key, op, flags, mask) = match rec.layer {
                Some(l) => (
                    blend_key(l.blend),
                    l.opacity,
                    if l.visible { 0u8 } else { 0x02 },
                    mask_bytes(width, height, l.mask),
                ),
                // У служебной границы группы маски нет, но её длина в блоке
                // обязательна — иначе запись поедет.
                None => (blend_key(BlendMode::Normal), 255u8, 0x02, 0u32.to_be_bytes().to_vec()),
            };
            body.extend_from_slice(&key);
            body.extend_from_slice(&[op]);
            body.extend_from_slice(&[0u8]); // не обрезан
            body.extend_from_slice(&[flags]);
            body.extend_from_slice(&[0u8]);
            let extra = extra_bytes(name, Some(head), mask);
            body.extend_from_slice(&(extra.len() as u32).to_be_bytes());
            body.extend_from_slice(&extra);
            continue;
        };
        // Кадр слоя — тесная рамка вокруг непрозрачного содержимого.
        let (x0, y0, x1, y1) = tight_rect(width, height, l.rgba);
        let rw = (x1 - x0) as usize;
        let rh = (y1 - y0) as usize;
        let chans: [(i16, usize); 4] = [(-1, 3), (0, 0), (1, 1), (2, 2)];
        // Данные каналов собираем отдельно: их длины нужны в записи слоя.
        let mut datas: Vec<Vec<u8>> = Vec::with_capacity(4);
        for (_, c) in chans {
            let mut plane: Vec<u8> = Vec::with_capacity(rw * rh);
            for y in 0..rh {
                for x in 0..rw {
                    plane.push(l.rgba[((y0 + y as i32) as usize * width + (x0 + x as i32) as usize) * 4 + c]);
                }
            }
            datas.push(plane_rows(&plane, rw, rh));
        }
        body.extend_from_slice(&y0.to_be_bytes());
        body.extend_from_slice(&x0.to_be_bytes());
        body.extend_from_slice(&y1.to_be_bytes());
        body.extend_from_slice(&x1.to_be_bytes());
        body.extend_from_slice(&4u16.to_be_bytes());
        for (i, (id, _)) in chans.iter().enumerate() {
            body.extend_from_slice(&id.to_be_bytes());
            body.extend_from_slice(&(datas[i].len() as u32).to_be_bytes());
        }
        body.extend_from_slice(b"8BIM");
        body.extend_from_slice(&blend_key(l.blend));
        body.extend_from_slice(&[l.opacity]);
        body.extend_from_slice(&[0u8]); // не прижат: прижатие мы не переносим
        body.extend_from_slice(&[if l.visible { 0u8 } else { 0x02 }]);
        body.extend_from_slice(&[0u8]);
        let extra = extra_bytes(l.name, None, mask_bytes(width, height, l.mask));
        body.extend_from_slice(&(extra.len() as u32).to_be_bytes());
        body.extend_from_slice(&extra);
        for d in &datas {
            body.extend_from_slice(d);
        }
    }
    // Длина секции слоёв кратна двум, как требует формат.
    if body.len() % 2 == 1 {
        body.push(0);
    }
    // Порядок обязателен: сначала длина секции (с числом слоёв внутри неё),
    // затем само число слоёв, потом записи.
    let mut info: Vec<u8> = Vec::new();
    info.extend_from_slice(&((body.len() + 2) as u32).to_be_bytes());
    info.extend_from_slice(&(recs.len() as i16).to_be_bytes());
    info.extend_from_slice(&body);

    let mut lm: Vec<u8> = Vec::new();
    lm.extend_from_slice(&info);
    lm.extend_from_slice(&0u32.to_be_bytes()); // глобальная маска пуста
    lm.extend_from_slice(&0u32.to_be_bytes()); // доп. информация пуста
    out.extend_from_slice(&(lm.len() as u32).to_be_bytes());
    out.extend_from_slice(&lm);
    write_merged(&mut out, width, height, composite);
    out
}

fn write_header(out: &mut Vec<u8>, width: usize, height: usize, channels: u16) {
    out.extend_from_slice(b"8BPS");
    out.extend_from_slice(&1u16.to_be_bytes()); // версия
    out.extend_from_slice(&[0u8; 6]); // зарезервировано
    out.extend_from_slice(&channels.to_be_bytes()); // R, G, B, A
    out.extend_from_slice(&(height as u32).to_be_bytes());
    out.extend_from_slice(&(width as u32).to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes()); // глубина
    out.extend_from_slice(&3u16.to_be_bytes()); // режим RGB
}

fn write_merged(out: &mut Vec<u8>, width: usize, height: usize, rgba: &[u8]) {
    out.extend_from_slice(&1u16.to_be_bytes()); // сжатие RLE
    // Порядок как в Photoshop: у каждого канала сначала размеры строк,
    // потом сами строки.
    for c in 0..4 {
        let mut rows: Vec<Vec<u8>> = Vec::with_capacity(height);
        for y in 0..height {
            let mut line = Vec::with_capacity(width);
            for x in 0..width {
                line.push(rgba[(y * width + x) * 4 + c]);
            }
            rows.push(packbits(&line));
        }
        for r in &rows {
            out.extend_from_slice(&(r.len() as u16).to_be_bytes());
        }
        for r in &rows {
            out.extend_from_slice(r);
        }
    }
}

/// Плоскость в виде сжатых строк: два байта сжатия, размеры строк, сами строки.
fn plane_rows(plane: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(plane.len() + h * 2 + 8);
    out.extend_from_slice(&1u16.to_be_bytes());
    let mut rows: Vec<Vec<u8>> = Vec::with_capacity(h);
    for y in 0..h {
        rows.push(packbits(&plane[y * w..y * w + w]));
    }
    for r in &rows {
        out.extend_from_slice(&(r.len() as u16).to_be_bytes());
    }
    for r in &rows {
        out.extend_from_slice(r);
    }
    out
}

/// Есть ли в слое что рисовать: хотя бы один непрозрачный пиксель.
fn has_content(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).any(|p| p[3] != 0)
}

/// Рамка непрозрачного содержимого слоя. Пустой слой получает кадр 1×1:
/// так его честно видно в панели слоёв.
fn tight_rect(w: usize, h: usize, rgba: &[u8]) -> (i32, i32, i32, i32) {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, -1i32, -1i32);
    for y in 0..h {
        for x in 0..w {
            if rgba[(y * w + x) * 4 + 3] == 0 {
                continue;
            }
            let (xi, yi) = (x as i32, y as i32);
            x0 = x0.min(xi);
            y0 = y0.min(yi);
            x1 = x1.max(xi + 1);
            y1 = y1.max(yi + 1);
        }
    }
    if x1 < 0 { (0, 0, 1, 1) } else { (x0, y0, x1, y1) }
}

/// Служебные данные слоя: маска, пустые диапазоны, имя (в двух видах) и
/// тип секции. Порядок именно такой — его ждёт Photoshop.
fn extra_bytes(name: &str, section: Option<u32>, mask: Vec<u8>) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(64 + mask.len());
    out.extend_from_slice(&mask);
    out.extend_from_slice(&0u32.to_be_bytes()); // диапазоны смешивания пусты
    let mut nm: Vec<u8> = Vec::with_capacity(name.len() + 1);
    nm.push(name.len().min(255) as u8);
    nm.extend_from_slice(&name.as_bytes()[..name.len().min(255)]);
    while nm.len() % 4 != 0 {
        nm.push(0);
    }
    out.extend_from_slice(&nm);
    // Настоящее имя в UTF-16 — его Photoshop и показывает в панели.
    let units: Vec<u16> = name.encode_utf16().collect();
    out.extend_from_slice(b"luni");
    out.extend_from_slice(&((units.len() * 2 + 4) as u32).to_be_bytes());
    out.extend_from_slice(&(units.len() as u32).to_be_bytes());
    for u in units {
        out.extend_from_slice(&u.to_be_bytes());
    }
    if let Some(v) = section {
        out.extend_from_slice(b"lsct");
        out.extend_from_slice(&4u32.to_be_bytes());
        out.extend_from_slice(&v.to_be_bytes());
    }
    out
}

/// Маска слоя в виде блока PSD. Маску, которая ничего не закрывает, не пишем.
fn mask_bytes(w: usize, h: usize, mask: Option<&[u8]>) -> Vec<u8> {
    let Some(m) = mask else { return 0u32.to_be_bytes().to_vec() };
    if m.len() != w * h || m.iter().all(|&v| v == 255) {
        return 0u32.to_be_bytes().to_vec();
    }
    let mut data: Vec<u8> = Vec::with_capacity(w * h + 32);
    data.extend_from_slice(&0i32.to_be_bytes()); // кадр маски — весь холст
    data.extend_from_slice(&0i32.to_be_bytes());
    data.extend_from_slice(&(h as i32).to_be_bytes());
    data.extend_from_slice(&(w as i32).to_be_bytes());
    data.push(0); // цвет вне маски
    data.push(0); // флаги: маска включена, не инвертирована
    data.extend_from_slice(b"8BIM");
    data.extend_from_slice(&0u32.to_be_bytes()); // параметров у маски нет
    data.extend_from_slice(&plane_rows(m, w, h));
    let mut out: Vec<u8> = Vec::with_capacity(data.len() + 4);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&data);
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
            return Err(format!("блок {} байт не помещается (позиция {})", n, self.p));
        }
        self.p += n;
        Ok(())
    }

    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }

    fn i16(&mut self) -> Result<i16, String> {
        let s = self.take(2)?;
        Ok(i16::from_be_bytes([s[0], s[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn i32(&mut self) -> Result<i32, String> {
        let s = self.take(4)?;
        Ok(i32::from_be_bytes([s[0], s[1], s[2], s[3]]))
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

    /// Слой для проверки: сплошной прямоугольник своего цвета.
    fn flat(w: usize, h: usize, c: [u8; 4]) -> Vec<u8> {
        let mut px = vec![0u8; w * h * 4];
        for p in px.chunks_exact_mut(4) {
            p.copy_from_slice(&c);
        }
        px
    }

    #[test]
    fn psd_layers_round_trip_with_names_and_settings() {
        let (w, h) = (6, 4);
        let top = flat(w, h, [255, 0, 0, 255]);
        let bottom = flat(w, h, [0, 0, 255, 128]);
        let mut mask = vec![255u8; w * h];
        for y in 0..2 {
            for x in 0..w {
                mask[y * w + x] = 40;
            }
        }
        let layers = vec![
            OutLayer {
                name: "Низ",
                rgba: &bottom,
                mask: Some(&mask),
                opacity: 128,
                visible: false,
                blend: BlendMode::Normal,
                group: None,
                group_header: false,
                group_open: true,
            },
            OutLayer {
                name: "Верх",
                rgba: &top,
                mask: None,
                opacity: 255,
                visible: true,
                blend: BlendMode::Multiply,
                group: None,
                group_header: false,
                group_open: true,
            },
        ];
        let merged = flat(w, h, [10, 20, 30, 255]);
        let file = write_layers(w, h, &merged, &layers);
        let back = read_file(&file).expect("прочитали свой PSD со слоями");
        assert_eq!((back.width, back.height), (w, h));
        assert_eq!(back.merged.pixels, merged, "итоговая картинка на месте");
        assert_eq!(back.layers.len(), 2, "оба слоя прочитаны");
        // Порядок как в документе: слой 0 — нижний.
        let low = &back.layers[0];
        assert_eq!(low.name, "Низ", "имя слоя");
        assert!(!low.visible, "скрытый слой остался скрытым");
        assert_eq!(low.opacity, 128, "непрозрачность");
        assert_eq!(low.pixel(0, 0), [0, 0, 255, 128], "пиксели нижнего слоя");
        assert_eq!(low.pixel(0, 3), [0, 0, 255, 128]);
        let m = low.mask.as_ref().expect("маска прочитана");
        assert_eq!(m.width, w, "маска на весь холст");
        assert_eq!(m.data[0], 40, "верх маски закрыт");
        assert_eq!(m.data[w * 3], 255, "низ маски открыт");
        let up = &back.layers[1];
        assert_eq!(up.name, "Верх");
        assert_eq!(up.blend, BlendMode::Multiply, "режим наложения");
        assert!(up.mask.is_none(), "у слоя без маски маски нет");
    }

    #[test]
    fn psd_groups_become_sections_and_keep_names() {
        let (w, h) = (4, 4);
        let a = flat(w, h, [255, 255, 0, 255]);
        let b = flat(w, h, [0, 255, 255, 255]);
        let header = flat(w, h, [0, 0, 0, 0]);
        // Порядок документа: слой 0 — нижний, шапка папки — самый верхний
        // слой своей группы.
        let layers = vec![
            OutLayer {
                name: "Снаружи",
                rgba: &b,
                mask: None,
                opacity: 255,
                visible: true,
                blend: BlendMode::Normal,
                group: None,
                group_header: false,
                group_open: true,
            },
            OutLayer {
                name: "Внутри",
                rgba: &a,
                mask: None,
                opacity: 255,
                visible: true,
                blend: BlendMode::Normal,
                group: Some("Папка"),
                group_header: false,
                group_open: true,
            },
            OutLayer {
                name: "Папка",
                rgba: &header,
                mask: None,
                opacity: 255,
                visible: true,
                blend: BlendMode::Normal,
                group: Some("Папка"),
                group_header: true,
                group_open: true,
            },
        ];
        let merged = flat(w, h, [1, 2, 3, 255]);
        let back = read_file(&write_layers(w, h, &merged, &layers)).expect("прочитали группы");
        let kinds: Vec<LayerKind> = back.layers.iter().map(|l| l.kind).collect();
        // Снизу вверх: слой вне папки, граница, содержимое, шапка папки.
        assert_eq!(
            kinds,
            vec![
                LayerKind::Pixel,
                LayerKind::Bounding,
                LayerKind::Pixel,
                LayerKind::GroupOpen
            ],
            "структура группы: {:?}",
            kinds
        );
        assert_eq!(back.layers[3].name, "Папка", "имя шапки");
        assert_eq!(back.layers[2].name, "Внутри", "имя содержимого");
        assert_eq!(back.layers[1].pixel(0, 0), [0, 0, 0, 0], "граница прозрачна");
    }

    #[test]
    fn psd_layer_frame_is_placed_by_its_offset() {
        // Слой 2×2 со сдвигом: кадр начинается не в углу холста.
        let (w, h) = (3, 3);
        // Собираем файл вручную: один слой с кадром (1,1)-(3,3).
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"8BPS");
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&[0u8; 6]);
        out.extend_from_slice(&4u16.to_be_bytes());
        out.extend_from_slice(&(h as u32).to_be_bytes());
        out.extend_from_slice(&(w as u32).to_be_bytes());
        out.extend_from_slice(&8u16.to_be_bytes());
        out.extend_from_slice(&3u16.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        // Блок слоёв: один слой, кадр со сдвигом, каналы без сжатия.
        // Порядок каналов как в Photoshop: сначала альфа, потом R, G, B.
        let mut chans: Vec<Vec<u8>> = Vec::new();
        for c in [3usize, 0, 1, 2] {
            let plane: Vec<u8> = (0..4).map(|i| (i * 30 + c * 5) as u8).collect();
            let mut d = vec![0u8, 0]; // сжатие «без сжатия»
            d.extend_from_slice(&plane);
            chans.push(d);
        }
        let mut rec: Vec<u8> = Vec::new();
        rec.extend_from_slice(&1i32.to_be_bytes()); // top
        rec.extend_from_slice(&1i32.to_be_bytes()); // left
        rec.extend_from_slice(&3i32.to_be_bytes()); // bottom
        rec.extend_from_slice(&3i32.to_be_bytes()); // right
        rec.extend_from_slice(&4u16.to_be_bytes());
        for (i, id) in [-1i16, 0, 1, 2].iter().enumerate() {
            rec.extend_from_slice(&id.to_be_bytes());
            rec.extend_from_slice(&(chans[i].len() as u32).to_be_bytes());
        }
        rec.extend_from_slice(b"8BIM");
        rec.extend_from_slice(b"norm");
        rec.extend_from_slice(&[255u8]);
        rec.extend_from_slice(&[0u8]);
        rec.extend_from_slice(&[0u8]);
        rec.extend_from_slice(&[0u8]);
        let extra = extra_bytes("Сдвинутый", None, 0u32.to_be_bytes().to_vec());
        rec.extend_from_slice(&(extra.len() as u32).to_be_bytes());
        rec.extend_from_slice(&extra);
        let rec_len = rec.len();
        assert_eq!(rec_len, 116, "длина записи слоя");
        let mut body: Vec<u8> = rec;
        for c in &chans {
            body.extend_from_slice(c);
        }
        if body.len() % 2 == 1 {
            body.push(0);
        }
        // Порядок обязателен: сначала длина секции (с числом слоёв внутри неё),
        // затем само число слоёв.
        let mut info: Vec<u8> = Vec::new();
        info.extend_from_slice(&((body.len() + 2) as u32).to_be_bytes());
        info.extend_from_slice(&1u16.to_be_bytes()); // один слой
        info.extend_from_slice(&body);
        let mut lm: Vec<u8> = Vec::new();
        lm.extend_from_slice(&info);
        lm.extend_from_slice(&0u32.to_be_bytes());
        lm.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&(lm.len() as u32).to_be_bytes());
        out.extend_from_slice(&lm);
        // Итоговая картинка: 3×3, альфа 255.
        out.extend_from_slice(&0u16.to_be_bytes());
        for c in 0..4 {
            for i in 0..9 {
                out.push(if c == 3 { 255 } else { (i + c) as u8 });
            }
        }
        let back = read_file(&out).expect("прочитали сдвинутый слой");
        assert_eq!(back.layers.len(), 1);
        let l = &back.layers[0];
        assert_eq!((l.left, l.top, l.width, l.height), (1, 1, 2, 2), "кадр слоя");
        assert_eq!(l.name, "Сдвинутый", "имя из luni");
        // Плоскости собраны так: 0, 30, 60, 90 по каждому каналу.
        assert_eq!(l.pixel(1, 1), [90, 95, 100, 105], "пиксель внутри кадра");
    }

    #[test]
    fn psd_reads_blend_and_hides_unknown_modes() {
        // Неизвестный режим читается как обычный, но ключ остаётся в слое.
        assert_eq!(blend_of(b"norm"), BlendMode::Normal);
        assert_eq!(blend_of(b"mul "), BlendMode::Multiply);
        assert_eq!(blend_of(b"scrn"), BlendMode::Screen);
        assert_eq!(blend_of(b"over"), BlendMode::Overlay);
        assert_eq!(blend_of(b"lin "), BlendMode::Add);
        assert_eq!(blend_of(b"dark"), BlendMode::Darken);
        assert_eq!(blend_of(b"lite"), BlendMode::Lighten);
        assert_eq!(blend_of(b"diff"), BlendMode::Difference);
        assert_eq!(blend_of(b"div "), BlendMode::ColorDodge);
        assert_eq!(blend_of(b"idiv"), BlendMode::ColorBurn);
        assert_eq!(blend_of(b"vivid"), BlendMode::Normal, "цветовой режим не умеем");
        assert_eq!(blend_of(b"sLit"), BlendMode::Normal, "мягкий свет не умеем");
        // Все наши режимы пишутся ключом, который читается обратно в тот же.
        for m in BlendMode::ALL {
            assert_eq!(blend_of(&blend_key(m)), m, "режим {:?} не пережил обмен", m);
        }
    }
}
