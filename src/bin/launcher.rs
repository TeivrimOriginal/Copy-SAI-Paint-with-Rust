//! Лаунчер Tpaint: маленькое стартовое окно.
//!
//! Задача лаунчера — не дублировать редактор, а быстро выбрать, с чего
//! начать: новый документ, недавний проект или открыть файл. Он использует
//! тот же движок интерфейса (GLFW + шрифты + OpenGL-рендерер), что и сам
//! редактор, и запускает его нужной командной строкой.
//!
//! Команды, которые понимает редактор:
//!   tpaint.exe                      — пустой документ
//!   tpaint.exe путь\к\файлу         — открыть проект или картинку
//!   tpaint.exe --new 1920x1080      — новый документ заданного размера

use std::path::PathBuf;
use std::process::Command;

use glfw::{Context, Key, MouseButtonLeft, WindowEvent};
use tpaint::app::{DialogMode, FileDialog};
use tpaint::layout::{file_dialog, Action};
use tpaint::renderer::Renderer;
use tpaint::text::Fonts;
use tpaint::ui::{theme, FONT_SMALL, FONT_UI, KeyEv, Ui, PAD};

const WIN_W: u32 = 620;
const WIN_H: u32 = 520;
const MAX_RECENT: usize = 8;

/// Готовые размеры нового документа: под рисование, дизайн и соцсети.
const PRESETS: &[(&str, usize, usize)] = &[
    ("Экран 1920×1080", 1920, 1080),
    ("Full HD +  1600×1000", 1600, 1000),
    ("Квадрат 1080×1080", 1080, 1080),
    ("Соцсеть 1200×630", 1200, 630),
    ("A4 при 300 dpi  2480×3508", 2480, 3508),
    ("Рабочий стол  1280×800", 1280, 800),
];

/// Размер нового документа, выбранный вручную.
struct LaunchState {
    custom_w: f32,
    custom_h: f32,
    /// Индекс выбранного пресета.
    preset: usize,
    dialog: Option<FileDialog>,
    recent: Vec<PathBuf>,
    /// Что запускать: None — ещё ничего не выбрано.
    launch: Option<Vec<String>>,
    /// Подпись к последнему действию (показывается внизу окна).
    status: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Если лаунчеру дали файл — сразу запускаем редактор с ним.
    if let Some(first) = args.first().filter(|a| !a.starts_with("--")) {
        let _ = start_editor(&[first.clone()]);
        return;
    }

    enable_dpi_awareness();
    let mut glfw = match glfw::init::<()>(None) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Не удалось инициализировать GLFW: {:?}", e);
            return;
        }
    };
    glfw.window_hint(glfw::WindowHint::ContextVersionMajor(3));
    glfw.window_hint(glfw::WindowHint::ContextVersionMinor(3));
    glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    glfw.window_hint(glfw::WindowHint::Resizable(false));
    // Без MSAA: иначе мелкий текст мылится (см. main.rs).
    glfw.window_hint(glfw::WindowHint::Samples(None));

    let (mut window, events) = glfw
        .create_window(WIN_W, WIN_H, "Tpaint — запуск", glfw::WindowMode::Windowed)
        .expect("не удалось создать окно лаунчера");
    window.set_all_polling(true);
    window.make_current();
    glfw.set_swap_interval(glfw::SwapInterval::Sync(1));
    ::gl::load_with(|s| window.get_proc_address(s));

    let fonts = match Fonts::load() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Не удалось загрузить шрифт: {}", e);
            return;
        }
    };
    let (mut fw, mut fh) = window.get_framebuffer_size();
    let mut renderer = Renderer::new(fw, fh);
    let mut ui = Ui::new(fonts);

    let mut st = LaunchState {
        custom_w: 1920.0,
        custom_h: 1080.0,
        preset: 0,
        dialog: None,
        recent: read_recent(),
        launch: None,
        status: String::new(),
    };

    let mut prev = (false, false);
    // Сколько кадров ещё осталось до снимка (0 — не снимаем).
    let mut shot_frame = if std::env::var("TPAINT_LAUNCHER_SHOT").is_ok() { 6 } else { 0 };
    // Атлас глифов перезаливаем только когда нарисованы новые символы.
    let mut atlas_rev = u64::MAX;
    while !window.should_close() {
        glfw.poll_events();
        while let Ok((_, ev)) = events.try_recv() {
            match ev {
                WindowEvent::FramebufferSize(w, h) => {
                    if w > 0 && h > 0 {
                        fw = w;
                        fh = h;
                    }
                }
                WindowEvent::Close => return,
                WindowEvent::MouseButton(MouseButtonLeft, glfw::Action::Press, _) => window.focus(),
                WindowEvent::Key(key, _, action, _) => {
                    if action == glfw::Action::Press {
                        match key {
                            Key::Escape => {
                                // Esc закрывает окно файлов, иначе — сам лаунчер.
                                if st.dialog.is_some() {
                                    st.dialog = None;
                                    ui.focus = None;
                                } else {
                                    return;
                                }
                            }
                            Key::Enter => {
                                if st.dialog.is_some() {
                                    take_dialog_result(&mut st);
                                } else {
                                    new_document(&mut st);
                                }
                            }
                            _ => {
                                if st.dialog.is_none() {
                                    if let Some(c) = char_of(key) {
                                        ui.keys.push(KeyEv::Char(c));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let (mx, my) = window.get_cursor_pos();
        let left = window.get_mouse_button(MouseButtonLeft) == glfw::Action::Press;
        let pressed = left && !prev.0;
        let released = !left && prev.0;
        prev = (left, false);

        ui.begin((mx as f32, my as f32), pressed, released, left, 0.0);
        let mut actions: Vec<Action> = Vec::new();
        build(&mut ui, &mut st, fw as f32, fh as f32, &mut actions);
        if ui.fonts.atlas_rev != atlas_rev {
            renderer.update_atlas(&ui.fonts.atlas, tpaint::text::ATLAS_SIZE, tpaint::text::ATLAS_SIZE);
            atlas_rev = ui.fonts.atlas_rev;
        }
        ui.end();
        renderer.draw(&ui.items, &ui.tris, fw, fh);
        // Снимок окна средствами самого приложения: PrintWindow для OpenGL
        // отдаёт пустую картинку. Используется для проверок интерфейса.
        if std::env::var("TPAINT_LAUNCHER_SHOT").is_ok() && shot_frame > 0 {
            shot_frame -= 1;
            if shot_frame == 0 {
                if let Some(p) = save_framebuffer(fw, fh) {
                    eprintln!("снимок сохранён: {}", p);
                }
            }
        }
        window.swap_buffers();

        if let Some(args) = st.launch.take() {
            let _ = start_editor(&args);
            return;
        }
    }
}

/// Собирает окно лаунчера: заголовок, пресеты, свой размер, недавние файлы.
fn build(ui: &mut Ui, st: &mut LaunchState, w: f32, h: f32, actions: &mut Vec<Action>) {
    ui.background([0.0, 0.0, w, h], theme::BG);
    let lh = ui.line_height(FONT_UI);
    let mut y = 22.0;

    // Заголовок
    ui.text(PAD * 2.0, y, "Tpaint", FONT_UI + 10.0, theme::TEXT, true);
    ui.text(PAD * 2.0, y + 26.0, "Растровый редактор — выберите, с чего начать", FONT_SMALL, theme::TEXT_DIM, false);
    y += 56.0;
    ui.quad([PAD * 2.0, y, w - PAD * 2.0, y + 1.0], theme::BORDER);
    y += 16.0;

    // Новый документ
    ui.text(PAD * 2.0, y, "Новый документ", FONT_UI, theme::TEXT, true);
    y += 24.0;
    for (i, (name, pw, ph)) in PRESETS.iter().enumerate() {
        let br = [PAD * 2.0, y, w - PAD * 2.0, y + 24.0];
        if i == st.preset {
            ui.quad(br, theme::ACCENT_DIM);
        }
        if ui.button(br, name) {
            st.preset = i;
            st.custom_w = *pw as f32;
            st.custom_h = *ph as f32;
        }
        y += 28.0;
    }

    // Свой размер
    y += 6.0;
    let half = (w - PAD * 4.0) / 2.0;
    let mut cw = st.custom_w;
    let mut ch = st.custom_h;
    if ui.value_field([PAD * 2.0, y, PAD * 2.0 + half - 6.0, y + 22.0], "Ширина", &mut cw, 16.0, 8192.0, 10.0) {
        st.custom_w = cw;
    }
    if ui.value_field([PAD * 2.0 + half + 6.0, y, w - PAD * 2.0, y + 22.0], "Высота", &mut ch, 16.0, 8192.0, 10.0) {
        st.custom_h = ch;
    }
    y += 30.0;

    // Кнопки запуска
    let bw = (w - PAD * 4.0 - 12.0) / 3.0;
    if ui.button([PAD * 2.0, y, PAD * 2.0 + bw, y + 30.0], "Создать") {
        new_document(st);
    }
    if ui.button([PAD * 2.0 + bw + 6.0, y, PAD * 2.0 + bw * 2.0 + 6.0, y + 30.0], "Открыть проект…") {
        st.dialog = Some(FileDialog::new(DialogMode::OpenProject, ""));
    }
    if ui.button([PAD * 2.0 + bw * 2.0 + 12.0, y, w - PAD * 2.0, y + 30.0], "Открыть картинку…") {
        st.dialog = Some(FileDialog::new(DialogMode::ImportImage, ""));
    }
    y += 42.0;

    // Недавние файлы
    if !st.recent.is_empty() {
        ui.quad([PAD * 2.0, y, w - PAD * 2.0, y + 1.0], theme::BORDER);
        y += 10.0;
        ui.text(PAD * 2.0, y, "Недавние", FONT_UI, theme::TEXT, true);
        y += 22.0;
        for path in st.recent.iter().take(MAX_RECENT) {
            let br = [PAD * 2.0, y, w - PAD * 2.0, y + 22.0];
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let dir = path
                .parent()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let label = format!("{}   —   {}", name, dir);
            if ui.button(br, &label) {
                st.launch = Some(vec![path.to_string_lossy().into_owned()]);
            }
            y += 26.0;
        }
    }

    // Статус и подсказка
    let sy = h - 34.0;
    ui.quad([0.0, sy - 6.0, w, h], theme::PANEL);
    ui.quad([0.0, sy - 6.0, w, sy - 5.0], theme::BORDER);
    let text = if st.status.is_empty() {
        format!("Новый документ: {}×{}", st.custom_w.round() as i32, st.custom_h.round() as i32)
    } else {
        st.status.clone()
    };
    ui.text(PAD * 2.0, sy + 4.0, &text, FONT_SMALL, theme::TEXT_DIM, false);
    let _ = lh;

    // Окно файлов — то же, что в редакторе.
    if let Some(dlg) = st.dialog.as_mut() {
        file_dialog(ui, dlg, w, h, actions);
    }
}

/// Снимок кадра из заднего буфера — так виден интерфейс лаунчера.
/// Путь берётся из TPAINT_LAUNCHER_SHOT.
fn save_framebuffer(w: i32, h: i32) -> Option<String> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let path = std::env::var("TPAINT_LAUNCHER_SHOT").ok()?;
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    unsafe {
        ::gl::PixelStorei(::gl::PACK_ALIGNMENT, 1);
        ::gl::ReadPixels(0, 0, w, h, ::gl::RGBA, ::gl::UNSIGNED_BYTE, buf.as_mut_ptr() as *mut _);
    }
    let stride = w as usize * 4;
    let mut img = image::RgbaImage::new(w as u32, h as u32);
    for y in 0..h as usize {
        let src = (h as usize - 1 - y) * stride;
        let dst = y * stride;
        img.as_mut()[dst..dst + stride].copy_from_slice(&buf[src..src + stride]);
    }
    match img.save(&path) {
        Ok(()) => Some(path),
        Err(e) => {
            eprintln!("не удалось сохранить снимок: {}", e);
            None
        }
    }
}

/// Запуск редактора с заданными аргументами. Путь ищется рядом с лаунчером,
/// затем в рабочем каталоге и в PATH.
fn start_editor(args: &[String]) -> Result<(), String> {
    let exe = editor_path().ok_or_else(|| "не найден tpaint.exe рядом с лаунчером".to_string())?;
    Command::new(&exe)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("не удалось запустить {:?}: {}", exe, e))
}

/// Путь к tpaint.exe: сперва рядом с лаунчером, потом в текущем каталоге.
fn editor_path() -> Option<PathBuf> {
    let mut dir = std::env::current_exe().ok()?;
    dir.pop();
    let beside = dir.join("tpaint.exe");
    if beside.is_file() {
        return Some(beside);
    }
    if dir.file_name().is_some_and(|n| n == "debug" || n == "release") {
        let up = dir.join("..").join("tpaint.exe");
        if up.is_file() {
            return Some(up.canonicalize().unwrap_or(up));
        }
    }
    let cur = PathBuf::from("tpaint.exe");
    cur.is_file().then_some(cur)
}

/// Список недавних проектов: `%APPDATA%\Tpaint\recent.txt`, по пути в строке.
fn recent_file() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(base).join("Tpaint").join("recent.txt"))
}

fn read_recent() -> Vec<PathBuf> {
    let Some(path) = recent_file() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let mut out: Vec<PathBuf> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect();
    out.dedup();
    out.truncate(MAX_RECENT);
    out
}

fn push_recent(path: &str) {
    let Some(file) = recent_file() else { return };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut list = read_recent();
    list.retain(|p| p.to_string_lossy() != path);
    list.insert(0, PathBuf::from(path));
    list.truncate(MAX_RECENT);
    let text: Vec<String> = list.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let _ = std::fs::write(file, text.join("\n"));
}

/// Запуск нового документа выбранного размера.
fn new_document(st: &mut LaunchState) {
    let w = st.custom_w.round().max(16.0) as u32;
    let h = st.custom_h.round().max(16.0) as u32;
    st.launch = Some(vec!["--new".to_string(), format!("{}x{}", w, h)]);
}

/// Забирает выбранный в окне файл и запускает с ним редактор.
fn take_dialog_result(st: &mut LaunchState) {
    let Some(dlg) = st.dialog.as_mut() else { return };
    if let Some((path, _mode)) = dlg.result.take() {
        push_recent(&path);
        st.launch = Some(vec![path]);
    }
    st.dialog = None;
}

/// Символ по клавише — только для цифр и букв в поле размера.
fn char_of(key: Key) -> Option<char> {
    let c = key as u32;
    // GLFW отдаёт клавиши ASCII-диапазона как их код.
    if (32..127).contains(&c) {
        char::from_u32(c)
    } else {
        None
    }
}

#[link(name = "user32")]
extern "system" {
    fn SetProcessDpiAwarenessContext(value: *mut core::ffi::c_void) -> i32;
}

fn enable_dpi_awareness() {
    let ctx = -4isize as *mut core::ffi::c_void;
    unsafe {
        SetProcessDpiAwarenessContext(ctx);
    }
}
