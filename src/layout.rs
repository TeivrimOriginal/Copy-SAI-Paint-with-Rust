//! Компоновка интерфейса в духе Clip Studio: меню, тулбар слева,
//! панель свойств инструмента и слоёв справа, холст по центру, статус внизу.

use crate::app::App;
use crate::doc::BlendMode;
use crate::palette;
use crate::renderer::UNIT_FLOAT;
use crate::tools::{rows_for, ParamRow, Tool, PRESETS, TOOLS};
use crate::ui::{theme, Icon, KeyEv, Rect, Ui, FONT_SMALL, FONT_UI, PAD};

pub const MENU_H: f32 = 26.0;
pub const TOOLBAR_W: f32 = 46.0;
pub const PANEL_W: f32 = 252.0;
pub const STATUS_H: f32 = 24.0;
pub const LAYERS_H: f32 = 196.0;
/// Высота полосы истории под холстом.
pub const HISTORY_H: f32 = 44.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    New,
    Open,
    ImportImage,
    Save,
    SaveAs,
    ExportPng,
    ExportPngAlpha,
    ExportJpeg,
    Quit,
    Undo,
    Redo,
    ClearLayer,
    SelectAll,
    Deselect,
    Copy,
    Cut,
    Paste,
    DeleteSel,
    Fit,
    Zoom100,
    ToggleGrid,
    GridSize(u32),
    ToggleRulers,
    CenterGuides,
    ClearGuides,
    CanvasSize(usize, usize),
    AddLayer,
    DupLayer,
    DelLayer,
    MergeDown,
    LayerUp,
    LayerDown,
    OpenFilters,
    FlipLayerH,
    FlipLayerV,
    CropToSelection,
    TrimCanvas,
    CanvasSizeDialog,
    FreeTransform,
    AddMask,
    MaskWhite,
    MaskBlack,
    InvertMask,
    ToggleMask,
    EditMask,
    DeleteMask,
    MakeGroup,
    Ungroup,
    Rotate90,
    Rotate180,
    Rotate270,
    MirrorCanvasH,
    MirrorCanvasV,
}

const MENUS: &[(&str, &[(&str, Action)])] = &[
    ("Файл", &[
        ("Новый", Action::New),
        ("", Action::Open),
        ("Открыть проект…\tCtrl+O", Action::Open),
        ("Импорт изображения…\tCtrl+Shift+O", Action::ImportImage),
        ("", Action::Save),
        ("Сохранить\tCtrl+S", Action::Save),
        ("Сохранить как…\tCtrl+Shift+S", Action::SaveAs),
        ("", Action::ExportPng),
        ("Экспорт в PNG…", Action::ExportPng),
        ("Экспорт в PNG (прозрачный)…", Action::ExportPngAlpha),
        ("Экспорт в JPEG…", Action::ExportJpeg),
        ("", Action::Quit),
        ("Выход", Action::Quit),
    ]),
    ("Правка", &[
        ("Отменить", Action::Undo),
        ("Повторить", Action::Redo),
        ("", Action::SelectAll),
        ("Выделить всё\tCtrl+A", Action::SelectAll),
        ("Снять выделение\tEsc", Action::Deselect),
        ("", Action::Copy),
        ("Копировать\tCtrl+C", Action::Copy),
        ("Вырезать\tCtrl+X", Action::Cut),
        ("Вставить\tCtrl+V", Action::Paste),
        ("Удалить\tDel", Action::DeleteSel),
        ("", Action::FreeTransform),
        ("Свободная трансформация\tCtrl+T", Action::FreeTransform),
        ("", Action::ClearLayer),
        ("Очистить слой", Action::ClearLayer),
    ]),
    ("Фильтры", &[
        ("Коррекция слоя…", Action::OpenFilters),
        ("", Action::FlipLayerH),
        ("Отразить слева направо", Action::FlipLayerH),
        ("Отразить сверху вниз", Action::FlipLayerV),
    ]),
    ("Вид", &[
        ("Вписать", Action::Fit),
        ("100 %", Action::Zoom100),
        ("", Action::ToggleGrid),
        ("Сетка", Action::ToggleGrid),
        ("Шаг сетки: 16", Action::GridSize(16)),
        ("Шаг сетки: 32", Action::GridSize(32)),
        ("Шаг сетки: 64", Action::GridSize(64)),
        ("Шаг сетки: 128", Action::GridSize(128)),
        ("", Action::ToggleRulers),
        ("Линейки", Action::ToggleRulers),
        ("", Action::CenterGuides),
        ("Направляющие по центру", Action::CenterGuides),
        ("Убрать направляющие", Action::ClearGuides),
        ("", Action::CanvasSizeDialog),
        ("Размер холста…", Action::CanvasSizeDialog),
        ("Холст 1920×1080", Action::CanvasSize(1920, 1080)),
        ("Холст 1280×800", Action::CanvasSize(1280, 800)),
        ("Холст 1024×1024", Action::CanvasSize(1024, 1024)),
        ("", Action::TrimCanvas),
        ("Обрезать пустые поля", Action::TrimCanvas),
        ("", Action::CropToSelection),
        ("Обрезать по выделению", Action::CropToSelection),
        ("", Action::Rotate90),
        ("Повернуть холст на 90°", Action::Rotate90),
        ("Повернуть холст на 180°", Action::Rotate180),
        ("Повернуть холст на 270°", Action::Rotate270),
        ("", Action::MirrorCanvasH),
        ("Отразить холст слева направо", Action::MirrorCanvasH),
        ("Отразить холст сверху вниз", Action::MirrorCanvasV),
    ]),
    ("Слой", &[
        ("Новый слой", Action::AddLayer),
        ("Дублировать", Action::DupLayer),
        ("Удалить", Action::DelLayer),
        ("", Action::MergeDown),
        ("Объединить вниз", Action::MergeDown),
        ("Выше", Action::LayerUp),
        ("Ниже", Action::LayerDown),
        ("", Action::AddMask),
        ("Маска из выделения", Action::AddMask),
        ("", Action::MaskWhite),
        ("Маска: залить белым", Action::MaskWhite),
        ("Маска: залить чёрным", Action::MaskBlack),
        ("Инвертировать маску", Action::InvertMask),
        ("Включить/выключить маску", Action::ToggleMask),
        ("Редактировать маску", Action::EditMask),
        ("Удалить маску", Action::DeleteMask),
        ("", Action::MakeGroup),
        ("Убрать в папку", Action::MakeGroup),
        ("Распустить папку", Action::Ungroup),
        ("", Action::FlipLayerH),
        ("Отразить слева направо", Action::FlipLayerH),
        ("Отразить сверху вниз", Action::FlipLayerV),
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

    // Сетка поверх холста: тонкие линии через равные промежутки.
    if app.show_grid && app.grid_size >= 2.0 {
        let step = app.grid_size * app.zoom;
        if step >= 4.0 {
            let color = [0.15, 0.15, 0.18, 0.35];
            let mut x = (cr[0] / step).ceil() * step;
            while x < cr[2] {
                ui.line(x, cr[1], x, cr[3], color, 1.0);
                x += step;
            }
            let mut y = (cr[1] / step).ceil() * step;
            while y < cr[3] {
                ui.line(cr[0], y, cr[2], y, color, 1.0);
                y += step;
            }
        }
    }

    // Плавающий фрагмент после вставки: рисуется поверх холста, но под
    // панелями, поэтому остаётся отдельным проходом рендерера.
    if let Some(f) = app.floating.as_ref() {
        let a = app.canvas_to_screen(f.x, f.y);
        let b = app.canvas_to_screen(f.x + f.data.width() as f32, f.y + f.data.height() as f32);
        let fr = [a.0, a.1, b.0, b.1];
        ui.checker(fr, 8.0);
        ui.textured(fr, [0.0, 0.0, 1.0, 1.0], UNIT_FLOAT);
        // рамка фрагмента, пока он не закреплён
        ui.frame(fr, [0.2, 0.85, 0.4, 0.95], 1.0);
    }

    // Свободная трансформация: рамка с ручками, поворот и вписывание.
    if let Some(t) = app.transform.as_ref() {
        let sp: [(f32, f32); 4] = [
            app.canvas_to_screen(t.pts[0].0, t.pts[0].1),
            app.canvas_to_screen(t.pts[1].0, t.pts[1].1),
            app.canvas_to_screen(t.pts[2].0, t.pts[2].1),
            app.canvas_to_screen(t.pts[3].0, t.pts[3].1),
        ];
        let (mut x0, mut y0, mut x1, mut y1) = (1e9f32, 1e9f32, -1e9f32, -1e9f32);
        for p in sp.iter() {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        ui.quad([x0, y0, x1, y1], [0.2, 0.6, 1.0, 0.12]);
        // Четыре стороны рамки
        for i in 0..4 {
            let a = sp[i];
            let b = sp[(i + 1) % 4];
            ui.line(a.0, a.1, b.0, b.1, [0.35, 0.75, 1.0, 0.95], 1.5);
        }
        // Ручки-квадратики по углам
        for (i, p) in sp.iter().enumerate() {
            ui.quad([p.0 - 4.0, p.1 - 4.0, p.0 + 4.0, p.1 + 4.0], [1.0, 1.0, 1.0, 1.0]);
            ui.frame([p.0 - 4.0, p.1 - 4.0, p.0 + 4.0, p.1 + 4.0], [0.2, 0.5, 0.9, 1.0], 1.0);
            let _ = i;
        }
        // Ручка поворота сверху
        let top = ((sp[0].0 + sp[1].0) / 2.0, (sp[0].1 + sp[1].1) / 2.0);
        let up = (top.0, top.1 - 26.0);
        ui.line(top.0, top.1, up.0, up.1, [0.35, 0.75, 1.0, 0.95], 1.5);
        ui.quad([up.0 - 5.0, up.1 - 5.0, up.0 + 5.0, up.1 + 5.0], [1.0, 1.0, 1.0, 1.0]);
        ui.frame([up.0 - 5.0, up.1 - 5.0, up.0 + 5.0, up.1 + 5.0], [0.2, 0.5, 0.9, 1.0], 1.0);
        // Подсказка с текущими размером и углом
        let cur_w = (((t.pts[1].0 - t.pts[0].0).powi(2) + (t.pts[1].1 - t.pts[0].1).powi(2)).sqrt()).round() as i32;
        let cur_h = (((t.pts[3].0 - t.pts[0].0).powi(2) + (t.pts[3].1 - t.pts[0].1).powi(2)).sqrt()).round() as i32;
        let ang = (t.pts[1].1 - t.pts[0].1).atan2(t.pts[1].0 - t.pts[0].0).to_degrees();
        let txt = format!("{}×{}  {:.1}°", cur_w.max(1), cur_h.max(1), ang);
        let tw = ui.text_width(&txt, FONT_SMALL, false) + 14.0;
        ui.quad([x0, y1 + 8.0, x0 + tw, y1 + 28.0], [0.0, 0.0, 0.0, 0.8]);
        ui.text(x0 + 7.0, y1 + 10.0, &txt, FONT_SMALL, theme::TEXT, false);
        // Кнопки: вписать, применить, отмена — как в Clip Studio.
        let by = y1 + 32.0;
        if ui.button([x0, by, x0 + 78.0, by + 24.0], "Вписать") {
            app.transform_fit();
        }
        if ui.button([x0 + 84.0, by, x0 + 168.0, by + 24.0], "Применить") {
            app.transform_commit();
        }
        if ui.button([x0 + 174.0, by, x0 + 252.0, by + 24.0], "Отмена") {
            app.transform_cancel();
        }
    }

    // Направляющие: тонкие линии, тянутся мышью прямо по холсту.
    draw_guides(ui, app, cr);

    // Рамка выделения: гасим область вне выделения и рисуем «муравьёв».
    // Во время трансформации выделение не гасим картинку.
    if let Some(s) = app.selection.as_ref() {
        if app.transform.is_none() {
            let a = app.canvas_to_screen(s.x, s.y);
            let b = app.canvas_to_screen(s.x + s.w, s.y + s.h);
            let sr = [a.0, a.1, b.0, b.1];
            // затемнение снаружи: четыре полосы вокруг рамки
            let dim = [0.0, 0.0, 0.0, 0.28];
            ui.quad([cr[0], cr[1], cr[2], sr[1]], dim);
            ui.quad([cr[0], sr[3], cr[2], cr[3]], dim);
            ui.quad([cr[0], sr[1], sr[0], sr[3]], dim);
            ui.quad([sr[2], sr[1], cr[2], sr[3]], dim);
            // пунктир «бежит» со временем — app.ants_phase растёт каждый кадр
            ui.marching_ants(sr, app.ants_phase, 6.0, 4.0);
        }
    }
    // Рамка вокруг документа — тонкая, чтобы не затенять сам холст.
    ui.frame([cr[0] - 1.0, cr[1] - 1.0, cr[2] + 1.0, cr[3] + 1.0], [0.0, 0.0, 0.0, 0.6], 1.0);

    // Поле ввода текста: рисуем ровно то, что потом ляжет в слой.
    if app.text.is_some() {
        // Пока набирается текст, символы идут сюда, а не в поля панели.
        let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
        for k in keys {
            match k {
                KeyEv::Char(c) => app.text_input(c),
                KeyEv::Backspace => app.text_backspace(),
                KeyEv::Enter => app.commit_text(),
                KeyEv::Escape => app.cancel_text(),
                _ => {}
            }
        }
    }
    if let Some(t) = app.text.as_ref() {
        let size = (app.params.size * app.zoom).max(6.0);
        let bold = app.params.bold_text;
        let p = app.canvas_to_screen(t.x, t.y);
        // Якорь — верх строки, а текст рисуется по базовой линии, как в слое.
        let base = p.1 + ui.fonts.ascent(size);
        let tw = ui.text_width(&t.buf, size, bold);
        let lh = ui.line_height(size);
        let box_r = [p.0 - 6.0, p.1 - 2.0, p.0 + tw + 10.0, base + lh * 0.3];
        ui.quad(box_r, [0.0, 0.0, 0.0, 0.45]);
        ui.frame(box_r, theme::ACCENT, 1.0);
        let c = app.color();
        let col = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0];
        ui.text(p.0, base, &t.buf, size, col, bold);
        // курсор мигает — используем фазу «муравьёв»
        if (app.ants_phase * 2.0).fract() < 0.5 {
            ui.quad([p.0 + tw + 1.0, base - lh * 0.75, p.0 + tw + 2.5, base + lh * 0.1], col);
        }
    }

    // Круг текущего размера кисти под курсором — как в Clip Studio.
    if app.cursor_in_canvas {
        let rad = (app.params.size * 0.5 * app.zoom).max(1.5);
        let (mx, my) = app.cursor_screen;
        ui.ring(mx, my, rad, [0.0, 0.0, 0.0, 0.75], 1.0);
        ui.ring(mx, my, rad + 1.0, [1.0, 1.0, 1.0, 0.75], 1.0);
    }
    ui.pop_clip();

    // --- линейки: тянутся мышью, из них рождаются направляющие ---
    if app.show_rulers {
        rulers(ui, app, r, actions);
    }

    // --- меню ---
    menu_bar(ui, app, MENU_H, actions);

    // --- тулбар ---
    toolbar(ui, app, TOOLBAR_W, MENU_H, bottom_y, actions);

    // --- правая панель: свойства + слои; под холстом — полоса истории ---
    let props_r = [right_x, MENU_H, w as f32, bottom_y - LAYERS_H];
    let layers_r = [right_x, bottom_y - LAYERS_H, w as f32, bottom_y];
    properties(ui, app, props_r);
    layers_panel(ui, app, layers_r, actions);
    history_strip(ui, app, [r[0], bottom_y - HISTORY_H, right_x, bottom_y]);

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
    if app.size_dialog {
        size_dialog(ui, app, w as f32, h as f32);
    }
    if app.filter_dialog {
        filter_dialog(ui, app, w as f32, h as f32);
    }
}

/// Линейки сверху и слева: деления в пикселях холста. Зажатая кнопка мыши
/// внутри линейки тянет новую направляющую, как в Clip Studio.
fn rulers(ui: &mut Ui, app: &mut App, cr: Rect, actions: &mut Vec<Action>) {
    const R: f32 = 18.0;
    let hz = [cr[0], cr[1], cr[2], cr[1] + R];
    let vt = [cr[0], cr[1], cr[0] + R, cr[3]];
    ui.quad(hz, theme::CANVAS_BG);
    ui.quad(vt, theme::CANVAS_BG);
    ui.quad([cr[0], cr[1], cr[2], cr[1] + 1.0], theme::BORDER);
    ui.quad([cr[0], cr[1], cr[0] + 1.0, cr[3]], theme::BORDER);

    // Выбираем шаг деления так, чтобы подписи не наезжали друг на друга.
    let (mut step, mut mul) = (10.0f32, 1.0f32);
    while step * app.zoom < 44.0 {
        step *= if mul < 9.0 { 2.0 } else { 2.5 };
        mul = if mul < 9.0 { mul * 2.0 } else { mul * 2.5 };
    }
    let _ = mul;
    let a = app.canvas_to_screen(0.0, 0.0);
    let b = app.canvas_to_screen(app.doc.width as f32, app.doc.height as f32);
    let tick = theme::TEXT_DIM;
    // Горизонтальная: деления по X, подписи снизу линейки.
    let mut x = (a.0 / step).ceil() * step;
    while x < b.0 {
        if x >= cr[0] {
            ui.line(x, cr[1] + 5.0, x, cr[1] + R, tick, 1.0);
            let px = (app.screen_to_canvas(x, cr[1]).0).round() as i32;
            let s = px.to_string();
            let tw = ui.text_width(&s, 10.0, false);
            ui.text(x - tw / 2.0, cr[1] + 2.0, &s, 10.0, tick, false);
        }
        x += step;
    }
    // Вертикальная: деления по Y, подписи повёрнуты не будем — просто полосой.
    let mut y = (a.1 / step).ceil() * step;
    while y < b.1 {
        if y >= cr[1] {
            ui.line(cr[0] + 5.0, y, cr[0] + R, y, tick, 1.0);
        }
        y += step;
    }

    // Тянем новую направляющую из линейки. Клик по линейке не должен
    // доходить до холста, иначе под линейкой начнётся мазок.
    let in_ruler = hover(ui, hz) || hover(ui, vt);
    if ui.pressed && in_ruler {
        ui.consumed_click = true;
        if hover(ui, hz) {
            let cx = app.screen_to_canvas(ui.mouse.0, cr[1]).0;
            app.guides.push((cx, true));
        } else {
            let cy = app.screen_to_canvas(cr[0], ui.mouse.1).1;
            app.guides.push((cy, false));
        }
        app.guide_drag = Some(app.guides.len() - 1);
    }
    if app.guide_drag.is_some() && !ui.down {
        app.guide_drag = None;
    }
    // Двойной щелчок по линейке рядом с направляющей — удалить её.
    if ui.double_click() && in_ruler {
        let near = app.guides.iter().position(|g| {
            if g.1 {
                let gy = app.canvas_to_screen(0.0, g.0).1;
                (gy - ui.mouse.1).abs() <= 5.0
            } else {
                let gx = app.canvas_to_screen(g.0, 0.0).0;
                (gx - ui.mouse.0).abs() <= 5.0
            }
        });
        if let Some(i) = near {
            app.guides.remove(i);
            app.notify("Направляющая удалена");
        }
    }
    let _ = actions;
}

/// Рисует направляющие и обрабатывает их перетаскивание. Ближайшая к
/// курсору линия цепляется мышью — как в Clip Studio.
fn draw_guides(ui: &mut Ui, app: &mut App, cr: Rect) {
    if app.guides.is_empty() && app.guide_drag.is_none() {
        return;
    }
    let grab_r = 5.0;
    // Сначала ищем, за что схватились.
    if ui.pressed {
        let mut best: Option<(f32, usize)> = None;
        for (i, (pos, horiz)) in app.guides.iter().enumerate() {
            let s = if *horiz {
                let y = app.canvas_to_screen(0.0, *pos).1;
                ((y - ui.mouse.1).abs(), i)
            } else {
                let x = app.canvas_to_screen(*pos, 0.0).0;
                ((x - ui.mouse.0).abs(), i)
            };
            if s.0 <= grab_r && best.map_or(true, |b| s.0 < b.0) {
                best = Some(s);
            }
        }
        app.guide_drag = best.map(|b| b.1);
    }
    // Тянем выбранную направляющую.
    if let Some(i) = app.guide_drag {
        if ui.down {
            let (pos, horiz) = app.guides[i];
            let p = app.screen_to_canvas(ui.mouse.0, ui.mouse.1);
            let v = if horiz { p.1 } else { p.0 };
            app.guides[i].0 = v.clamp(0.0, if horiz { app.doc.height as f32 } else { app.doc.width as f32 });
            let _ = pos;
        } else {
            app.guide_drag = None;
        }
    }
    for (i, (pos, horiz)) in app.guides.iter().enumerate() {
        let active = app.guide_drag == Some(i);
        let color = if active { [1.0, 0.55, 0.2, 1.0] } else { [0.3, 0.85, 1.0, 0.9] };
        if *horiz {
            let y = app.canvas_to_screen(0.0, *pos).1;
            ui.line(cr[0], y, cr[2], y, color, 1.0);
        } else {
            let x = app.canvas_to_screen(*pos, 0.0).0;
            ui.line(x, cr[1], x, cr[3], color, 1.0);
        }
    }
}

/// Окно коррекции слоя: яркость, контраст, насыщенность, оттенок,
/// размытие и резкость — как «Фильтры» в Clip Studio.
fn filter_dialog(ui: &mut Ui, app: &mut App, w: f32, h: f32) {
    ui.quad([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    let dw = 380.0;
    let dh = 320.0;
    let r = [(w - dw) / 2.0, (h - dh) / 2.0, (w + dw) / 2.0, (h + dh) / 2.0];
    ui.quad(r, theme::PANEL);
    ui.frame(r, theme::BORDER, 1.0);
    let lh = ui.line_height(FONT_UI + 2.0);
    ui.text(r[0] + PAD + 4.0, r[1] + 10.0, "Фильтры слоя", FONT_UI + 2.0, theme::TEXT, true);
    let name = app.doc.layer_name(app.doc.active);
    ui.text(r[0] + PAD + 4.0, r[1] + 26.0, &format!("Слой: {}", name), FONT_SMALL, theme::TEXT_DIM, false);

    let mut y = r[1] + 44.0;
    let row = |ui: &mut Ui, y: f32, label: &str, v: &mut f32, min: f32, max: f32, step: f32| {
        let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
        ui.value_field(fr, label, v, min, max, step);
    };
    row(ui, y, "Яркость", &mut app.f_bright, -1.0, 1.0, 0.02);
    y += 26.0;
    row(ui, y, "Контраст", &mut app.f_contrast, -1.0, 1.0, 0.02);
    y += 26.0;
    row(ui, y, "Насыщенность", &mut app.f_saturate, -1.0, 1.0, 0.02);
    y += 26.0;
    row(ui, y, "Оттенок", &mut app.f_hue, -1.0, 1.0, 0.01);
    y += 26.0;
    row(ui, y, "Размытие", &mut app.f_blur, 0.0, 20.0, 0.5);
    y += 26.0;
    row(ui, y, "Резкость", &mut app.f_sharpen, 0.0, 1.0, 0.05);
    y += 24.0;
    ui.text(r[0] + PAD, y, "Enter — применить, Esc — отмена", FONT_SMALL, theme::TEXT_DIM, false);

    // Клавиши работают, когда окно открыто.
    let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
    let had_keys = !keys.is_empty();
    for k in keys {
        match k {
            KeyEv::Enter => app.apply_filters(),
            KeyEv::Escape => {
                app.filter_dialog = false;
                ui.focus = None;
            }
            _ => {}
        }
    }
    if app.filter_dialog && !had_keys && ui.pressed && hover(ui, [0.0, 0.0, w, h]) {
        // щелчок мимо окна — закрыть
        let inside = ui.mouse.0 > r[0] && ui.mouse.0 < r[2] && ui.mouse.1 > r[1] && ui.mouse.1 < r[3];
        if !inside {
            app.filter_dialog = false;
        }
    }

    let br = [r[2] - PAD - 90.0, r[3] - 38.0, r[2] - PAD, r[3] - 14.0];
    if ui.button(br, "Применить") {
        app.filter_dialog = false;
        ui.focus = None;
        app.apply_filters();
    }
    let cr = [br[0] - 84.0, br[1], br[0] - 8.0, br[3]];
    if ui.button(cr, "Отмена") {
        app.filter_dialog = false;
        ui.focus = None;
    }
    let _ = lh;
}

/// Своё окно размера холста: два числовых поля и кнопка «Применить».
pub fn size_dialog(ui: &mut Ui, app: &mut App, w: f32, h: f32) {
    ui.quad([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    let dw = 340.0;
    let dh = 208.0;
    let r = [(w - dw) / 2.0, (h - dh) / 2.0, (w + dw) / 2.0, (h + dh) / 2.0];
    ui.quad(r, theme::PANEL);
    ui.frame(r, theme::BORDER, 1.0);
    let lh = ui.line_height(FONT_UI + 2.0);
    ui.text(r[0] + PAD + 4.0, r[1] + 10.0, "Размер холста", FONT_UI + 2.0, theme::TEXT, true);

    // Поля ввода получают фокус по клику; цифры набираются с клавиатуры.
    let mut y = r[1] + 40.0;
    let mut fields: [(&str, f32, f32); 2] = [
        ("Ширина", app.size_w, 8192.0),
        ("Высота", app.size_h, 8192.0),
    ];
    for (i, (label, val, max)) in fields.iter_mut().enumerate() {
        let fr = [r[0] + PAD, y, r[2] - PAD, y + 24.0];
        if hover(ui, fr) && ui.pressed {
            ui.focus = Some(900 + i as u32);
        }
        let focused = ui.focus == Some(900 + i as u32);
        if focused {
            let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
            for k in keys {
                match k {
                    KeyEv::Char(c) if c.is_ascii_digit() => {
                        // Набор числа сдвигом влево: было 800, стало 8000.
                        let v = (*val * 10.0).floor() + (c as i64 - '0' as i64) as f32;
                        *val = v.clamp(1.0, *max);
                    }
                    KeyEv::Backspace => *val = (*val / 10.0).floor().max(1.0),
                    KeyEv::Enter => app.size_dialog = false,
                    KeyEv::Escape => {
                        app.size_dialog = false;
                        ui.focus = None;
                    }
                    _ => {}
                }
            }
        }
        ui.quad(fr, theme::FIELD);
        ui.frame(fr, if focused { theme::ACCENT } else { theme::BORDER }, 1.0);
        let txt = format!("{}: {}", label, *val as i32);
        ui.text(fr[0] + 8.0, fr[1] + (24.0 + lh * 0.72) / 2.0, &txt, FONT_SMALL, theme::TEXT, false);
        y += 30.0;
    }
    app.size_w = fields[0].1;
    app.size_h = fields[1].1;

    let hint = format!("Сейчас: {}×{}", app.doc.width, app.doc.height);
    ui.text(r[0] + PAD, y, &hint, FONT_SMALL, theme::TEXT_DIM, false);

    let br = [r[2] - PAD - 90.0, r[3] - 38.0, r[2] - PAD, r[3] - 14.0];
    if ui.button(br, "Применить") {
        let w = app.size_w as usize;
        let h = app.size_h as usize;
        app.size_dialog = false;
        ui.focus = None;
        app.resize_canvas(w, h);
    }
    let cr = [br[0] - 84.0, br[1], br[0] - 8.0, br[3]];
    if ui.button(cr, "Отмена") {
        app.size_dialog = false;
        ui.focus = None;
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
        Tool::Gradient => Icon::Gradient,
        Tool::Select => Icon::Select,
        Tool::Text => Icon::Text,
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
            ParamRow::GradientSoft => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let mut v = app.params.gradient_soft;
                if ui.value_field(fr, "Мягкость", &mut v, 0.0, 1.0, 0.005) {
                    app.params.gradient_soft = v.clamp(0.0, 1.0);
                }
                y += 26.0;
            }
            ParamRow::Checkbox { label, on } => {
                // У заливки это «ограничить область», у фигур — «залить целиком»,
                // у текста — «жирный»; читаем и пишем ровно тот флаг, который виден.
                let mut v = match app.tool {
                    Tool::Fill => app.params.contiguous,
                    Tool::Text => app.params.bold_text,
                    _ => app.params.shape_fill,
                };
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                if ui.checkbox(fr, &mut v, label) {
                    match app.tool {
                        Tool::Fill => app.params.contiguous = v,
                        Tool::Text => app.params.bold_text = v,
                        _ => app.params.shape_fill = v,
                    }
                }
                let _ = on;
                y += 26.0;
            }
            ParamRow::Shape => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let shape = app.params.shape;
                if ui.button(fr, &format!("Форма: {}", shape.name())) {
                    app.params.shape = shape.next();
                    app.notify(&format!("Форма кисти: {}", app.params.shape.name()));
                }
                y += 26.0;
            }
            ParamRow::Mirror => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let mut v = app.params.mirror;
                if ui.checkbox(fr, &mut v, "Зеркально") {
                    app.params.mirror = v;
                }
                y += 26.0;
            }
        }
    }

    // Пресеты кистей — ряд заменяет ручную настройку, как в Clip Studio.
    // Показываем их, только если снизу остаётся место под колорпикер.
    if app.tool.is_freehand() && r[3] - y > 250.0 {
        ui.text(r[0] + PAD, y, "Кисти", FONT_SMALL, theme::TEXT_DIM, false);
        y += 16.0;
        let cols = 3;
        let bw = (r[2] - r[0] - PAD * 2.0 - 4.0 * (cols - 1) as f32) / cols as f32;
        for (i, p) in PRESETS.iter().enumerate() {
            let cx = r[0] + PAD + (bw + 4.0) * (i % cols) as f32;
            let cy = y + 22.0 * (i / cols) as f32;
            if ui.small_button([cx, cy, cx + bw, cy + 19.0], p.name) {
                p.apply(&mut app.params);
                app.notify(&format!("Кисть «{}»", p.name));
            }
        }
        y += 22.0 * ((PRESETS.len() + cols - 1) / cols) as f32 + 6.0;
    }

    // --- цвет ---
    // Если места не хватило, колорпикер рисуется укороченным, а не наезжает.
    if app.tool.needs_color() && y + 60.0 < r[3] {
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

/// Номер строки слоя в видимом списке (нумерация сверху вниз с нуля).
fn row_of_index(shown: &[usize], index: usize, scroll: usize) -> usize {
    shown.iter().position(|i| *i == index).unwrap_or(scroll) + scroll
}

fn layers_panel(ui: &mut Ui, app: &mut App, r: Rect, actions: &mut Vec<Action>) {
    ui.quad(r, theme::PANEL);
    ui.quad([r[0], r[1], r[2], r[1] + 1.0], theme::BORDER);
    // Снизу панели живут три строки: свойства слоя, отражение и кнопки.
    // Список занимает всё остальное — иначе строки наезжают друг на друга.
    const CTRL_H: f32 = 80.0;
    let rows_h = (r[3] - r[1] - CTRL_H).max(40.0);
    ui.push_clip([r[0], r[1], r[2], r[1] + rows_h]);

    // Заголовок
    let head = [r[0] + PAD, r[1] + 4.0, r[2] - PAD, r[1] + 20.0];
    ui.text(head[0], head[1], &format!("Слои ({})", app.doc.layers.len()), FONT_UI, theme::TEXT, true);
    let active = app.doc.active;
    let meta = app.doc.layers[active].meta.clone();

    let row_h = 30.0;
    let list_top = r[1] + 26.0;
    let list_h = rows_h - 26.0;
    // Список видимых слоёв: сверху вниз, скрытые внутри закрытых папок пропускаем.
    // Скролл хранится в строках, поэтому при сворачивании папки он остаётся ровным.
    let shown: Vec<usize> = (0..app.doc.layers.len())
        .rev()
        .filter(|i| app.layer_visible_in_panel(*i))
        .collect();
    let vis_rows = ((list_h / row_h).floor() as usize).max(1).min(shown.len().max(1));
    let max_scroll = shown.len().saturating_sub(vis_rows);
    if app.layer_scroll > max_scroll {
        app.layer_scroll = max_scroll;
    }
    // Активный слой всегда виден: список прокручивается к нему.
    if let Some(pos) = shown.iter().position(|i| *i == active) {
        if pos < app.layer_scroll {
            app.layer_scroll = pos;
        } else if pos >= app.layer_scroll + vis_rows {
            app.layer_scroll = pos + 1 - vis_rows;
        }
    }
    // Куда попадёт сброшенный слой: строка под курсором → индекс слоя.
    let row_to_index = |row: f32| -> Option<usize> {
        let row = row.floor() as i64;
        if row < app.layer_scroll as i64 || row >= (app.layer_scroll + vis_rows) as i64 {
            return None;
        }
        shown.get(row as usize - app.layer_scroll).copied()
    };
    // Перетаскивание обрабатываем один раз, до цикла по строкам: иначе
    // строки, над которыми мышь не проходит, сбрасывали бы перенос.
    if let Some(from) = app.layer_drag {
        if let Some(target) = row_to_index((ui.mouse.1 - list_top) / row_h) {
            if ui.released {
                app.layer_drag = None;
                if target != from {
                    app.doc.active = from;
                    app.move_layer(target);
                }
            } else if target != from {
                let line = (list_top + (row_of_index(&shown, from, app.layer_scroll) as f32 + 0.5) * row_h)
                    .clamp(list_top, list_top + list_h);
                ui.quad([r[0] + 4.0, line - 1.5, r[2] - 4.0, line + 1.5], theme::ACCENT);
            }
        } else if ui.released {
            app.layer_drag = None;
        }
    } else if !ui.down {
        app.layer_drag = None;
    }

    // Сверху — верхний слой
    for row in app.layer_scroll..shown.len().min(app.layer_scroll + vis_rows) {
        let idx = shown[row];
        let rr = [r[0] + 4.0, list_top + (row - app.layer_scroll) as f32 * row_h, r[2] - 4.0, list_top + (row - app.layer_scroll + 1) as f32 * row_h - 2.0];
        // Строку можно перетащить мышью, но не за кнопку видимости и не за маску.
        let vis_r = [rr[0] + 4.0, rr[1] + 6.0, rr[0] + 20.0, rr[1] + 22.0];
        let mth = [rr[0] + 50.0, rr[1] + 4.0, rr[0] + 50.0 + 22.0, rr[1] + 4.0 + 22.0];
        let mask_area = app.doc.layers[idx].mask.is_some() && hover(ui, mth);
        let on_row = hover(ui, rr);
        // Шапка папки — если слой открывает новую группу, рисуем над ним папку.
        if app.is_group_header(idx) {
            let gr = [rr[0] - 2.0, rr[1] - row_h + 1.0, rr[2] + 2.0, rr[1] - 1.0];
            let open = app.group_open(idx);
            let gname = app.doc.layers[idx].meta.group.clone().unwrap_or_default();
            ui.quad(gr, theme::FIELD);
            ui.frame(gr, theme::BORDER, 1.0);
            // Стрелка раскрытия
            let tri = [gr[0] + 8.0, gr[1] + (gr[3] - gr[1]) / 2.0];
            ui.quad([tri[0] - 3.0, tri[1] - 4.0, tri[0] + 2.0, tri[1] + 3.0], theme::TEXT_DIM);
            if !open {
                ui.quad([tri[0] - 1.0, tri[1] - 4.0, tri[0] + 3.0, tri[1] + 4.0], theme::FIELD);
            }
            ui.text(gr[0] + 20.0, gr[1] + 2.0, &gname, FONT_SMALL, theme::TEXT, true);
            // Глаз на папке — видимость сразу всех слоёв внутри.
            let gv = [gr[2] - 22.0, gr[1] + 2.0, gr[2] - 4.0, gr[3] - 2.0];
            let all_vis = app.group_members(idx).iter().all(|i| app.doc.layers[*i].meta.visible);
            if ui.tool_button(gv, if all_vis { Icon::Eye } else { Icon::EyeOff }, "Видимость папки", false) {
                app.toggle_group_visibility(idx);
            }
            if hover(ui, gr) && !hover(ui, gv) && ui.pressed {
                app.toggle_group(idx);
            }
        }
        if on_row && !hover(ui, vis_r) && !mask_area && ui.pressed {
            app.layer_drag = Some(idx);
            app.select_layer(idx);
        }
        if ui.list_row(rr, idx == active, on_row) && app.layer_drag.is_none() {
            app.select_layer(idx);
        }
        let visible = app.doc.layers[idx].meta.visible;
        let non_empty = app.doc.layers[idx].pixels.iter().any(|v| *v != 0);
        if ui.tool_button(vis_r, if visible { Icon::Eye } else { Icon::EyeOff }, "Видимость", false) {
            let i = idx;
            app.set_layer_meta(i, |m| m.visible = !m.visible);
        }
        // миниатюра слоя из атласа
        let th = [rr[0] + 24.0, rr[1] + 4.0, rr[0] + 24.0 + 22.0, rr[1] + 4.0 + 22.0];
        ui.checker(th, 6.0);
        if non_empty {
            ui.thumb(th, app.thumb_uv(idx));
        }
        // Активная цель рисования обведена белой рамкой: слой или маска.
        let editing_here = idx == active && !app.edit_mask;
        ui.frame(th, if editing_here { [1.0, 1.0, 1.0, 1.0] } else { theme::BORDER }, if editing_here { 2.0 } else { 1.0 });
        // Миниатюра маски — справа от слоя, клик по ней включает правку маски.
        if let Some(mask) = app.doc.layers[idx].mask.as_ref() {
            let mask_used = mask.iter().any(|v| *v != 0);
            ui.checker(mth, 6.0);
            if mask_used {
                ui.mask_thumb(mth, app.thumb_uv(idx), if app.doc.layers[idx].mask_on { 1.0 } else { 0.45 });
            }
            let mask_active = idx == active && app.edit_mask;
            ui.frame(mth, if mask_active { [1.0, 1.0, 1.0, 1.0] } else { theme::BORDER }, if mask_active { 2.0 } else { 1.0 });
            if mask_area && ui.pressed {
                app.select_layer(idx);
                app.set_edit_mask(!app.edit_mask);
            }
        }
        let name_x = match app.doc.layers[idx].mask {
            Some(_) => mth[2] + 6.0,
            None => th[2] + 6.0,
        };
        let name = app.doc.layer_name(idx);
        let lh = ui.line_height(FONT_SMALL);
        let col = if idx == active { theme::TEXT } else { theme::TEXT_DIM };
        ui.text(name_x, rr[1] + (row_h + lh * 0.72) / 2.0 - 2.0, &name, FONT_SMALL, col, idx == active);
        let op = format!("{}%", (app.doc.layers[idx].meta.opacity * 100.0).round() as i32);
        ui.text_right(rr[2] - 6.0, rr[1] + (row_h + lh * 0.72) / 2.0 - 2.0, &op, FONT_SMALL, theme::TEXT_DIM, false);
    }
    ui.pop_clip();

    // Свойства активного слоя
    let py = r[1] + rows_h + 6.0;
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
    // Отражение слоя — двумя кнопками под полями.
    let fy = py + 24.0;
    let fw = (r[2] - r[0] - PAD * 2.0 - 6.0) / 2.0;
    if ui.small_button([r[0] + PAD, fy, r[0] + PAD + fw, fy + 20.0], "⇋ Сверху") {
        actions.push(Action::FlipLayerV);
    }
    if ui.small_button([r[0] + PAD + fw + 6.0, fy, r[2] - PAD, fy + 20.0], "⇄ Слева") {
        actions.push(Action::FlipLayerH);
    }

    // Кнопки управления слоями
    let by = r[3] - 26.0;
    let b = 24.0;
    let bw = (r[2] - r[0] - PAD * 2.0 - 5.0 * 6.0) / 7.0;
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
    // Кнопка маски: без маски — создать из выделения, с маской — правка маски.
    let has_mask = app.doc.layers[active].mask.is_some();
    if ui.small_button(mk(5), if has_mask { "▦✎" } else { "▦" }) {
        if has_mask {
            let on = !app.edit_mask;
            app.set_edit_mask(on);
        } else {
            actions.push(Action::AddMask);
        }
    }
    if ui.small_button(mk(6), "✕") {
        actions.push(Action::DelLayer);
    }
    // Подсказка под кнопкой маски: что сейчас рисуется.
    if app.edit_mask {
        ui.text(r[0] + PAD + (b + 5.0) * 5.0, by - 12.0, "маска", FONT_SMALL, theme::ACCENT, false);
    } else if has_mask && !app.doc.layers[active].mask_on {
        ui.text(r[0] + PAD + (b + 5.0) * 5.0, by - 12.0, "выкл", FONT_SMALL, theme::TEXT_DIM, false);
    }
    let _ = b;
}

/// Полоса истории под холстом: шаги слева направо, текущее состояние
/// подсвечено. Щелчок по шагу перематывает документ в это состояние.
fn history_strip(ui: &mut Ui, app: &mut App, r: Rect) {
    ui.quad(r, theme::PANEL);
    ui.quad([r[0], r[1], r[2], r[1] + 1.0], theme::BORDER);
    let lh = ui.line_height(FONT_SMALL);
    let cy = r[1] + (r[3] - r[1] + lh * 0.72) / 2.0;
    let x0 = r[0] + PAD;
    let x1 = r[2] - PAD;

    let labels = app.history_labels();
    let pos = app.history.position();
    // Ширина каждого шага — по его названию, одинаковая для всех,
    // чтобы прокрутка была предсказуемой.
    let chip_w = (ui.text_width("Режим наложения", FONT_SMALL, false) + 26.0).max(96.0);
    let steps_h = chip_w + 8.0;
    let vis = ((x1 - x0) / steps_h).floor().max(1.0) as usize;
    if app.history_scroll + vis > labels.len() {
        app.history_scroll = labels.len().saturating_sub(vis);
    }
    if app.history_scroll > pos {
        app.history_scroll = pos;
    }

    ui.push_clip([r[0], r[1] + 1.0, r[2], r[3]]);
    for i in 0..vis {
        let idx = app.history_scroll + i;
        if idx >= labels.len() {
            break;
        }
        let cr = [x0 + i as f32 * steps_h, r[1] + 6.0, x0 + (i as f32 + 1.0) * steps_h - 4.0, r[3] - 6.0];
        let current = idx == pos;
        let hot = hover(ui, cr);
        if ui.list_row(cr, current, hot) && !current {
            app.history_goto(idx);
        }
        let col = if current { theme::TEXT } else { theme::TEXT_DIM };
        // шаги после текущего показаны бледнее: их можно повторить
        let mark = if current { "▸ " } else { "" };
        ui.text(cr[0] + 8.0, cy, &format!("{}{}", mark, labels[idx]), FONT_SMALL, col, current);
    }
    ui.pop_clip();

    if hover(ui, r) && ui.wheel != 0.0 {
        let max_scroll = labels.len().saturating_sub(vis);
        let next = app.history_scroll as f32 - ui.wheel * 2.0;
        app.history_scroll = (next.max(0.0) as usize).min(max_scroll);
    }
    // Стрелки прокрутки, если список не помещается.
    let last = app.history_scroll + vis;
    if last < labels.len() {
        let ar = [r[2] - 20.0, r[1] + 6.0, r[2] - 4.0, r[3] - 6.0];
        if ui.small_button(ar, "◀") {
            app.history_scroll = (app.history_scroll + vis).min(labels.len().saturating_sub(vis));
        }
    }
    if app.history_scroll > 0 {
        let ar = [r[0] + 4.0, r[1] + 6.0, r[0] + 20.0, r[3] - 6.0];
        if ui.small_button(ar, "▶") {
            app.history_scroll = app.history_scroll.saturating_sub(vis);
        }
    }
}

fn status_bar(ui: &mut Ui, app: &mut App, w: f32, y: f32) {
    ui.quad([0.0, y, w, y + STATUS_H], theme::PANEL);
    ui.quad([0.0, y, w, y + 1.0], theme::BORDER);
    let lh = ui.line_height(FONT_SMALL);
    let ty = y + (STATUS_H + lh * 0.72) / 2.0;
    let cx = app.cursor.0 as i32;
    let cy = app.cursor.1 as i32;
    let left = format!(
        "{}  |  {}×{}  |  {}%  |  X:{} Y:{}{}",
        app.tool.name(),
        app.doc.width,
        app.doc.height,
        (app.zoom * 100.0).round() as i32,
        cx,
        cy,
        if app.edit_mask { "  |  МАСКА" } else { "" }
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
    let title = dlg.mode.title();
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
    if ui.button(br, if dlg.mode.is_save() { "Сохранить" } else { "Открыть" }) {
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

