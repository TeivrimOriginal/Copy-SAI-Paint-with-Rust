//! Tpaint — растровый редактор (в духе Clip Studio) на Rust + GLFW + OpenGL.
//!
//! Никаких Win32-контролов: окно создаёт GLFW, интерфейс рисует собственный
//! immediate-mode UI на OpenGL, текст растеризуется системным шрифтом.

use tpaint::app::{App, CanvasOp, DialogMode, FileDialog, PathKind};
use tpaint::layout::Action;
use tpaint::renderer::Renderer;
use tpaint::text::Fonts;
use tpaint::tools::Tool;
use tpaint::ui::{KeyEv, Ui};
use glfw::{Action as GAction, Context, Key, MouseButtonLeft, MouseButtonMiddle, MouseButtonRight, WindowEvent};

const WIN_W: u32 = 1400;
const WIN_H: u32 = 900;

/// Размер холста из строки вида `1920x1080` (регистр и пробелы не важны).
fn parse_size(s: &str) -> Option<(usize, usize)> {
    let t = s.trim().to_lowercase().replace(' ', "");
    let (w, h) = t.split_once(['x', '×', '*'])?;
    let w: usize = w.trim().parse().ok()?;
    let h: usize = h.trim().parse().ok()?;
    if (16..=16384).contains(&w) && (16..=16384).contains(&h) {
        Some((w, h))
    } else {
        None
    }
}

/// Снимок кадра средствами OpenGL: читаем задний буфер прямо из приложения.
/// Системный PrintWindow для окон с OpenGL часто отдаёт пустую картинку,
/// поэтому для проверок интерфейса используем этот путь: клавиша F12 либо
/// переменная окружения TPAINT_SHOT (тогда кадр снимается сам при простое).
fn save_framebuffer(w: i32, h: i32) -> Option<String> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let path = std::env::var("TPAINT_SHOT")
        .ok()
        .unwrap_or_else(|| "tpaint_shot.png".to_string());
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    unsafe {
        ::gl::PixelStorei(::gl::PACK_ALIGNMENT, 1);
        ::gl::ReadPixels(0, 0, w, h, ::gl::RGBA, ::gl::UNSIGNED_BYTE, buf.as_mut_ptr() as *mut _);
    }
    // В OpenGL строки идут снизу вверх — переворачиваем.
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
            eprintln!("Не удалось сохранить снимок: {}", e);
            None
        }
    }
}

#[link(name = "user32")]
extern "system" {
    /// DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
    fn SetProcessDpiAwarenessContext(value: *mut core::ffi::c_void) -> i32;
}

/// Без этого окно не-DPI-осведомлённое: размер кадрового буфера не совпадает
/// с клиентской областью, нижняя часть интерфейса обрезается, а координаты
/// мыши не совпадают с той системой, в которой мы рисуем.
fn enable_dpi_awareness() {
    let ctx = -4isize as *mut core::ffi::c_void;
    unsafe {
        SetProcessDpiAwarenessContext(ctx);
    }
}

fn main() {
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
    glfw.window_hint(glfw::WindowHint::Resizable(true));
    // MSAA выключен намеренно: он сглаживает края квадов глифов, и мелкий
    // текст превращается в мыло. Панели и холст состоят из прямоугольников,
    // а круги кисти рисуются сглаженной в шейдере альфой.
    glfw.window_hint(glfw::WindowHint::Samples(None));

    let (mut window, events) = glfw
        .create_window(WIN_W, WIN_H, "Tpaint — растровый редактор", glfw::WindowMode::Windowed)
        .expect("не удалось создать окно GLFW");
    // Без этого события (клавиши, символы, колесо, закрытие и ресайз окна)
    // не доходят до приложения: в glfw-rs 0.17 callbacks ставятся явно.
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
    let mut app = App::new();
    // Командная строка: путь к файлу или `--new 1920x1080` (так запускает лаунчер).
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--new" {
            if let Some(spec) = args.get(i + 1) {
                if let Some((w, h)) = parse_size(spec) {
                    app.new_document(w, h);
                } else {
                    app.notify(&format!("Не понял размер: {}", spec));
                }
                i += 1;
            }
        } else if !a.starts_with("--") {
            // Проект .tpaint открывается целиком, остальное — как картинка.
            let r = if a.to_lowercase().ends_with(tpaint::project::EXT_DOT) {
                app.open_project(a)
            } else {
                app.open_png(a)
            };
            if let Err(e) = r {
                app.notify(&format!("Не удалось открыть: {}", e));
            }
        }
        i += 1;
    }

    // Состояние кнопок мыши: из него считаем нажатия/отпускания.
    let mut prev = (false, false, false);
    let mut alt = false;
    let mut shift = false;
    let mut ctrl_held = false;
    let mut space = false;
    let mut drawing_left = false;
    let mut drawing_right = false;
    let mut panning = false;
    let mut pan_last = (0.0f32, 0.0f32);
    // Тянем ли рамку свободной трансформации.
    let mut transform_dragging = false;
    let mut last_rev = u64::MAX;
    let mut last_atlas_rev = 0u64;
    let mut last_thumb_rev = u64::MAX;
    let mut last_sel_rev = u64::MAX;
    let mut wheel = 0.0f32;
    let mut last_t = std::time::Instant::now();
    // Снимок кадра: F12 — сразу, переменная TPAINT_SHOT — после первого ввода
    // и полутора секунд простоя (так кадр снимается без клавиатуры).
    let shot_env = std::env::var("TPAINT_SHOT").is_ok();
    let mut shot_requested = shot_env;
    let mut shot_now = false;
    let mut shot_armed = false;
    let mut idle = 0.0f32;
    // None на первом кадре: иначе «движение» засчиталось бы сразу при старте.
    let mut last_mouse: Option<(f32, f32)> = None;

    'main: while !window.should_close() {
        let now = std::time::Instant::now();
        let dt = (now.duration_since(last_t).as_secs_f32()).min(0.1);
        last_t = now;
        // --- события окна: клавиши, колесо, размер, закрытие ---
        glfw.poll_events();
        while let Ok((_, ev)) = events.try_recv() {
            match ev {
                WindowEvent::FramebufferSize(w, h) => {
                    if w > 0 && h > 0 {
                        fw = w;
                        fh = h;
                    }
                }
                WindowEvent::Scroll(_, dy) => wheel += dy as f32,
                WindowEvent::Close => break 'main,
                WindowEvent::Key(key, _sc, action, m) => {
                    if action == GAction::Press {
                        shot_armed = true;
                    }
                    let ctrl = m.contains(glfw::modifiers::Control);
                    ctrl_held = ctrl;
                    shift = m.contains(glfw::modifiers::Shift);
                    alt = m.contains(glfw::modifiers::Alt);
                    match (key, action) {
                        (Key::Space, GAction::Press) => space = true,
                        (Key::Space, GAction::Release) => space = false,
                        // F12 — снимок окна в PNG (для проверки интерфейса)
                        (Key::F12, GAction::Press) => {
                            shot_armed = true;
                            shot_requested = true;
                            shot_now = true;
                        }
                        (Key::Escape, GAction::Release) => {
                            app.end_stroke();
                            // Во время свободной трансформации Esc отменяет
                            // только её, не трогая выделение.
                            if app.transform.is_some() {
                                app.transform_cancel();
                            } else {
                                app.deselect();
                            }
                            app.cancel_text();
                            ui.focus = None;
                            ui.open_menu = None;
                            if app.dialog.is_some() {
                                if let Some(d) = app.dialog.as_mut() {
                                    d.close();
                                }
                            }
                        }
                        (Key::Enter, GAction::Press) | (Key::KpEnter, GAction::Press) => {
                            // Enter фиксирует набранный текст, а не дублирует символ.
                            if app.text.is_some() {
                                app.commit_text();
                            } else if app.transform.is_some() {
                                app.transform_commit();
                                app.tool = Tool::Brush;
                            } else if app.poly_active {
                                // Замыкаем контур многоугольника.
                                app.finish_polygon();
                            } else {
                                ui.keys.push(KeyEv::Enter);
                            }
                        }
                        (_, GAction::Press) | (_, GAction::Repeat) => {
                            if let Some(t) = key_to_text(key, ctrl) {
                                ui.keys.push(t);
                            }
                            if action == GAction::Press {
                                match shortcut(key, ctrl, shift) {
                                    Some(Short::Undo) => app.undo(),
                                    Some(Short::Redo) => app.redo(),
                                    Some(Short::New) => app.new_document(app.doc.width, app.doc.height),
                                    Some(Short::Open) => {
                                        if shift {
                                            file_dialog(&mut app, DialogMode::ImportImage)
                                        } else {
                                            file_dialog(&mut app, DialogMode::OpenProject)
                                        }
                                    }
                                    Some(Short::Save) => save_now(&mut app),
                                    Some(Short::SaveAs) => file_dialog(&mut app, DialogMode::SaveProject),
                                    Some(Short::Fit) => app.fit_pending = true,
                                    Some(Short::Zoom100) => {
                                        app.zoom = 1.0;
                                        app.fit_pending = true;
                                    }
                                    Some(Short::ZoomIn) => app.zoom_by(1.2),
                                    Some(Short::ZoomOut) => app.zoom_by(1.0 / 1.2),
                                    Some(Short::Swap) => app.color_swapped = !app.color_swapped,
                                    Some(Short::Smaller) => {
                                        app.params.size = (app.params.size - 2.0).max(1.0)
                                    }
                                    Some(Short::Bigger) => app.params.size = (app.params.size + 2.0).min(800.0),
                                    Some(Short::Tool(t)) => {
                                        // Незакрытый контур многоугольника теряет смысл.
                                        app.cancel_polygon();
                                        app.tool = t;
                                    }
                                    Some(Short::SelectAll) => app.select_all(),
                                    Some(Short::Deselect) => app.deselect(),
                                    Some(Short::Copy) => app.copy_selection(),
                                    Some(Short::Cut) => app.cut_selection(),
                                    Some(Short::Paste) => app.paste_selection(),
                                    Some(Short::FreeTransform) => app.begin_transform(),
                                    Some(Short::DeleteSel) => {
                                        if app.selection.is_some() {
                                            app.delete_selection()
                                        } else {
                                            app.clear_layer()
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // Символы приходят отдельным событием — так работает и кириллица.
                WindowEvent::Char(c) => {
                    if !ctrl_held && !alt {
                        if c != '\r' && c != '\n' {
                            ui.keys.push(KeyEv::Char(c));
                        }
                    }
                }
                _ => {}
            }
        }
        let (mw, mh) = window.get_framebuffer_size();
        if mw > 0 && mh > 0 && (mw != fw || mh != fh) {
            fw = mw;
            fh = mh;
        }

        // --- мышь ---
        let (mx, my) = window.get_cursor_pos();
        let mouse = (mx as f32, my as f32);
        let left = window.get_mouse_button(MouseButtonLeft) == GAction::Press;
        let right = window.get_mouse_button(MouseButtonRight) == GAction::Press;
        let middle = window.get_mouse_button(MouseButtonMiddle) == GAction::Press;
        let pressed_l = left && !prev.0;
        let released_l = !left && prev.0;
        let pressed_r = right && !prev.1;
        let released_r = !right && prev.1;
        let pressed_m = middle && !prev.2;
        let released_m = !middle && prev.2;
        prev = (left, right, middle);

        // --- кадр ---
        let vw = fw as f32;
        let vh = fh as f32;
        app.canvas_rect = [
            tpaint::layout::TOOLBAR_W,
            tpaint::layout::MENU_H,
            vw - tpaint::layout::PANEL_W,
            vh - tpaint::layout::STATUS_H,
        ];
        if app.fit_pending {
            app.fit_view();
            app.fit_pending = false;
        }

        ui.begin(mouse, pressed_l, released_l, left, wheel);
        let mut actions = Vec::new();
        tpaint::layout::build(&mut ui, &mut app, fw, fh, &mut actions);
        for a in actions {
            apply(&mut app, a);
        }
        // Открытое меню, как и файловый диалог, не пропускает клики в холст.
        let modal = app.dialog.is_some() || ui.open_menu.is_some();

        // --- рисование (если интерфейс не съел клик) ---
        if !ui.consumed_click && !modal {
            if wheel != 0.0 {
                if shift {
                    app.params.size = (app.params.size * (1.0 + wheel * 0.1)).clamp(1.0, 800.0);
                } else if app.in_canvas(mouse) {
                    app.zoom_at(mouse, 1.0 + wheel * 0.12);
                }
            }
            let cp = app.screen_to_canvas(mouse.0, mouse.1);
            app.cursor = cp;
            let inside = app.in_canvas(mouse);
            app.cursor_screen = mouse;
            app.cursor_in_canvas = inside
                && !matches!(app.tool, tpaint::tools::Tool::Eyedropper | tpaint::tools::Tool::Pan)
                && !app.tool.is_selection();

            // панорамирование: средняя кнопка, пробел+левая, инструмент «Рука»
            if pressed_m || (space && pressed_l) || (app.tool == Tool::Pan && pressed_l) {
                panning = true;
                pan_last = mouse;
            }
            if panning && (middle || (space && left) || (app.tool == Tool::Pan && left)) {
                app.pan.0 += mouse.0 - pan_last.0;
                app.pan.1 += mouse.1 - pan_last.1;
                pan_last = mouse;
            }
            if released_m || (!left && !middle) {
                panning = false;
            }
            if panning {
                // пока панорамируем — мазок не рисуем
            } else if app.transform.is_some() {
                // Свободная трансформация: хватаем ручку, рамку или поворот.
                if inside && (left || right) {
                    if pressed_l || pressed_r {
                        let grab = app.transform_grab_at(mouse);
                        if grab != tpaint::app::TransformGrab::None {
                            app.transform_grab_begin(cp, grab);
                            transform_dragging = true;
                        }
                    }
                    if transform_dragging {
                        app.transform_drag(cp, app.transform.as_ref().map(|t| t.grab).unwrap_or(tpaint::app::TransformGrab::None));
                    }
                    if released_l || released_r {
                        app.transform_grab_end();
                        transform_dragging = false;
                    }
                }
            } else if inside && alt && pressed_l {
                app.pick(cp);
            } else if inside && pressed_l {
                // Пока набирается текст, щелчок по холсту его фиксирует.
                if app.text.is_some() {
                    app.commit_text();
                } else {
                    app.begin_stroke(cp, false);
                }
                drawing_left = true;
            } else if inside && pressed_r {
                app.begin_stroke(cp, true);
                drawing_right = true;
            } else if inside && (left || right) {
                if drawing_left {
                    app.move_stroke(cp, false);
                } else if drawing_right {
                    app.move_stroke(cp, true);
                }
            }
            // Выделение: рамка тянется сама по кнопке, а плавающий фрагмент
            // следует за курсором даже без зажатой кнопки.
            if inside && (drawing_left || drawing_right) {
                app.move_select(cp);
            } else if app.floating.is_some() && app.sel_drag == tpaint::app::SelDrag::None {
                app.move_select(cp);
            }
            if (released_l && drawing_left) || (released_r && drawing_right) || (!inside && (left || right)) {
                app.end_stroke();
                app.end_select();
                drawing_left = false;
                drawing_right = false;
            }
        } else if drawing_left || drawing_right {
            app.end_stroke();
            app.end_select();
            drawing_left = false;
            drawing_right = false;
        }

        // --- результат диалога файлов ---
        if let Some(d) = app.dialog.as_mut() {
            if let Some((path, mode)) = d.result.take() {
                app.dialog = None;
                if !path.is_empty() {
                    // Ошибки показываем в статус-баре, окно уже закрыто.
                    let r = match mode {
                        DialogMode::OpenProject => app.open_project(&path),
                        DialogMode::SaveProject => app.save_project(&path),
                        DialogMode::ImportImage => app.import_layer(&path),
                        DialogMode::ExportPng => app.export_png(&path, false),
                        DialogMode::ExportPngAlpha => app.export_png(&path, true),
                        DialogMode::ExportJpeg => app.export_jpeg(&path, 92),
                    };
                    if let Err(e) = r {
                        app.notify(&format!("Ошибка: {}", e));
                    }
                }
            }
        }

        // --- текстуры ---
        // Пока идёт свободная трансформация, слой перерисовывается по рамке.
        app.transform_preview();
        app.doc.ensure_composite();
        if app.doc.shown_rev != last_rev {
            renderer.update_canvas(app.doc.width, app.doc.height, &app.doc.composite);
            last_rev = app.doc.shown_rev;
        }
        if ui.fonts.atlas_rev != last_atlas_rev {
            renderer.update_atlas(&ui.fonts.atlas, tpaint::text::ATLAS_SIZE, tpaint::text::ATLAS_SIZE);
            last_atlas_rev = ui.fonts.atlas_rev;
        }
        // Маска выделения — перезаливается только при изменении.
        if app.sel_mask_rev != last_sel_rev {
            match app.sel_mask.as_ref() {
                Some(m) => renderer.update_sel(app.doc.width, app.doc.height, m),
                None => {
                    // Без выделения заливаем белым: тогда «снаружи» пусто.
                    renderer.update_sel(1, 1, &[255]);
                }
            }
            last_sel_rev = app.sel_mask_rev;
        }

        // Миниатюры слоёв пересобираем только когда изменился холст.
        if app.doc.shown_rev != last_thumb_rev {
            let atlas = app.build_thumb_atlas();
            renderer.update_thumbs(&atlas);
            let masks = app.build_mask_atlas();
            renderer.update_masks(&masks);
            last_thumb_rev = app.doc.shown_rev;
        }
        // Плавающий фрагмент после вставки — своя текстура поверх холста.
        if app.float_dirty || app.floating.as_ref().is_some_and(|f| f.data.width() == 0) {
            match app.floating.as_ref() {
                Some(f) if f.data.width() > 0 => {
                    renderer.update_float(f.data.width(), f.data.height(), &f.data.pixels)
                }
                _ => renderer.update_float(1, 1, &[0, 0, 0, 0]),
            }
            app.float_dirty = false;
        }
        // Анимация «муравьиных дорожек» идёт, пока есть выделение.
        if app.selection.is_some() {
            app.ants_phase = (app.ants_phase + dt * 14.0).rem_euclid(10.0);
        }

        // --- вывод ---
        unsafe {
            ::gl::ClearColor(0.105, 0.105, 0.117, 1.0);
            ::gl::Clear(::gl::COLOR_BUFFER_BIT);
        }
        renderer.draw(&ui.items, &ui.tris, fw, fh);
        // Снимок кадра читаем до обмена буферов: задний буфер и есть кадр.
        let moved = match last_mouse {
            None => false,
            Some(p) => (mouse.0 - p.0).abs() > 0.5 || (mouse.1 - p.1).abs() > 0.5 || left || right || middle,
        };
        last_mouse = Some(mouse);
        if moved {
            shot_armed = true;
            idle = 0.0;
        } else {
            idle += dt;
        }
        if shot_requested && (shot_now || (shot_armed && idle > 1.5)) {
            shot_requested = false;
            shot_now = false;
            if let Some(p) = save_framebuffer(fw, fh) {
                eprintln!("снимок сохранён: {}", p);
            }
        }
        window.swap_buffers();

        ui.end();
        wheel = 0.0;
    }
}

enum Short {
    Undo,
    Redo,
    New,
    Open,
    Save,
    SaveAs,
    Fit,
    ZoomIn,
    ZoomOut,
    Zoom100,
    Swap,
    Smaller,
    Bigger,
    Tool(Tool),
    SelectAll,
    Deselect,
    Copy,
    Cut,
    Paste,
    DeleteSel,
    FreeTransform,
}

fn shortcut(key: Key, ctrl: bool, shift: bool) -> Option<Short> {
    use glfw::Key::*;
    if ctrl {
        return Some(match key {
            Z => Short::Undo,
            Y => Short::Redo,
            N => Short::New,
            O => Short::Open,
            S if shift => Short::SaveAs,
            S => Short::Save,
            Num0 => Short::Fit,
            Num1 => Short::Zoom100,
            Equal | KpAdd => Short::ZoomIn,
            Minus | KpSubtract => Short::ZoomOut,
            A => Short::SelectAll,
            C => Short::Copy,
            X => Short::Cut,
            V => Short::Paste,
            T => Short::FreeTransform,
            _ => return None,
        });
    }
    Some(match key {
        X => Short::Swap,
        LeftBracket => Short::Smaller,
        RightBracket => Short::Bigger,
        Delete | Backspace => Short::DeleteSel,
        Escape => Short::Deselect,
        P => Short::Tool(Tool::Pencil),
        B => Short::Tool(Tool::Brush),
        E => Short::Tool(Tool::Eraser),
        L => Short::Tool(Tool::Line),
        R => Short::Tool(Tool::Rect),
        O => Short::Tool(Tool::Ellipse),
        F => Short::Tool(Tool::Fill),
        D => Short::Tool(Tool::Gradient),
        M => Short::Tool(Tool::Select),
        A => Short::Tool(Tool::EllipseSelect),
        W => Short::Tool(Tool::Wand),
        G => Short::Tool(Tool::Polygon),
        T => Short::Tool(Tool::Text),
        I => Short::Tool(Tool::Eyedropper),
        H => Short::Tool(Tool::Pan),
        _ => return None,
    })
}

fn key_to_text(key: Key, ctrl: bool) -> Option<KeyEv> {
    use glfw::Key::*;
    if ctrl {
        return None;
    }
    // Символы приходят событием Char (см. выше), здесь только служебные клавиши.
    match key {
        Backspace => Some(KeyEv::Backspace),
        Delete => Some(KeyEv::Delete),
        Enter | KpEnter => Some(KeyEv::Enter),
        Escape => Some(KeyEv::Escape),
        _ => None,
    }
}

/// Каталог, который логично открыть первой: папка текущего файла.
fn current_dir(app: &App) -> String {
    app.path
        .as_ref()
        .and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// Открывает своё окно файлов в нужном режиме.
fn file_dialog(app: &mut App, mode: DialogMode) {
    app.dialog = Some(FileDialog::new(mode, &current_dir(app)));
}

/// Ctrl+S: сохраняет в текущий файл, а если его нет — открывает окно.
/// Проект сохраняется целиком со слоями, картинка — плоским PNG.
fn save_now(app: &mut App) {
    let Some(p) = app.path.clone() else {
        file_dialog(app, DialogMode::SaveProject);
        return;
    };
    let r = match app.path_kind {
        PathKind::Project => app.save_project(&p),
        PathKind::Image => app.save_png(&p),
    };
    match r {
        Ok(()) => app.notify("Файл сохранён"),
        Err(e) => {
            app.notify(&format!("Ошибка: {}", e));
            // Файл недоступен — даём выбрать другой.
            file_dialog(app, DialogMode::SaveProject);
        }
    }
}

fn apply(app: &mut App, a: Action) {
    match a {
        Action::New => app.new_document(app.doc.width, app.doc.height),
        Action::Open => file_dialog(app, DialogMode::OpenProject),
        Action::ImportImage => file_dialog(app, DialogMode::ImportImage),
        Action::Save => save_now(app),
        Action::SaveAs => file_dialog(app, DialogMode::SaveProject),
        Action::ExportPng => file_dialog(app, DialogMode::ExportPng),
        Action::ExportPngAlpha => file_dialog(app, DialogMode::ExportPngAlpha),
        Action::ExportJpeg => file_dialog(app, DialogMode::ExportJpeg),
        Action::Quit => {}
        Action::Undo => app.undo(),
        Action::Redo => app.redo(),
        Action::ClearLayer => app.clear_layer(),
        Action::SelectAll => app.select_all(),
        Action::InvertSel => app.invert_selection(),
        Action::FeatherSel => app.feather_selection(6.0),
        Action::GrowSel => app.grow_selection(2.0),
        Action::ShrinkSel => app.grow_selection(-2.0),
        Action::Deselect => app.deselect(),
        Action::Copy => app.copy_selection(),
        Action::Cut => app.cut_selection(),
        Action::Paste => app.paste_selection(),
        Action::DeleteSel => {
            if app.selection.is_some() {
                app.delete_selection()
            } else {
                app.clear_layer()
            }
        }
        Action::Fit => app.fit_pending = true,
        Action::Zoom100 => {
            app.zoom = 1.0;
            app.fit_pending = true;
        }
        Action::ToggleGrid => app.show_grid = !app.show_grid,
        Action::GridSize(n) => {
            app.grid_size = n as f32;
            app.show_grid = true;
        }
        Action::ToggleRulers => app.show_rulers = !app.show_rulers,
    Action::ToggleNav => {
        app.show_nav = !app.show_nav;
        app.notify(if app.show_nav { "Навигатор включён" } else { "Навигатор выключен" });
    }
        Action::CenterGuides => {
            app.guides.push((app.doc.width as f32 / 2.0, false));
            app.guides.push((app.doc.height as f32 / 2.0, true));
            app.notify("Направляющие по центру холста");
        }
        Action::ClearGuides => {
            app.guides.clear();
            app.notify("Направляющие убраны");
        }
        Action::CanvasSize(w, h) => app.resize_canvas(w, h),
        Action::CanvasSizeDialog => {
            app.size_w = app.doc.width as f32;
            app.size_h = app.doc.height as f32;
            app.size_dialog = true;
        }
        Action::CropToSelection => app.crop_to_selection(),
        Action::TrimCanvas => app.trim_canvas(),
        Action::Rotate90 => app.transform_canvas(CanvasOp::Rotate90),
        Action::Rotate180 => app.transform_canvas(CanvasOp::Rotate180),
        Action::Rotate270 => app.transform_canvas(CanvasOp::Rotate270),
        Action::MirrorCanvasH => app.transform_canvas(CanvasOp::MirrorH),
        Action::MirrorCanvasV => app.transform_canvas(CanvasOp::MirrorV),
        Action::FlipLayerH => app.flip_layer(true),
        Action::FlipLayerV => app.flip_layer(false),
        Action::OpenFilters => app.open_filters(),
        Action::FreeTransform => app.begin_transform(),
        Action::AddMask => app.add_mask_from_selection(),
        Action::MaskWhite => app.fill_mask(true),
        Action::MaskBlack => app.fill_mask(false),
        Action::InvertMask => app.invert_mask(),
        Action::ToggleMask => app.toggle_mask(),
        Action::EditMask => {
            let on = !app.edit_mask;
            app.set_edit_mask(on);
        }
        Action::DeleteMask => app.delete_mask(),
        Action::MakeGroup => app.make_group(),
        Action::Ungroup => app.ungroup(),
        Action::ClipLayer => app.toggle_clip(),
        Action::OpenFx => app.open_fx(),
        Action::Shadow => app.toggle_fx_shadow(),
        Action::OutlineFx => app.toggle_fx_outline(),
        Action::LayerTag => app.cycle_tag(),
        Action::AddLayer => app.add_layer(),
        Action::DupLayer => app.duplicate_layer(),
        Action::DelLayer => app.delete_layer(),
        Action::MergeDown => app.merge_down(),
        Action::LayerUp => {
            let a = app.doc.active;
            if a + 1 < app.doc.layers.len() {
                app.move_layer(a + 1);
            }
        }
        Action::LayerDown => {
            let a = app.doc.active;
            if a > 0 {
                app.move_layer(a - 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn size_argument_is_parsed_in_any_spelling() {
        assert_eq!(parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_size("  800 X 600 "), Some((800, 600)));
        assert_eq!(parse_size("1024×768"), Some((1024, 768)));
    }

    #[test]
    fn wrong_size_argument_is_rejected() {
        // Мусор, нули и неправдоподобные размеры отклоняются, а не падают.
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("abc"), None);
        assert_eq!(parse_size("1920"), None);
        assert_eq!(parse_size("0x100"), None);
        assert_eq!(parse_size("99999x10"), None);
    }
}

