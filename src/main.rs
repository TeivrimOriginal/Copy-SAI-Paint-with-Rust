//! TPaint — растровый редактор (в духе Clip Studio) на Rust + GLFW + OpenGL.
//!
//! Никаких Win32-контролов: окно создаёт GLFW, интерфейс рисует собственный
//! immediate-mode UI на OpenGL, текст растеризуется системным шрифтом.

mod app;
mod doc;
mod renderer;
mod layout;
mod palette;
mod raster;
mod text;
mod tools;
mod ui;

use app::{App, FileDialog};
use renderer::Renderer;
use glfw::{Action as GAction, Context, Key, MouseButtonLeft, MouseButtonMiddle, MouseButtonRight, WindowEvent};
use layout::Action;
use text::Fonts;
use tools::Tool;
use ui::{KeyEv, Ui};

const WIN_W: u32 = 1400;
const WIN_H: u32 = 900;

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
    glfw.window_hint(glfw::WindowHint::Samples(Some(4)));

    let (mut window, events) = glfw
        .create_window(WIN_W, WIN_H, "TPaint — растровый редактор", glfw::WindowMode::Windowed)
        .expect("не удалось создать окно GLFW");
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
    if let Some(p) = std::env::args().nth(1) {
        if let Err(e) = app.open_png(&p) {
            app.notify(&format!("Не удалось открыть: {}", e));
        }
    }

    // Состояние кнопок мыши: из него считаем нажатия/отпускания.
    let mut prev = (false, false, false);
    let mut alt = false;
    let mut shift = false;
    let mut space = false;
    let mut drawing_left = false;
    let mut drawing_right = false;
    let mut panning = false;
    let mut pan_last = (0.0f32, 0.0f32);
    let mut last_rev = u64::MAX;
    let mut last_atlas_rev = 0u64;
    let mut wheel = 0.0f32;

    'main: while !window.should_close() {
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
                    let ctrl = m.contains(glfw::modifiers::Control);
                    shift = m.contains(glfw::modifiers::Shift);
                    alt = m.contains(glfw::modifiers::Alt);
                    match (key, action) {
                        (Key::Space, GAction::Press) => space = true,
                        (Key::Space, GAction::Release) => space = false,
                        (Key::Escape, GAction::Release) => {
                            app.end_stroke();
                            ui.focus = None;
                            ui.open_menu = None;
                            if app.dialog.is_some() {
                                if let Some(d) = app.dialog.as_mut() {
                                    d.close();
                                }
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
                                    Some(Short::Open) => open_dialog(&mut app),
                                    Some(Short::Save) => save_dialog(&mut app, shift),
                                    Some(Short::SaveAs) => save_dialog(&mut app, true),
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
                                    Some(Short::Tool(t)) => app.tool = t,
                                    Some(Short::Clear) => app.clear_layer(),
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
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
            layout::TOOLBAR_W,
            layout::MENU_H,
            vw - layout::PANEL_W,
            vh - layout::STATUS_H,
        ];
        if app.fit_pending {
            app.fit_view();
            app.fit_pending = false;
        }

        ui.begin(mouse, pressed_l, released_l, left, wheel);
        let mut actions = Vec::new();
        layout::build(&mut ui, &mut app, fw, fh, &mut actions);
        for a in actions {
            apply(&mut app, a);
        }
        let modal = app.dialog.is_some();

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
            } else if inside && alt && pressed_l {
                app.pick(cp);
            } else if inside && pressed_l {
                app.begin_stroke(cp, false);
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
            if (released_l && drawing_left) || (released_r && drawing_right) || (!inside && (left || right)) {
                app.end_stroke();
                drawing_left = false;
                drawing_right = false;
            }
        } else if drawing_left || drawing_right {
            app.end_stroke();
            drawing_left = false;
            drawing_right = false;
        }

        // --- результат диалога файлов ---
        if let Some(d) = app.dialog.as_mut() {
            if let Some((path, save)) = d.result.take() {
                app.dialog = None;
                if !path.is_empty() {
                    let r = if save { app.save_png(&path) } else { app.open_png(&path) };
                    match r {
                        Ok(()) => app.notify(if save { "Файл сохранён" } else { "Файл открыт" }),
                        Err(e) => app.notify(&format!("Ошибка: {}", e)),
                    }
                }
            }
        }

        // --- текстуры ---
        app.doc.ensure_composite();
        if app.doc.shown_rev != last_rev {
            renderer.update_canvas(app.doc.width, app.doc.height, &app.doc.composite);
            last_rev = app.doc.shown_rev;
        }
        if ui.fonts.atlas_rev != last_atlas_rev {
            renderer.update_atlas(&ui.fonts.atlas, text::ATLAS_SIZE, text::ATLAS_SIZE);
            last_atlas_rev = ui.fonts.atlas_rev;
        }

        // --- вывод ---
        unsafe {
            ::gl::ClearColor(0.105, 0.105, 0.117, 1.0);
            ::gl::Clear(::gl::COLOR_BUFFER_BIT);
        }
        renderer.draw(&ui.items, &ui.tris, fw, fh);
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
    Clear,
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
            _ => return None,
        });
    }
    Some(match key {
        X => Short::Swap,
        LeftBracket => Short::Smaller,
        RightBracket => Short::Bigger,
        Delete => Short::Clear,
        P => Short::Tool(Tool::Pencil),
        B => Short::Tool(Tool::Brush),
        E => Short::Tool(Tool::Eraser),
        L => Short::Tool(Tool::Line),
        R => Short::Tool(Tool::Rect),
        O => Short::Tool(Tool::Ellipse),
        F => Short::Tool(Tool::Fill),
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
    let c = match key {
        Backspace => return Some(KeyEv::Backspace),
        Delete => return Some(KeyEv::Delete),
        Enter | KpEnter => return Some(KeyEv::Enter),
        Escape => return Some(KeyEv::Escape),
        Space => ' ',
        A => 'a',
        B => 'b',
        C => 'c',
        D => 'd',
        E => 'e',
        F => 'f',
        G => 'g',
        H => 'h',
        I => 'i',
        J => 'j',
        K => 'k',
        L => 'l',
        M => 'm',
        N => 'n',
        O => 'o',
        P => 'p',
        Q => 'q',
        R => 'r',
        S => 's',
        T => 't',
        U => 'u',
        V => 'v',
        W => 'w',
        X => 'x',
        Y => 'y',
        Z => 'z',
        Num0 => '0',
        Num1 => '1',
        Num2 => '2',
        Num3 => '3',
        Num4 => '4',
        Num5 => '5',
        Num6 => '6',
        Num7 => '7',
        Num8 => '8',
        Num9 => '9',
        Apostrophe => '\'',
        Comma => ',',
        Minus => '-',
        Period => '.',
        Slash => '/',
        Semicolon => ';',
        Equal => '=',
        LeftBracket => '[',
        RightBracket => ']',
        _ => return None,
    };
    Some(KeyEv::Char(c))
}

fn open_dialog(app: &mut App) {
    let dir = app
        .path
        .as_ref()
        .and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_string_lossy().into_owned()))
        .unwrap_or_default();
    app.dialog = Some(FileDialog::new(false, &dir));
}

fn save_dialog(app: &mut App, force: bool) {
    if !force {
        if let Some(p) = app.path.clone() {
            if app.save_png(&p).is_ok() {
                app.notify("Файл сохранён");
                return;
            }
        }
    }
    let dir = app
        .path
        .as_ref()
        .and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_string_lossy().into_owned()))
        .unwrap_or_default();
    app.dialog = Some(FileDialog::new(true, &dir));
}

fn apply(app: &mut App, a: Action) {
    match a {
        Action::New => app.new_document(app.doc.width, app.doc.height),
        Action::Open => open_dialog(app),
        Action::Save => save_dialog(app, false),
        Action::SaveAs => save_dialog(app, true),
        Action::Quit => {}
        Action::Undo => app.undo(),
        Action::Redo => app.redo(),
        Action::ClearLayer => app.clear_layer(),
        Action::Fit => app.fit_pending = true,
        Action::Zoom100 => {
            app.zoom = 1.0;
            app.fit_pending = true;
        }
        Action::ToggleGrid => app.show_grid = !app.show_grid,
        Action::CanvasSize(w, h) => app.resize_canvas(w, h),
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

