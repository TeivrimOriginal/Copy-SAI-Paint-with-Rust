//! Компоновка интерфейса в духе Clip Studio: меню, тулбар слева,
//! панель свойств инструмента и слоёв справа, холст по центру, статус внизу.

use crate::app::App;
use crate::doc::BlendMode;
use crate::palette;
use crate::tools::{rows_for, ParamRow, Tool, TOOLS};
use crate::ui::{theme, Icon, KeyEv, Rect, Ui, FONT_SMALL, FONT_UI, PAD};

pub const MENU_H: f32 = 26.0;
pub const TOOLBAR_W: f32 = 46.0;
pub const PANEL_W: f32 = 252.0;
pub const STATUS_H: f32 = 24.0;
pub const LAYERS_H: f32 = 196.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    Quit,
    Undo,
    Redo,
    ClearLayer,
    Fit,
    Zoom100,
    ToggleGrid,
    CanvasSize(usize, usize),
    AddLayer,
    DupLayer,
    DelLayer,
    MergeDown,
    LayerUp,
    LayerDown,
}

const MENUS: &[(&str, &[(&str, Action)])] = &[
    ("Файл", &[
        ("Новый", Action::New),
        ("Открыть…", Action::Open),
        ("Сохранить", Action::Save),
        ("Сохранить как…", Action::SaveAs),
        ("", Action::Quit),
        ("Выход", Action::Quit),
    ]),
    ("Правка", &[
        ("Отменить", Action::Undo),
        ("Повторить", Action::Redo),
        ("", Action::ClearLayer),
        ("Очистить слой", Action::ClearLayer),
    ]),
    ("Вид", &[
        ("Вписать", Action::Fit),
        ("100 %", Action::Zoom100),
        ("", Action::ToggleGrid),
        ("Сетка", Action::ToggleGrid),
        ("", Action::CanvasSize(1920, 1080)),
        ("Холст 1920×1080", Action::CanvasSize(1920, 1080)),
        ("Холст 1280×800", Action::CanvasSize(1280, 800)),
    ]),
    ("Слой", &[
        ("Новый слой", Action::AddLayer),
        ("Дублировать", Action::DupLayer),
        ("Удалить", Action::DelLayer),
        ("", Action::MergeDown),
        ("Объединить вниз", Action::MergeDown),
        ("Выше", Action::LayerUp),
        ("Ниже", Action::LayerDown),
    ]),
];

pub fn build(ui: &mut Ui, app: &mut App, w: i32, h: i32, actions: &mut Vec<Action>) {
    ui.background([0.0, 0.0, w as f32, h as f32], theme::BG);
    let r = app.canvas_rect;
    let right_x = w as f32 - PANEL_W;
    let bottom_y = h as f32 - STATUS_H;

    // --- холст (сначала, чтобы панели перекрывали края) ---
    ui.push_clip(r);
    ui.background(r, theme::CANVAS_BG);
    let tl = app.canvas_to_screen(0.0, 0.0);
    let br = app.canvas_to_screen(app.doc.width as f32, app.doc.height as f32);
    let cr = [tl.0, tl.1, br.0, br.1];
    ui.checker(cr, 16.0);
    ui.canvas_quad(cr, [0.0, 0.0, 1.0, 1.0]);
    // Рамка вокруг документа — тонкая, чтобы не затенять сам холст.
    ui.frame([cr[0] - 1.0, cr[1] - 1.0, cr[2] + 1.0, cr[3] + 1.0], [0.0, 0.0, 0.0, 0.6], 1.0);
    ui.pop_clip();

    // --- меню ---
    menu_bar(ui, app, MENU_H, actions);

    // --- тулбар ---
    toolbar(ui, app, TOOLBAR_W, MENU_H, bottom_y, actions);

    // --- правая панель: свойства + слои ---
    let props_r = [right_x, MENU_H, w as f32, bottom_y - LAYERS_H];
    let layers_r = [right_x, bottom_y - LAYERS_H, w as f32, bottom_y];
    properties(ui, app, props_r);
    layers_panel(ui, app, layers_r, actions);

    // --- статус ---
    status_bar(ui, app, w as f32, bottom_y);

    // --- подсказка при наведении на инструмент ---
    if let Some((tx, ty, text)) = ui.tooltip.clone() {
        let tw = ui.text_width(&text, FONT_SMALL, false) + 12.0;
        ui.quad([tx, ty, tx + tw, ty + 20.0], [0.0, 0.0, 0.0, 0.85]);
        ui.frame([tx, ty, tx + tw, ty + 20.0], theme::BORDER, 1.0);
        let lh = ui.line_height(FONT_SMALL);
        ui.text(tx + 6.0, ty + (20.0 + lh * 0.72) / 2.0, &text, FONT_SMALL, theme::TEXT, false);
    }

    if let Some(dlg) = &mut app.dialog {
        file_dialog(ui, dlg, w as f32, h as f32, actions);
    }
}

fn menu_bar(ui: &mut Ui, app: &App, h: f32, actions: &mut Vec<Action>) {
    ui.quad([0.0, 0.0, 1e5, h], theme::PANEL);
    ui.quad([0.0, h - 1.0, 1e5, h], theme::BORDER);
    let mut x = 6.0;
    for (i, (title, items)) in MENUS.iter().enumerate() {
        let w = ui.text_width(title, FONT_UI, false) + 18.0;
        let r = [x, 0.0, x + w, h];
        let id = 700_000 + i as u32;
        let hot = hover(ui, r);
        if hot && ui.pressed {
            ui.open_menu = if ui.open_menu == Some(id) { None } else { Some(id) };
            ui.consumed_click = true;
        }
        if hot {
            ui.quad(r, theme::PANEL_HI);
        }
        if ui.open_menu == Some(id) {
            ui.quad(r, theme::ACCENT_DIM);
        }
        let lh = ui.line_height(FONT_UI);
        ui.text(r[0] + 9.0, (h + lh * 0.72) / 2.0, title, FONT_UI, theme::TEXT, false);
        // выпадающее меню
        if ui.open_menu == Some(id) {
            let ih = 22.0;
            let mut maxw = 0.0f32;
            for (label, _) in items.iter() {
                maxw = maxw.max(ui.text_width(label, FONT_UI, false));
            }
            let list = [r[0], h, r[0] + maxw + 30.0, h + ih * items.len() as f32];
            ui.quad(list, theme::PANEL);
            ui.frame(list, theme::BORDER, 1.0);
            for (k, (label, act)) in items.iter().enumerate() {
                let ir = [list[0] + 1.0, list[1] + 1.0 + k as f32 * ih, list[2] - 1.0, list[1] + 1.0 + (k + 1) as f32 * ih];
                if hover(ui, ir) && ui.pressed {
                    ui.open_menu = None;
                    ui.consumed_click = true;
                    if !label.is_empty() {
                        actions.push(*act);
                    }
                } else if hover(ui, ir) {
                    ui.quad(ir, theme::PANEL_HI);
                }
                if !label.is_empty() {
                    let lh = ui.line_height(FONT_UI);
                    ui.text(ir[0] + 8.0, ir[1] + (ih + lh * 0.72) / 2.0, label, FONT_UI, theme::TEXT, false);
                } else {
                    ui.quad([ir[0] + 6.0, ir[1] + ih / 2.0, ir[2] - 6.0, ir[1] + ih / 2.0 + 1.0], theme::BORDER);
                }
            }
        }
        x += w;
    }
    // правая часть меню: масштаб холста
    let lh = ui.line_height(FONT_SMALL);
    let zt = format!("Масштаб {}%", (app.zoom * 100.0).round() as i32);
    ui.text_right(1e5 - 10.0, (h + lh * 0.72) / 2.0, &zt, FONT_SMALL, theme::TEXT_DIM, false);
}

fn toolbar(ui: &mut Ui, app: &mut App, w: f32, top: f32, bottom: f32, actions: &mut Vec<Action>) {
    ui.quad([0.0, top, w, bottom], theme::PANEL);
    ui.quad([w - 1.0, top, w, bottom], theme::BORDER);
    let bs = 34.0;
    let mut y = top + 6.0;
    for (i, t) in TOOLS.iter().enumerate() {
        let r = [6.0, y, 6.0 + bs, y + bs];
        let icon = tool_icon(*t);
        if ui.tool_button(r, icon, t.name(), app.tool == *t) {
            app.tool = *t;
        }
        // После карандаша — небольшой отступ, как в Clip Studio.
        y += if i == 0 { bs + 10.0 } else { bs };
    }
    // разделитель и история
    y += 6.0;
    ui.quad([6.0, y, w - 6.0, y + 1.0], theme::BORDER);
    y += 8.0;
    let ur = [6.0, y, 6.0 + bs, y + bs];
    if ui.tool_button(ur, Icon::Undo, "Отменить (Ctrl+Z)", app.history.can_undo()) {
        actions.push(Action::Undo);
    }
    y += bs + 4.0;
    let rr = [6.0, y, 6.0 + bs, y + bs];
    if ui.tool_button(rr, Icon::Redo, "Повторить (Ctrl+Y)", app.history.can_redo()) {
        actions.push(Action::Redo);
    }
}

fn tool_icon(t: Tool) -> Icon {
    match t {
        Tool::Pencil => Icon::Pencil,
        Tool::Brush => Icon::Brush,
        Tool::Eraser => Icon::Eraser,
        Tool::Line => Icon::Line,
        Tool::Rect => Icon::Rect,
        Tool::Ellipse => Icon::Ellipse,
        Tool::Fill => Icon::Fill,
        Tool::Eyedropper => Icon::Eyedropper,
        Tool::Pan => Icon::Pan,
    }
}

fn properties(ui: &mut Ui, app: &mut App, r: Rect) {
    ui.quad(r, theme::PANEL);
    ui.quad([r[0], r[1], r[0] + 1.0, r[3]], theme::BORDER);
    ui.push_clip(r);
    let mut y = r[1] + 6.0;

    // заголовок инструмента
    ui.text(r[0] + PAD, y, app.tool.name(), FONT_UI + 2.0, theme::TEXT, true);
    y += 20.0;
    ui.text(r[0] + PAD, y, app.tool.hint(), FONT_SMALL, theme::TEXT_DIM, false);
    y += 18.0;
    ui.quad([r[0] + PAD, y, r[2] - PAD, y + 1.0], theme::BORDER);
    y += 8.0;

    // параметры инструмента — набор зависит от инструмента
    if app.tool.needs_color() {
        // два цвета + обмен
        let sw = 44.0;
        let c1 = [r[0] + PAD, y, r[0] + PAD + sw, y + sw];
        let (p, s) = (app.primary, app.secondary);
        let (big, small) = if app.color_swapped { (s, p) } else { (p, s) };
        ui.swatch(c1, big);
        let c2 = [c1[0] + 12.0, c1[1] + 12.0, c1[2] - 12.0, c2_bottom(c1, sw)];
        ui.swatch(c2, small);
        let swap_r = [c1[2] + 6.0, c1[1] + 8.0, c1[2] + 24.0, c1[1] + 26.0];
        if ui.tool_button(swap_r, Icon::Swap, "Поменять цвета (X)", false) {
            app.color_swapped = !app.color_swapped;
        }
        y += sw + 8.0;
    }

    for row in rows_for(app.tool) {
        match row {
            ParamRow::Size { label, min, max } => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let mut v = app.params.size;
                if ui.value_field(fr, label, &mut v, min, max, max / 300.0) {
                    app.params.size = v.clamp(min, max);
                }
                // быстрый предпросмотр мазка
                let pr = [r[0] + PAD, y + 22.0, r[2] - PAD, y + 44.0];
                ui.quad(pr, theme::FIELD);
                let c = app.color();
                let col = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0];
                ui.checker(pr, 6.0);
                let rad = (v * 0.5).clamp(1.0, 9.0);
                ui.circle((pr[0] + pr[2]) / 2.0, (pr[1] + pr[3]) / 2.0, rad, col);
                y += 50.0;
            }
            ParamRow::Opacity => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let mut v = app.params.opacity;
                if ui.value_field(fr, "Непрозрачность", &mut v, 0.0, 1.0, 0.005) {
                    app.params.opacity = v.clamp(0.0, 1.0);
                }
                y += 26.0;
            }
            ParamRow::Hardness => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let mut v = app.params.hardness;
                if ui.value_field(fr, "Мягкость края", &mut v, 0.0, 1.0, 0.005) {
                    app.params.hardness = v.clamp(0.0, 1.0);
                }
                y += 26.0;
            }
            ParamRow::Tolerance => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let mut v = app.params.tolerance;
                if ui.value_field(fr, "Допуск", &mut v, 0.0, 255.0, 0.5) {
                    app.params.tolerance = v.clamp(0.0, 255.0);
                }
                y += 26.0;
            }
            ParamRow::Smoothing => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let mut v = app.params.smoothing;
                if ui.value_field(fr, "Сглаживание", &mut v, 0.0, 1.0, 0.005) {
                    app.params.smoothing = v.clamp(0.0, 1.0);
                }
                y += 26.0;
            }
            ParamRow::Checkbox { label, on } => {
                let mut v = app.params.contiguous || app.params.shape_fill;
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                if ui.checkbox(fr, &mut v, label) {
                    if app.tool == Tool::Fill {
                        app.params.contiguous = v;
                    } else {
                        app.params.shape_fill = v;
                    }
                }
                let _ = on;
                y += 26.0;
            }
        }
    }

    // --- цвет ---
    if app.tool.needs_color() {
        y += 4.0;
        ui.quad([r[0] + PAD, y, r[2] - PAD, y + 1.0], theme::BORDER);
        y += 8.0;
        ui.text(r[0] + PAD, y, "Цвет", FONT_UI, theme::TEXT, true);
        y += 18.0;
        let pr = [r[0] + PAD, y, r[2] - PAD, r[3] - 6.0];
        let mut c = app.color();
        let mut hex = app.hex_buf.clone();
        if palette::color_picker(ui, pr, &mut c, &mut hex) {
            app.set_color(c);
        }
        if !hex.is_empty() {
            app.hex_buf = hex;
        }
    }
    ui.pop_clip();
}

fn c2_bottom(c1: Rect, sw: f32) -> f32 {
    c1[1] + sw - 12.0
}

fn layers_panel(ui: &mut Ui, app: &mut App, r: Rect, actions: &mut Vec<Action>) {
    ui.quad(r, theme::PANEL);
    ui.quad([r[0], r[1], r[2], r[1] + 1.0], theme::BORDER);
    let rows_h = r[3] - r[1] - 34.0;
    ui.push_clip([r[0], r[1], r[2], r[1] + rows_h]);

    // Заголовок
    let head = [r[0] + PAD, r[1] + 4.0, r[2] - PAD, r[1] + 20.0];
    ui.text(head[0], head[1], &format!("Слои ({})", app.doc.layers.len()), FONT_UI, theme::TEXT, true);
    let active = app.doc.active;
    let meta = app.doc.layers[active].meta.clone();

    let row_h = 30.0;
    let list_top = r[1] + 26.0;
    // Сверху — верхний слой
    for k in 0..app.doc.layers.len() {
        let idx = app.doc.layers.len() - 1 - k;
        let rr = [r[0] + 4.0, list_top + k as f32 * row_h, r[2] - 4.0, list_top + (k + 1) as f32 * row_h - 2.0];
        if ui.list_row(rr, idx == active, false) {
            app.doc.active = idx;
        }
        let visible = app.doc.layers[idx].meta.visible;
        let non_empty = app.doc.layers[idx].pixels.iter().any(|v| *v != 0);
        let vis_r = [rr[0] + 4.0, rr[1] + 6.0, rr[0] + 20.0, rr[1] + 22.0];
        if ui.tool_button(vis_r, if visible { Icon::Eye } else { Icon::EyeOff }, "Видимость", false) {
            let i = idx;
            app.set_layer_meta(i, |m| m.visible = !m.visible);
        }
        // миниатюра-заглушка: тон активного цвета
        let th = [rr[0] + 24.0, rr[1] + 4.0, rr[0] + 24.0 + 22.0, rr[1] + 4.0 + 22.0];
        ui.checker(th, 6.0);
        if non_empty {
            ui.quad(th, [0.55, 0.58, 0.62, 0.85]);
        }
        ui.frame(th, theme::BORDER, 1.0);
        let name = app.doc.layer_name(idx);
        let lh = ui.line_height(FONT_SMALL);
        let col = if idx == active { theme::TEXT } else { theme::TEXT_DIM };
        ui.text(th[2] + 6.0, rr[1] + (row_h + lh * 0.72) / 2.0 - 2.0, &name, FONT_SMALL, col, idx == active);
        let op = format!("{}%", (app.doc.layers[idx].meta.opacity * 100.0).round() as i32);
        ui.text_right(rr[2] - 6.0, rr[1] + (row_h + lh * 0.72) / 2.0 - 2.0, &op, FONT_SMALL, theme::TEXT_DIM, false);
    }
    ui.pop_clip();

    // Свойства активного слоя
    let py = r[1] + rows_h + 4.0;
    let half = (r[2] - r[0] - PAD * 2.0 - 6.0) / 2.0;
    let op_r = [r[0] + PAD, py, r[0] + PAD + half, py + 20.0];
    let mut op = meta.opacity;
    if ui.value_field(op_r, "Непрозр.", &mut op, 0.0, 1.0, 0.005) {
        let i = active;
        app.set_layer_meta(i, |m| m.opacity = op.clamp(0.0, 1.0));
    }
    let blend_names: Vec<&str> = BlendMode::ALL.iter().map(|b| b.name()).collect();
    let bi = BlendMode::ALL.iter().position(|b| *b == meta.blend).unwrap_or(0);
    let br = [r[0] + PAD + half + 6.0, py, r[2] - PAD, py + 20.0];
    if let Some(n) = ui.dropdown(br, bi, &blend_names) {
        let mode = BlendMode::ALL[n];
        let i = active;
        app.set_layer_meta(i, |m| m.blend = mode);
    }

    // Кнопки управления слоями
    let by = r[3] - 26.0;
    let b = 24.0;
    let bw = (r[2] - r[0] - PAD * 2.0 - 5.0 * 4.0) / 6.0;
    let mk = |i: usize| [r[0] + PAD + (b + 5.0) * i as f32, by, r[0] + PAD + (b + 5.0) * i as f32 + bw, by + 22.0];
    if ui.small_button(mk(0), "+") {
        actions.push(Action::AddLayer);
    }
    if ui.small_button(mk(1), "⧉") {
        actions.push(Action::DupLayer);
    }
    if ui.small_button(mk(2), "▲") {
        actions.push(Action::LayerUp);
    }
    if ui.small_button(mk(3), "▼") {
        actions.push(Action::LayerDown);
    }
    if ui.small_button(mk(4), "▼⇄") {
        actions.push(Action::MergeDown);
    }
    if ui.small_button(mk(5), "✕") {
        actions.push(Action::DelLayer);
    }
    let _ = b;
}

fn status_bar(ui: &mut Ui, app: &mut App, w: f32, y: f32) {
    ui.quad([0.0, y, w, y + STATUS_H], theme::PANEL);
    ui.quad([0.0, y, w, y + 1.0], theme::BORDER);
    let lh = ui.line_height(FONT_SMALL);
    let ty = y + (STATUS_H + lh * 0.72) / 2.0;
    let cx = app.cursor.0 as i32;
    let cy = app.cursor.1 as i32;
    let left = format!(
        "{}  |  {}×{}  |  {}%  |  X:{} Y:{}",
        app.tool.name(),
        app.doc.width,
        app.doc.height,
        (app.zoom * 100.0).round() as i32,
        cx,
        cy
    );
    ui.text(PAD, ty, &left, FONT_SMALL, theme::TEXT_DIM, false);
    let right = match &app.notice {
        Some((msg, t)) if t.elapsed().as_secs_f32() < 4.0 => msg.clone(),
        _ => app.path.clone().unwrap_or_else(|| "Файл не сохранён".to_string()),
    };
    ui.text_right(w - PAD, ty, &right, FONT_SMALL, theme::TEXT_DIM, false);
}

/// Модальное окно работы с файлами — полностью своё, без системных диалогов.
pub fn file_dialog(ui: &mut Ui, dlg: &mut crate::app::FileDialog, w: f32, h: f32, actions: &mut Vec<Action>) {
    ui.quad([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    let dw = 620.0;
    let dh = 400.0;
    let r = [(w - dw) / 2.0, (h - dh) / 2.0, (w + dw) / 2.0, (h + dh) / 2.0];
    ui.quad(r, theme::PANEL);
    ui.frame(r, theme::BORDER, 1.0);
    let title = if dlg.save { "Сохранить как" } else { "Открыть" };
    let lh = ui.line_height(FONT_UI + 2.0);
    ui.text(r[0] + PAD + 4.0, r[1] + 10.0, title, FONT_UI + 2.0, theme::TEXT, true);

    // поле пути
    let pr = [r[0] + PAD, r[1] + 40.0, r[2] - PAD, r[1] + 64.0];
    if hover(ui, pr) && ui.pressed {
        ui.focus = Some(424_242);
    }
    let focused = ui.focus == Some(424_242);
    if focused {
        let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
        for k in keys {
            match k {
                KeyEv::Char(c) => dlg.path.push(c),
                KeyEv::Backspace => {
                    dlg.path.pop();
                }
                KeyEv::Enter => {
                    dlg.scan();
                    ui.focus = None;
                }
                KeyEv::Escape => ui.focus = None,
                _ => {}
            }
        }
    }
    ui.quad(pr, theme::FIELD);
    ui.frame(pr, if focused { theme::ACCENT } else { theme::BORDER }, 1.0);
    let shown = if dlg.path.is_empty() { "C:\\" } else { &dlg.path };
    ui.text(pr[0] + 8.0, pr[1] + (24.0 + lh * 0.72) / 2.0, shown, FONT_SMALL, theme::TEXT, false);
    ui.text(r[0] + PAD, r[1] + 72.0, &format!("{} элементов", dlg.entries.len()), FONT_SMALL, theme::TEXT_DIM, false);

    // список
    let lr = [r[0] + PAD, r[1] + 92.0, r[2] - PAD - 150.0, r[3] - 76.0];
    ui.quad(lr, theme::FIELD);
    ui.frame(lr, theme::BORDER, 1.0);
    ui.push_clip([lr[0] + 1.0, lr[1] + 1.0, lr[2] - 1.0, lr[3] - 1.0]);
    let row_h = 22.0;
    let rows = ((lr[3] - lr[1] - 2.0) / row_h).floor() as usize;
    let first = dlg.scroll.min(dlg.entries.len().saturating_sub(1));
    for i in 0..rows {
        let idx = first + i;
        if idx >= dlg.entries.len() {
            break;
        }
        let (name, is_dir) = dlg.entries[idx].clone();
        let rr = [lr[0] + 1.0, lr[1] + 1.0 + i as f32 * row_h, lr[2] - 1.0, lr[1] + 1.0 + (i + 1) as f32 * row_h];
        if ui.list_row(rr, false, false) {
            if is_dir {
                dlg.enter_dir(&name);
            } else {
                dlg.file = name.clone();
            }
        }
        let icon_c = if is_dir { theme::ACCENT } else { theme::TEXT_DIM };
        let rh = ui.line_height(FONT_SMALL);
        ui.text(rr[0] + 8.0, rr[1] + (row_h + rh * 0.72) / 2.0, &name, FONT_SMALL, icon_c, false);
    }
    ui.pop_clip();
    if hover(ui, lr) {
        let d = ui.wheel * 3.0;
        dlg.scroll = (dlg.scroll as f32 - d).max(0.0) as usize;
    }
    // прокрутка вверх
    let up = [lr[2] - 16.0, lr[1] + 2.0, lr[2] - 2.0, lr[1] + 18.0];
    if ui.small_button(up, "▲") && dlg.scroll > 0 {
        dlg.scroll -= 1;
    }
    let dn = [lr[2] - 16.0, lr[3] - 18.0, lr[2] - 2.0, lr[3] - 2.0];
    if ui.small_button(dn, "▼") && dlg.scroll + 1 < dlg.entries.len() {
        dlg.scroll += 1;
    }

    // имя файла
    let fr = [r[0] + PAD, r[3] - 68.0, r[2] - PAD, r[3] - 44.0];
    if hover(ui, fr) && ui.pressed {
        ui.focus = Some(424_243);
    }
    let ffoc = ui.focus == Some(424_243);
    if ffoc {
        let keys2: Vec<KeyEv> = std::mem::take(&mut ui.keys);
        for k in keys2 {
            match k {
                KeyEv::Char(c) => dlg.file.push(c),
                KeyEv::Backspace => {
                    dlg.file.pop();
                }
                KeyEv::Enter => {
                    dlg.accept();
                }
                KeyEv::Escape => ui.focus = None,
                _ => {}
            }
        }
    }
    ui.quad(fr, theme::FIELD);
    ui.frame(fr, if ffoc { theme::ACCENT } else { theme::BORDER }, 1.0);
    ui.text(fr[0] + 8.0, fr[1] + (24.0 + lh * 0.72) / 2.0, &dlg.file, FONT_SMALL, theme::TEXT, false);

    // кнопки
    let br = [r[2] - PAD - 80.0, r[3] - 36.0, r[2] - PAD, r[3] - 12.0];
    if ui.button(br, if dlg.save { "Сохранить" } else { "Открыть" }) {
        dlg.accept();
        ui.focus = None;
    }
    let cr = [br[0] - 88.0, br[1], br[0] - 8.0, br[3]];
    if ui.button(cr, "Отмена") {
        dlg.close();
    }
    let _ = actions;
}

fn hover(ui: &Ui, r: Rect) -> bool {
    let (mx, my) = ui.mouse;
    mx >= r[0] && mx < r[2] && my >= r[1] && my < r[3]
}

