//! Компоновка интерфейса в духе Clip Studio: меню, тулбар слева,
//! панель свойств инструмента и слоёв справа, холст по центру, статус внизу.

use crate::app::App;
use crate::doc::BlendMode;
use crate::palette;
use crate::renderer::UNIT_FLOAT;
use crate::tools::{rows_for, ParamRow, Symmetry, Tool, PRESETS, TOOLS};
use crate::ui::{theme, Icon, KeyEv, Rect, Ui, FONT_SMALL, FONT_UI, PAD};

pub const MENU_H: f32 = 26.0;
pub const TOOLBAR_W: f32 = 46.0;
pub const PANEL_W: f32 = 252.0;
pub const STATUS_H: f32 = 24.0;
/// Высота панели слоёв.
pub const LAYERS_H: f32 = 206.0;
/// Высота полосы истории под холстом.
pub const HISTORY_H: f32 = 44.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    New,
    Open,
    ImportImage,
    OpenPsd,
    ExportPsd,
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
    InvertSel,
    FeatherSel,
    GrowSel,
    ShrinkSel,
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
    ToggleNav,
    CenterGuides,
    ClearGuides,
    SnapGuides,
    SnapGrid,
    SnapAngle,
    CanvasSize(usize, usize),
    AddLayer,
    DupLayer,
    DelLayer,
    MergeDown,
    LayerUp,
    LayerDown,
    OpenFilters,
    OpenCurves,
    OpenLevels,
    OpenHsv,
    OpenBalance,
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
    ClipLayer,
    OpenFx,
    Shadow,
    OutlineFx,
    GlowFx,
    LayerTag,
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
        ("Открыть PSD…", Action::OpenPsd),
        ("", Action::Save),
        ("Сохранить\tCtrl+S", Action::Save),
        ("Сохранить как…\tCtrl+Shift+S", Action::SaveAs),
        ("", Action::ExportPng),
        ("Экспорт в PNG…", Action::ExportPng),
        ("Экспорт в PNG (прозрачный)…", Action::ExportPngAlpha),
        ("Экспорт в JPEG…", Action::ExportJpeg),
        ("Экспорт в PSD…", Action::ExportPsd),
        ("", Action::Quit),
        ("Выход", Action::Quit),
    ]),
    ("Правка", &[
        ("Отменить", Action::Undo),
        ("Повторить", Action::Redo),
        ("", Action::SelectAll),
        ("Выделить всё\tCtrl+A", Action::SelectAll),
        ("Инвертировать выделение", Action::InvertSel),
        ("", Action::FeatherSel),
        ("Растушевать выделение…", Action::FeatherSel),
        ("Расширить выделение", Action::GrowSel),
        ("Сузить выделение", Action::ShrinkSel),
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
        ("Кривые…", Action::OpenCurves),
        ("Уровни…", Action::OpenLevels),
        ("Тон и насыщенность…", Action::OpenHsv),
        ("Цветовой баланс…", Action::OpenBalance),
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
        ("", Action::ToggleNav),
        ("Навигатор", Action::ToggleNav),
        ("", Action::CenterGuides),
        ("Направляющие по центру", Action::CenterGuides),
        ("Убрать направляющие", Action::ClearGuides),
        ("", Action::SnapGuides),
        ("Привязка к направляющим", Action::SnapGuides),
        ("Привязка к сетке", Action::SnapGrid),
        ("Шаг угла 15°", Action::SnapAngle),
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
        ("", Action::ClipLayer),
        ("Прижать к нижнему слою", Action::ClipLayer),
        ("", Action::OpenFx),
        ("Эффекты слоя…", Action::OpenFx),
        ("", Action::Shadow),
        ("Тень", Action::Shadow),
        ("Обводка", Action::OutlineFx),
        ("Свечение", Action::GlowFx),
        ("", Action::LayerTag),
        ("Метка цвета", Action::LayerTag),
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

    // Подсказка привязки: к чему сейчас притянут курсор.
    draw_snap_hint(ui, app, cr);

    // Оси симметрии — видно, где мазок разойдётся копиями.
    draw_symmetry_axes(ui, app, cr);

    // Рамка выделения: гасим область вне выделения и рисуем «муравьёв».
    // Во время трансформации выделение не гасим картинку.
    if let Some(s) = app.selection.as_ref() {
        if app.transform.is_none() {
            let a = app.canvas_to_screen(s.x, s.y);
            let b = app.canvas_to_screen(s.x + s.w, s.y + s.h);
            let sr = [a.0, a.1, b.0, b.1];
            match app.sel_mask.as_ref() {
                // Форма выделения произвольная: затемнение считает шейдер
                // по маске, а «муравьи» бегут по её контуру.
                Some(mask) => {
                    ui.selection_quad(cr);
                    ants_by_mask(ui, app, mask, cr);
                }
                None => {
                    // Прямоугольное выделение: четыре полосы вокруг рамки.
                    let dim = [0.0, 0.0, 0.0, 0.28];
                    ui.quad([cr[0], cr[1], cr[2], sr[1]], dim);
                    ui.quad([cr[0], sr[3], cr[2], cr[3]], dim);
                    ui.quad([cr[0], sr[1], sr[0], sr[3]], dim);
                    ui.quad([sr[2], sr[1], cr[2], sr[3]], dim);
                    // пунктир «бежит» со временем — app.ants_phase растёт каждый кадр
                    ui.marching_ants(sr, app.ants_phase, 6.0, 4.0);
                }
            }
        }
    }
    // Набираемый многоугольник: вершины, рёбра и резиновая нить до курсора.
    if app.poly_active && !app.poly.is_empty() {
        let col = [0.35, 0.75, 1.0, 0.95];
        for i in 0..app.poly.len() {
            let a = app.canvas_to_screen(app.poly[i].0, app.poly[i].1);
            let b = app.canvas_to_screen(app.poly[(i + 1) % app.poly.len()].0, app.poly[(i + 1) % app.poly.len()].1);
            ui.line(a.0, a.1, b.0, b.1, col, 1.5);
        }
        // Нить от последней вершины к курсору — видно, где будет ребро.
        if app.tool.is_polygon() && app.cursor_in_canvas {
            let last = app.canvas_to_screen(app.poly[app.poly.len() - 1].0, app.poly[app.poly.len() - 1].1);
            ui.line(last.0, last.1, app.cursor_screen.0, app.cursor_screen.1, [0.8, 0.85, 0.9, 0.6], 1.0);
        }
        for (i, p) in app.poly.iter().enumerate() {
            let s = app.canvas_to_screen(p.0, p.1);
            let first = i == 0;
            let r = if first { 5.0 } else { 4.0 };
            // У первой вершины рамка толще: по ней контур замыкается.
            ui.quad([s.0 - r, s.1 - r, s.0 + r, s.1 + r], [0.1, 0.1, 0.12, 1.0]);
            ui.frame([s.0 - r, s.1 - r, s.0 + r, s.1 + r], col, if first { 2.0 } else { 1.0 });
        }
    }
    // Навигатор: маленькая копия холста с рамкой текущего вида.
    if app.show_nav {
        navigator(ui, app, cr);
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

    // Курсор кисти под указателем — той же формы, какой пойдёт мазок.
    if app.cursor_in_canvas {
        let rad = (app.params.size * 0.5 * app.zoom).max(1.0);
        let (mx, my) = app.cursor_screen;
        let shape = if app.tool.is_freehand() { app.brush_shape() } else { crate::raster::Shape::Round };
        match shape {
            crate::raster::Shape::Round => {
                ui.ring(mx, my, rad, [0.0, 0.0, 0.0, 0.75], 1.0);
                ui.ring(mx, my, rad + 1.0, [1.0, 1.0, 1.0, 0.75], 1.0);
            }
            crate::raster::Shape::Ellipse => {
                // Эллипс кисти шире по горизонтали — обводим его эллипсом.
                for (rx, ry) in [(rad * 1.9, rad), (rad * 1.9 + 1.0, rad + 1.0)] {
                    let n = 48;
                    for i in 0..n {
                        let a = i as f32 / n as f32 * std::f32::consts::TAU;
                        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
                        ui.line(
                            mx + a.cos() * rx, my + a.sin() * ry,
                            mx + a1.cos() * rx, my + a1.sin() * ry,
                            if i % 2 == 0 { [0.0, 0.0, 0.0, 0.75] } else { [1.0, 1.0, 1.0, 0.75] },
                            1.0,
                        );
                    }
                }
            }
            crate::raster::Shape::Square => {
                let c = [0.0, 0.0, 0.0, 0.75];
                for (o, col) in [(0.0f32, c), (1.0, [1.0, 1.0, 1.0, 0.75])] {
                    let x0 = mx - rad - o;
                    let y0 = my - rad - o;
                    let x1 = mx + rad + o;
                    let y1 = my + rad + o;
                    ui.line(x0, y0, x1, y0, col, 1.0);
                    ui.line(x1, y0, x1, y1, col, 1.0);
                    ui.line(x1, y1, x0, y1, col, 1.0);
                    ui.line(x0, y1, x0, y0, col, 1.0);
                }
            }
        }
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
    if app.curve_dialog {
        curve_dialog(ui, app, w as f32, h as f32);
    }
    if app.levels_dialog {
        levels_dialog(ui, app, w as f32, h as f32);
    }
    if app.hsv_dialog {
        hsv_dialog(ui, app, w as f32, h as f32);
    }
    if app.balance_dialog {
        balance_dialog(ui, app, w as f32, h as f32);
    }
    if app.fx_dialog {
        fx_dialog(ui, app, w as f32, h as f32);
    }
}

/// Рисует гистограмму активного слоя в прямоугольнике `g` — общая деталь
/// для окон «Уровни», «Тон и насыщенность» и «Цветовой баланс».
fn corr_histogram(ui: &mut Ui, app: &App, g: Rect) {
    ui.quad(g, [0.06, 0.06, 0.07, 1.0]);
    let peak = app.curve_hist.iter().copied().max().unwrap_or(1).max(1);
    let (gw, gh) = (g[2] - g[0], g[3] - g[1]);
    for (k, v) in app.curve_hist.iter().enumerate() {
        if *v == 0 {
            continue;
        }
        let bh = (*v as f32 / peak as f32).sqrt() * gh;
        let x = g[0] + k as f32 / 255.0 * gw;
        let bw = (gw / 180.0).max(1.0);
        ui.quad([x, g[3] - bh, x + bw, g[3]], [0.45, 0.5, 0.6, 0.75]);
    }
    ui.frame(g, theme::BORDER, 1.0);
}

/// Кнопки «Применить / Сбросить / Отмена» в общем виде для окон коррекции.
fn corr_buttons(ui: &mut Ui, app: &mut App, r: Rect, reset: bool) {
    let br = [r[2] - PAD - 90.0, r[3] - 38.0, r[2] - PAD, r[3] - 14.0];
    if ui.button(br, "Применить") {
        app.close_corr(true);
    }
    let rr = [br[0] - 84.0, br[1], br[0] - 8.0, br[3]];
    if ui.button(rr, "Сбросить") && reset {
        app.hsv_reset();
    }
    let cr = [rr[0] - 84.0, br[1], rr[0] - 8.0, br[3]];
    if ui.button(cr, "Отмена") {
        app.close_corr(false);
    }
}

/// Окно «Тон и насыщенность»: оттенок, насыщенность и светлота либо сразу
/// по всем цветам, либо по одному сектору.
fn hsv_dialog(ui: &mut Ui, app: &mut App, w: f32, h: f32) {
    use crate::raster::HsvChannel;
    ui.quad([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    let dw = 380.0;
    let dh = 320.0;
    let top = ((h - dh) / 2.0).max(8.0);
    let r = [(w - dw) / 2.0, top, (w + dw) / 2.0, top + dh];
    ui.quad(r, theme::PANEL);
    ui.frame(r, theme::BORDER, 1.0);
    ui.text(r[0] + PAD, r[1] + 8.0, "Тон и насыщенность", FONT_UI + 2.0, theme::TEXT, true);
    let name = app.doc.layer_name(app.doc.active);
    ui.text(r[0] + PAD, r[1] + 26.0, &format!("Слой: {}", name), FONT_SMALL, theme::TEXT_DIM, false);

    let g = [r[0] + PAD, r[1] + 46.0, r[2] - PAD, r[1] + 140.0];
    corr_histogram(ui, app, g);

    // Сектора: семь кнопок в два ряда — по названию не помещаются в одну.
    let bw = (dw - PAD * 2.0 - 6.0 * 3.0) / 4.0;
    for (i, ch) in HsvChannel::ALL.iter().enumerate() {
        let col = i % 4;
        let row = i / 4;
        let br = [
            r[0] + PAD + (bw + 6.0) * col as f32,
            g[3] + 10.0 + 24.0 * row as f32,
            r[0] + PAD + (bw + 6.0) * col as f32 + bw,
            g[3] + 30.0 + 24.0 * row as f32,
        ];
        if ui.button(br, ch.name()) {
            app.hsv_channel = *ch;
            app.update_color();
        }
        if app.hsv_channel == *ch {
            ui.frame(br, theme::ACCENT, 1.0);
        }
    }
    let mut y = g[3] + 62.0;
    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
    let mut hue = app.hsv_hue;
    if ui.value_field(fr, "Оттенок", &mut hue, -180.0, 180.0, 1.0) {
        app.hsv_hue = hue.clamp(-180.0, 180.0);
        app.update_color();
    }
    y += 26.0;
    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
    let mut sat = app.hsv_sat;
    if ui.value_field(fr, "Насыщенность", &mut sat, -100.0, 100.0, 1.0) {
        app.hsv_sat = sat.clamp(-100.0, 100.0);
        app.update_color();
    }
    y += 26.0;
    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
    let mut light = app.hsv_light;
    if ui.value_field(fr, "Светлота", &mut light, -100.0, 100.0, 1.0) {
        app.hsv_light = light.clamp(-100.0, 100.0);
        app.update_color();
    }

    let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
    for k in keys {
        match k {
            KeyEv::Enter => app.close_corr(true),
            KeyEv::Escape => {
                app.close_corr(false);
                ui.focus = None;
            }
            _ => {}
        }
    }
    corr_buttons(ui, app, r, true);
}

/// Окно «Цветовой баланс»: три полосы (тени, средние тона, света) и в
/// каждой сдвиг красного, зелёного и синего.
fn balance_dialog(ui: &mut Ui, app: &mut App, w: f32, h: f32) {
    ui.quad([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    let dw = 420.0;
    let dh = 300.0;
    let top = ((h - dh) / 2.0).max(8.0);
    let r = [(w - dw) / 2.0, top, (w + dw) / 2.0, top + dh];
    ui.quad(r, theme::PANEL);
    ui.frame(r, theme::BORDER, 1.0);
    ui.text(r[0] + PAD, r[1] + 8.0, "Цветовой баланс", FONT_UI + 2.0, theme::TEXT, true);
    let name = app.doc.layer_name(app.doc.active);
    ui.text(r[0] + PAD, r[1] + 26.0, &format!("Слой: {}", name), FONT_SMALL, theme::TEXT_DIM, false);

    let g = [r[0] + PAD, r[1] + 46.0, r[2] - PAD, r[1] + 126.0];
    corr_histogram(ui, app, g);
    // Полосы света подписаны прямо на гистограмме.
    for (i, (t, cap)) in [(0.0f32, "тени"), (0.5, "середина"), (1.0, "света")].iter().enumerate() {
        let _ = i;
        ui.text(g[0] + 4.0 + t * (g[2] - g[0] - 50.0), g[1] + 4.0, cap, FONT_SMALL, theme::TEXT_DIM, false);
    }

    let bands = ["Тени", "Средние тона", "Света"];
    let mut y = g[3] + 10.0;
    for (b, cap) in bands.iter().enumerate() {
        let bw = (dw - PAD * 2.0 - 8.0) / 3.0;
        for (c, ch) in ["R", "G", "B"].iter().enumerate() {
            let label = format!("{} {}", cap, ch);
            let fr = [
                r[0] + PAD + (bw + 4.0) * c as f32,
                y,
                r[0] + PAD + (bw + 4.0) * c as f32 + bw,
                y + 20.0,
            ];
            let mut v = app.balance[b][c];
            if ui.value_field(fr, &label, &mut v, -100.0, 100.0, 1.0) {
                app.balance[b][c] = v.clamp(-100.0, 100.0);
                app.update_color();
            }
        }
        y += 26.0;
    }

    let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
    for k in keys {
        match k {
            KeyEv::Enter => app.close_corr(true),
            KeyEv::Escape => {
                app.close_corr(false);
                ui.focus = None;
            }
            _ => {}
        }
    }
    let br = [r[2] - PAD - 90.0, r[3] - 38.0, r[2] - PAD, r[3] - 14.0];
    if ui.button(br, "Применить") {
        app.close_corr(true);
    }
    let rr = [br[0] - 84.0, br[1], br[0] - 8.0, br[3]];
    if ui.button(rr, "Сбросить") {
        app.balance_reset();
    }
    let cr = [rr[0] - 84.0, br[1], rr[0] - 8.0, rr[3]];
    if ui.button(cr, "Отмена") {
        app.close_corr(false);
    }
}

/// Окно эффектов слоя: падающая тень, обводка и свечение. Все три
/// неразрушающие — пиксели слоя не меняются.
fn fx_dialog(ui: &mut Ui, app: &mut App, w: f32, h: f32) {
    /// Шаг строки: поля, флажки и подписи цвета идут одинаково.
    const P: f32 = 27.0;
    ui.quad([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    let dw = 400.0;
    // Окно не должно уезжать за верх и низ окна.
    let dh = 56.0 + 13.0 * P + 40.0;
    let top = ((h - dh) / 2.0).max(8.0);
    let r = [(w - dw) / 2.0, top, (w + dw) / 2.0, top + dh];
    ui.quad(r, theme::PANEL);
    ui.frame(r, theme::BORDER, 1.0);
    ui.text(r[0] + PAD + 4.0, r[1] + 10.0, "Эффекты слоя", FONT_UI + 2.0, theme::TEXT, true);
    let name = app.doc.layer_name(app.doc.active);
    ui.text(
        r[0] + PAD + 4.0,
        r[1] + 30.0,
        &format!("Слой: {}", name),
        FONT_SMALL,
        theme::TEXT_DIM,
        false,
    );

    let fx = app.doc.layers[app.doc.active].meta.fx.clone();
    let x0 = r[0] + PAD;
    let x1 = r[2] - PAD;
    let mut y = r[1] + 56.0;
    ui.push_clip([r[0] + 1.0, r[1] + 46.0, r[2] - 1.0, r[3] - 34.0]);

    // --- Тень ---
    let mut on = fx.shadow;
    if ui.checkbox([x0, y, x1, y + 20.0], &mut on, "Тень") {
        app.set_layer_meta(app.doc.active, |m| m.fx.shadow = on);
    }
    y += P;
    if on {
        fx_num(ui, app, x0, x1, y, "Смещение X", fx.shadow_dx, FxNum::Dx, -64.0, 64.0, 0.5);
        y += P;
        fx_num(ui, app, x0, x1, y, "Смещение Y", fx.shadow_dy, FxNum::Dy, -64.0, 64.0, 0.5);
        y += P;
        fx_num(ui, app, x0, x1, y, "Мягкость", fx.shadow_blur, FxNum::Blur, 0.0, 40.0, 0.5);
        y += P;
        fx_num(ui, app, x0, x1, y, "Непрозрачность", fx.shadow_opacity, FxNum::Opacity, 0.0, 1.0, 0.01);
        y += P;
        color_row(ui, app, x0, y, "Цвет тени", fx.shadow_color, FxColor::Shadow);
        y += P;
    }

    // --- Обводка ---
    let mut on2 = fx.outline;
    if ui.checkbox([x0, y, x1, y + 20.0], &mut on2, "Обводка") {
        app.set_layer_meta(app.doc.active, |m| m.fx.outline = on2);
    }
    y += P;
    if on2 {
        fx_num(ui, app, x0, x1, y, "Толщина", fx.outline_size, FxNum::Size, 1.0, 24.0, 0.5);
        y += P;
        color_row(ui, app, x0, y, "Цвет обводки", fx.outline_color, FxColor::Outline);
        y += P;
    }

    // --- Свечение ---
    let mut on3 = fx.glow;
    if ui.checkbox([x0, y, x1, y + 20.0], &mut on3, "Свечение") {
        app.set_layer_meta(app.doc.active, |m| m.fx.glow = on3);
    }
    y += P;
    if on3 {
        fx_num(ui, app, x0, x1, y, "Радиус", fx.glow_blur, FxNum::GlowBlur, 0.0, 60.0, 0.5);
        y += P;
        fx_num(ui, app, x0, x1, y, "Непрозрачность", fx.glow_opacity, FxNum::GlowOpacity, 0.0, 1.0, 0.01);
        y += P;
        color_row(ui, app, x0, y, "Цвет свечения", fx.glow_color, FxColor::Glow);
    }
    ui.pop_clip();

    // Клавиши: Enter — закрыть, Esc — закрыть.
    let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
    for k in keys {
        match k {
            KeyEv::Enter | KeyEv::Escape => {
                app.fx_dialog = false;
                ui.focus = None;
            }
            _ => {}
        }
    }
    let br = [r[2] - PAD - 90.0, r[3] - 38.0, r[2] - PAD, r[3] - 14.0];
    if ui.button(br, "Закрыть") {
        app.fx_dialog = false;
    }
}

/// Числовое поле эффекта: значение пишется в мета слой через `field`.
fn fx_num(
    ui: &mut Ui,
    app: &mut App,
    x0: f32,
    x1: f32,
    y: f32,
    label: &str,
    cur: f32,
    field: FxNum,
    min: f32,
    max: f32,
    step: f32,
) {
    let mut v = cur;
    if ui.value_field([x0, y, x1, y + 20.0], label, &mut v, min, max, step) {
        let v = v.clamp(min, max);
        app.set_layer_meta(app.doc.active, |m| match field {
            FxNum::Dx => m.fx.shadow_dx = v,
            FxNum::Dy => m.fx.shadow_dy = v,
            FxNum::Blur => m.fx.shadow_blur = v,
            FxNum::Opacity => m.fx.shadow_opacity = v,
            FxNum::Size => m.fx.outline_size = v,
            FxNum::GlowBlur => m.fx.glow_blur = v,
            FxNum::GlowOpacity => m.fx.glow_opacity = v,
        });
    }
}

/// Какое числовое поле эффекта сейчас правится.
#[derive(Clone, Copy)]
enum FxNum {
    Dx,
    Dy,
    Blur,
    Opacity,
    Size,
    GlowBlur,
    GlowOpacity,
}

/// Ряд свотчей для выбора цвета эффекта.
fn color_row(
    ui: &mut Ui,
    app: &mut App,
    x0: f32,
    y: f32,
    label: &str,
    cur: [u8; 4],
    which: FxColor,
) {
    ui.text(x0, y, label, FONT_SMALL, theme::TEXT_DIM, false);
    let palette: [[u8; 4]; 8] = [
        [0, 0, 0, 255],
        [255, 255, 255, 255],
        [220, 50, 50, 255],
        [240, 160, 40, 255],
        [250, 230, 90, 255],
        [70, 190, 90, 255],
        [60, 120, 230, 255],
        [180, 90, 220, 255],
    ];
    let mut picked = None;
    for (i, c) in palette.iter().enumerate() {
        let x = x0 + 96.0 + i as f32 * 19.0;
        let sr = [x, y - 3.0, x + 17.0, y + 15.0];
        let sel = cur[..3] == c[..3];
        ui.swatch(sr, *c);
        ui.frame(sr, if sel { theme::ACCENT } else { theme::BORDER }, if sel { 2.0 } else { 1.0 });
        if hover(ui, sr) && ui.pressed {
            picked = Some(*c);
        }
    }
    if let Some(c) = picked {
        app.set_layer_meta(app.doc.active, |m| match which {
            FxColor::Shadow => m.fx.shadow_color = c,
            FxColor::Outline => m.fx.outline_color = c,
            FxColor::Glow => m.fx.glow_color = c,
        });
    }
}

/// Какой из цветов эффекта сейчас выбирается.
#[derive(Clone, Copy)]
enum FxColor {
    Shadow,
    Outline,
    Glow,
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
    if ui.double_click(0x11DE) && in_ruler {
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
/// Навигатор: весь холст целиком в маленьком окошке, рамка показывает, что
/// сейчас видно. Щелчок или перетаскивание двигает вид.
fn navigator(ui: &mut Ui, app: &mut App, cr: Rect) {
    const NAV_W: f32 = 196.0;
    const HEAD: f32 = 18.0;
    let img_h = ((NAV_W - 12.0) * app.doc.height as f32 / app.doc.width as f32).clamp(60.0, 240.0);
    let r = [cr[2] - NAV_W - 10.0, cr[1] + 10.0, cr[2] - 10.0, cr[1] + 10.0 + HEAD + img_h];
    // Подложка панели — фоном (его проход идёт раньше текстуры холста),
    // иначе сплошной квад закрыл бы миниатюру.
    ui.background(r, [0.08, 0.08, 0.09, 0.92]);
    ui.frame(r, theme::BORDER, 1.0);
    ui.text(r[0] + 6.0, r[1] + 3.0, "Навигатор", FONT_SMALL, theme::TEXT_DIM, false);
    // Кнопка закрытия — как в Clip Studio.
    let cb = [r[2] - 16.0, r[1] + 2.0, r[2] - 2.0, r[1] + 16.0];
    ui.quad([cb[0] + 5.0, cb[1] + 5.0, cb[2] - 5.0, cb[3] - 5.0], theme::TEXT_DIM);
    if hover(ui, cb) && ui.pressed {
        app.show_nav = false;
    }
    let ir = [r[0] + 4.0, r[1] + HEAD, r[2] - 4.0, r[3] - 4.0];
    ui.checker(ir, 8.0);
    // Весь холст вписан в окошко с сохранением пропорций.
    let k = (ir[2] - ir[0]) / app.doc.width as f32;
    let k2 = (ir[3] - ir[1]) / app.doc.height as f32;
    let k = k.min(k2);
    let dw = app.doc.width as f32 * k;
    let dh = app.doc.height as f32 * k;
    let dx = ir[0] + (ir[2] - ir[0] - dw) * 0.5;
    let dy = ir[1] + (ir[3] - ir[1] - dh) * 0.5;
    ui.canvas_quad([dx, dy, dx + dw, dy + dh], [0.0, 0.0, 1.0, 1.0]);
    ui.frame([dx, dy, dx + dw, dy + dh], theme::BORDER, 1.0);
    // Рамка текущего вида.
    let vw = (cr[2] - cr[0]) / app.zoom * k;
    let vh = (cr[3] - cr[1]) / app.zoom * k;
    let vp = app.screen_to_canvas(cr[0], cr[1]);
    let vx = dx + vp.0 * k;
    let vy = dy + vp.1 * k;
    let view = [vx.max(dx), vy.max(dy), (vx + vw).min(dx + dw), (vy + vh).min(dy + dh)];
    if view[2] > view[0] && view[3] > view[1] {
        ui.frame(view, theme::ACCENT, 1.0);
        // Всё, что вне рамки, приглушаем — видно, где мы находимся.
        let dim = [0.0, 0.0, 0.0, 0.35];
        ui.quad([dx, dy, dx + dw, view[1]], dim);
        ui.quad([dx, view[3], dx + dw, dy + dh], dim);
        ui.quad([dx, view[1], view[0], view[3]], dim);
        ui.quad([view[2], view[1], dx + dw, view[3]], dim);
    }
    // Перетаскивание рамки = панорамирование.
    let hot = hover(ui, ir) || hover(ui, r);
    if hot && ui.pressed {
        app.nav_drag = true;
        // Щелчок в навигаторе не должен попасть на холст.
        ui.consumed_click = true;
    }
    if app.nav_drag {
        if hot && (ui.down || ui.pressed) {
            let cx = (ui.mouse.0 - dx) / k;
            let cy = (ui.mouse.1 - dy) / k;
            app.center_on(cx, cy);
        } else {
            app.nav_drag = false;
        }
    }
}

/// Подсказка привязки: линия направляющей, рамка узла сетки или линия угла
/// от начала мазка — показывается всегда, когда курсор рядом.
fn draw_snap_hint(ui: &mut Ui, app: &App, cr: Rect) {
    use crate::app::SnapHit;
    if !app.in_canvas(app.cursor_screen) {
        return;
    }
    let from = if app.drawing { Some(app.stroke_start) } else { None };
    let Some((cur, hit)) = app.snap_preview(app.cursor, from) else { return };
    let col = [1.0, 0.2, 0.7, 0.9];
    match hit {
        SnapHit::GuideV(x) => {
            let a = app.canvas_to_screen(x, 0.0);
            let b = app.canvas_to_screen(x, app.doc.height as f32);
            ui.line(a.0, a.1, b.0, b.1, col, 1.0);
        }
        SnapHit::GuideH(y) => {
            let a = app.canvas_to_screen(0.0, y);
            let b = app.canvas_to_screen(app.doc.width as f32, y);
            ui.line(a.0, a.1, b.0, b.1, col, 1.0);
        }
        SnapHit::Grid(x, y) => {
            let p = app.canvas_to_screen(x, y);
            ui.frame([p.0 - 5.0, p.1 - 5.0, p.0 + 5.0, p.1 + 5.0], col, 1.5);
        }
        SnapHit::Angle => {
            let from = app.canvas_to_screen(app.stroke_start.0, app.stroke_start.1);
            let to = app.canvas_to_screen(cur.0, cur.1);
            ui.line(from.0, from.1, to.0, to.1, [0.4, 0.8, 1.0, 0.7], 1.0);
            ui.ring(to.0, to.1, 5.0, col, 1.5);
        }
    }
    let _ = cr;
}

/// Оси симметрии мазка: тонкие линии через центр холста. По лучам рисуем
/// столько осей, сколько выбрано секторов.
fn draw_symmetry_axes(ui: &mut Ui, app: &App, _cr: Rect) {
    if !app.params.symmetry.on() {
        return;
    }
    let (w, h) = (app.doc.width as f32, app.doc.height as f32);
    let c = app.canvas_to_screen(w * 0.5, h * 0.5);
    let col = [1.0, 0.75, 0.35, 0.5];
    // Луч обрезается краями холста, иначе линии уходят в тёмное поле вокруг.
    let ray = |a: f32| {
        let (dx, dy) = (a.cos(), a.sin());
        let tx = if dx.abs() > 0.001 { (w * 0.5) / dx.abs() } else { f32::MAX };
        let ty = if dy.abs() > 0.001 { (h * 0.5) / dy.abs() } else { f32::MAX };
        let t = tx.min(ty);
        (c.0 + dx * t * app.zoom, c.1 + dy * t * app.zoom)
    };
    let horizontal = |ui: &mut Ui| {
        let a = ray(std::f32::consts::PI);
        let b = ray(0.0);
        ui.line(a.0, a.1, b.0, b.1, col, 1.0);
    };
    let vertical = |ui: &mut Ui| {
        let a = ray(std::f32::consts::FRAC_PI_2);
        let b = ray(-std::f32::consts::FRAC_PI_2);
        ui.line(a.0, a.1, b.0, b.1, col, 1.0);
    };
    match app.params.symmetry {
        Symmetry::Center => {
            horizontal(ui);
            vertical(ui);
        }
        Symmetry::Vertical => vertical(ui),
        Symmetry::Horizontal => horizontal(ui),
        _ => {}
    }
    if app.params.symmetry == Symmetry::Radial {
        let n = app.params.sym_sides.clamp(2.0, 12.0) as f32;
        for k in 0..(n as i32) {
            let a = std::f32::consts::PI * k as f32 / n;
            let p0 = ray(a);
            let p1 = ray(a + std::f32::consts::PI);
            ui.line(p0.0, p0.1, p1.0, p1.1, col, 1.0);
        }
    }
    ui.ring(c.0, c.1, 4.0, [1.0, 0.75, 0.35, 0.8], 1.0);
}

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

/// Окно уровней: входные чёрная и белая точки, гамма, выходные точки.
/// Над полями — гистограмма активного слоя, под ними лента «до → после».
fn levels_dialog(ui: &mut Ui, app: &mut App, w: f32, h: f32) {
    ui.quad([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    let dw = 380.0;
    let dh = 380.0;
    let top = ((h - dh) / 2.0).max(8.0);
    let r = [(w - dw) / 2.0, top, (w + dw) / 2.0, top + dh];
    ui.quad(r, theme::PANEL);
    ui.frame(r, theme::BORDER, 1.0);
    ui.text(r[0] + PAD, r[1] + 8.0, "Уровни", FONT_UI + 2.0, theme::TEXT, true);
    let name = app.doc.layer_name(app.doc.active);
    ui.text(r[0] + PAD, r[1] + 26.0, &format!("Слой: {}", name), FONT_SMALL, theme::TEXT_DIM, false);

    // Гистограмма с подписью входных точек.
    let g = [r[0] + PAD, r[1] + 46.0, r[2] - PAD, r[1] + 166.0];
    ui.quad(g, [0.06, 0.06, 0.07, 1.0]);
    let peak = app.curve_hist.iter().copied().max().unwrap_or(1).max(1);
    let gw = g[2] - g[0];
    let gh = g[3] - g[1];
    for (k, v) in app.curve_hist.iter().enumerate() {
        if *v == 0 {
            continue;
        }
        let bh = (*v as f32 / peak as f32).sqrt() * gh;
        let x = g[0] + k as f32 / 255.0 * gw;
        let bw = (gw / 180.0).max(1.0);
        ui.quad([x, g[3] - bh, x + bw, g[3]], [0.45, 0.5, 0.6, 0.75]);
    }
    // Входные точки отмечаются вертикальными отметками.
    for (v, c) in [
        (app.lvl_in_black, [0.6, 0.6, 0.65, 1.0]),
        (app.lvl_in_white, [0.6, 0.6, 0.65, 1.0]),
    ] {
        let x = g[0] + v / 255.0 * gw;
        ui.line(x, g[1], x, g[3], c, 1.0);
    }
    ui.frame(g, theme::BORDER, 1.0);

    let mut y = g[3] + 12.0;
    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
    let mut ib = app.lvl_in_black;
    if ui.value_field(fr, "Чёрная", &mut ib, 0.0, 254.0, 1.0) {
        app.lvl_in_black = ib.clamp(0.0, 254.0);
        app.update_curve();
    }
    y += 26.0;
    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
    let mut iw = app.lvl_in_white;
    if ui.value_field(fr, "Белая", &mut iw, 1.0, 255.0, 1.0) {
        app.lvl_in_white = iw.clamp(1.0, 255.0);
        app.update_curve();
    }
    y += 26.0;
    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
    let mut gm = app.lvl_gamma;
    if ui.value_field(fr, "Гамма", &mut gm, 0.1, 4.0, 0.05) {
        app.lvl_gamma = gm.clamp(0.1, 4.0);
        app.update_curve();
    }
    y += 26.0;
    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
    let mut ob = app.lvl_out_black;
    if ui.value_field(fr, "Выход: чёрная", &mut ob, 0.0, 255.0, 1.0) {
        app.lvl_out_black = ob.clamp(0.0, 255.0);
        app.update_curve();
    }
    y += 26.0;
    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
    let mut ow = app.lvl_out_white;
    if ui.value_field(fr, "Выход: белая", &mut ow, 0.0, 255.0, 1.0) {
        app.lvl_out_white = ow.clamp(0.0, 255.0);
        app.update_curve();
    }

    let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
    for k in keys {
        match k {
            KeyEv::Enter => app.close_corr(true),
            KeyEv::Escape => {
                app.close_corr(false);
                ui.focus = None;
            }
            _ => {}
        }
    }
    let br = [r[2] - PAD - 90.0, r[3] - 38.0, r[2] - PAD, r[3] - 14.0];
    if ui.button(br, "Применить") {
        app.close_corr(true);
    }
    let rr = [br[0] - 84.0, br[1], br[0] - 8.0, br[3]];
    if ui.button(rr, "Сбросить") {
        app.levels_reset();
    }
    let cr = [rr[0] - 84.0, br[1], rr[0] - 8.0, br[3]];
    if ui.button(cr, "Отмена") {
        app.close_corr(false);
    }
}

/// Окно кривых: поле 300×300 с гистограммой активного слоя и самой кривой.
/// Точки тянутся мышью, клик по кривой добавляет точку, двойной щелчок
/// по точке убирает её.
fn curve_dialog(ui: &mut Ui, app: &mut App, w: f32, h: f32) {
    use crate::raster::CurveChannel;
    ui.quad([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    let size = 300.0f32;
    let dw = size + PAD * 2.0;
    let dh = size + 130.0;
    let top = ((h - dh) / 2.0).max(8.0);
    let r = [(w - dw) / 2.0, top, (w + dw) / 2.0, top + dh];
    ui.quad(r, theme::PANEL);
    ui.frame(r, theme::BORDER, 1.0);
    ui.text(r[0] + PAD, r[1] + 8.0, "Кривые", FONT_UI + 2.0, theme::TEXT, true);
    let name = app.doc.layer_name(app.doc.active);
    ui.text(r[0] + PAD, r[1] + 26.0, &format!("Слой: {}", name), FONT_SMALL, theme::TEXT_DIM, false);
    // Каналы — как в Clip Studio: общий тон и три цветовых.
    let bw = (dw - PAD * 2.0 - 6.0 * 3.0) / 4.0;
    for (i, ch) in CurveChannel::ALL.iter().enumerate() {
        let br = [r[0] + PAD + (bw + 6.0) * i as f32, r[1] + 42.0, r[0] + PAD + (bw + 6.0) * i as f32 + bw, r[1] + 62.0];
        if ui.button(br, ch.name()) {
            app.curve_channel = *ch;
            // Кривая одна на все каналы, но предпросмотр пересчитывается.
            app.update_curve();
        }
        if app.curve_channel == *ch {
            ui.frame(br, theme::ACCENT, 1.0);
        }
    }

    // Поле кривой: X — вход, Y — выход, обе оси 0..=1.
    let g = [r[0] + PAD, r[1] + 70.0, r[0] + PAD + size, r[1] + 70.0 + size];
    ui.quad(g, [0.06, 0.06, 0.07, 1.0]);
    // Сетка: четыре квадранта и диагональ «как есть».
    for i in 1..4 {
        let t = i as f32 / 4.0;
        let v = [g[0], g[1] + t * size, g[2], g[1] + t * size];
        let hz = [g[0] + t * size, g[1], g[0] + t * size, g[3]];
        ui.line(v[0], v[1], v[2], v[3], [1.0, 1.0, 1.0, 0.08], 1.0);
        ui.line(hz[0], hz[1], hz[2], hz[3], [1.0, 1.0, 1.0, 0.08], 1.0);
    }
    ui.line(g[0], g[3], g[2], g[1], [1.0, 1.0, 1.0, 0.12], 1.0);
    // Гистограмма яркости слоя.
    let peak = app.curve_hist.iter().copied().max().unwrap_or(1).max(1);
    for (k, v) in app.curve_hist.iter().enumerate() {
        if *v == 0 {
            continue;
        }
        // Прозрачность столбца гаснет у верха — так видна форма кривой.
        let bh = (*v as f32 / peak as f32).sqrt() * size;
        let x = g[0] + k as f32 / 255.0 * size;
        let wbar = (size / 200.0).max(1.0);
        ui.quad([x, g[3] - bh, x + wbar, g[3]], [0.45, 0.5, 0.6, 0.75]);
    }
    // Сама кривая по таблице значений.
    let lut = app.curve.lut();
    let col = [0.35, 0.75, 1.0, 1.0];
    for k in 0..255 {
        let x0 = g[0] + k as f32 / 255.0 * size;
        let y0 = g[3] - lut[k] as f32 / 255.0 * size;
        let x1 = g[0] + (k + 1) as f32 / 255.0 * size;
        let y1 = g[3] - lut[k + 1] as f32 / 255.0 * size;
        ui.line(x0, y0, x1, y1, col, 1.5);
    }
    // Точки управления.
    let to_screen = |p: (f32, f32)| (g[0] + p.0 * size, g[3] - p.1 * size);
    for (i, p) in app.curve.pts.clone().iter().enumerate() {
        let (x, y) = to_screen(*p);
        let rad = if i == 0 || i + 1 == app.curve.pts.len() { 5.0 } else { 4.5 };
        ui.quad([x - rad, y - rad, x + rad, y + rad], [0.1, 0.1, 0.12, 1.0]);
        ui.frame([x - rad, y - rad, x + rad, y + rad], col, 1.5);
    }
    ui.frame(g, theme::BORDER, 1.0);

    // Перетаскивание точек прямо по полю кривой.
    let inside = ui.mouse.0 > g[0] && ui.mouse.0 < g[2] && ui.mouse.1 > g[1] && ui.mouse.1 < g[3];
    let tol = 9.0 / size;
    let (mx, my) = (
        ((ui.mouse.0 - g[0]) / size).clamp(0.0, 1.0),
        (1.0 - (ui.mouse.1 - g[1]) / size).clamp(0.0, 1.0),
    );
    if ui.pressed && inside {
        // Двойной щелчок по точке убирает её, обычный — берёт её в руку,
        // а щелчок по самой кривой ставит новую точку.
        if ui.double_click(0xC0FE) {
            if let Some(i) = app.curve.nearest(mx, my, tol) {
                if i > 0 && i + 1 < app.curve.pts.len() {
                    app.curve_point_remove(i);
                    app.curve_drag = None;
                    ui.consumed_click = true;
                }
            }
        } else {
            app.curve_drag = Some(match app.curve.nearest(mx, my, tol) {
                Some(i) => i,
                None => app.curve_point_to(mx, my),
            });
            ui.consumed_click = true;
        }
    }
    if ui.released {
        app.curve_drag = None;
    }
    if ui.down && inside {
        if let Some(i) = app.curve_drag {
            // Точку именно двигаем, а не добавляем: иначе протяжка оставила бы
            // за собой десяток новых точек.
            app.curve_move_point(i, mx, my);
            ui.consumed_click = true;
        }
    }

    // Клавиши: Enter — применить, Esc — отменить.
    let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
    for k in keys {
        match k {
            KeyEv::Enter => app.close_curves(true),
            KeyEv::Escape => {
                app.close_curves(false);
                ui.focus = None;
            }
            _ => {}
        }
    }
    let br = [r[2] - PAD - 90.0, r[3] - 38.0, r[2] - PAD, r[3] - 14.0];
    if ui.button(br, "Применить") {
        app.close_curves(true);
    }
    let rr = [br[0] - 84.0, br[1], br[0] - 8.0, br[3]];
    if ui.button(rr, "Сбросить") {
        app.curve_reset();
    }
    let cr = [rr[0] - 84.0, br[1], rr[0] - 8.0, br[3]];
    if ui.button(cr, "Отмена") {
        app.close_curves(false);
    }
}

/// Окно коррекции слоя: яркость, контраст, насыщенность, оттенок, размытие
/// и резкость — как «Фильтры» в Clip Studio.
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
        // Эллипс-выделение и палочка отличаются от рамки только смыслом,
        // но иконки у них свои: иначе три кнопки выглядят одинаково.
        Tool::EllipseSelect => Icon::Ellipse,
        Tool::Wand => Icon::Gradient,
        Tool::Polygon => Icon::Polygon,
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
                // Предпросмотр мазка: реальная форма отпечатка и шаг между
                // dab'ами — видно, какой линией пойдёт кисть.
                let pr = [r[0] + PAD, y + 22.0, r[2] - PAD, y + 44.0];
                ui.quad(pr, theme::FIELD);
                let c = app.color();
                let col = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0];
                ui.checker(pr, 6.0);
                if app.tool.is_freehand() {
                    let rad = (v * 0.5).clamp(0.8, 8.0);
                    let cy = (pr[1] + pr[3]) / 2.0;
                    let span = pr[2] - pr[0] - rad * 2.0;
                    let step = (rad * 2.0 * app.params.spacing).clamp(0.7, span.max(0.7));
                    let n = ((span / step).floor() as usize).min(240);
                    let x0 = pr[0] + rad;
                    for i in 0..=n {
                        brush_footprint(ui, x0 + i as f32 * step, cy, rad, app.params.shape, col);
                    }
                } else {
                    ui.circle((pr[0] + pr[2]) / 2.0, (pr[1] + pr[3]) / 2.0, (v * 0.5).clamp(1.0, 9.0), col);
                }
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
            ParamRow::Spacing => {
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let mut v = app.params.spacing;
                if ui.value_field(fr, "Шаг", &mut v, 0.01, 1.0, 0.005) {
                    app.params.spacing = v.clamp(0.01, 1.0);
                }
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
            ParamRow::Symmetry => {
                // Симметрия мазка: режим выбирается списком, число лучей —
                // только для радиальной, как в Clip Studio.
                ui.text(r[0] + PAD, y, "Симметрия", FONT_SMALL, theme::TEXT_DIM, false);
                y += 16.0;
                let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                let names: Vec<&str> = Symmetry::ALL.iter().map(|s| s.name()).collect();
                let cur = Symmetry::ALL.iter().position(|s| *s == app.params.symmetry).unwrap_or(0);
                if let Some(n) = ui.dropdown(fr, cur, &names) {
                    app.params.symmetry = Symmetry::ALL[n];
                }
                y += 26.0;
                if app.params.symmetry == Symmetry::Radial {
                    let sr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                    let mut v = app.params.sym_sides;
                    if ui.value_field(sr, "Лучей", &mut v, 2.0, 12.0, 1.0) {
                        app.params.sym_sides = v.clamp(2.0, 12.0);
                    }
                    y += 26.0;
                }
            }
            ParamRow::Sticky => {
                let mut v = app.params.sticky;
                if ui.checkbox([r[0] + PAD, y, r[2] - PAD, y + 20.0], &mut v, "Липкая палочка") {
                    app.params.sticky = v;
                }
                y += 26.0;
                if v {
                    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                    let mut hue = app.params.sticky_hue;
                    if ui.value_field(fr, "Оттенок", &mut hue, -180.0, 180.0, 1.0) {
                        app.params.sticky_hue = hue.clamp(-180.0, 180.0);
                    }
                    y += 26.0;
                    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                    let mut sat = app.params.sticky_sat;
                    if ui.value_field(fr, "Насыщ.", &mut sat, -100.0, 100.0, 1.0) {
                        app.params.sticky_sat = sat.clamp(-100.0, 100.0);
                    }
                    y += 26.0;
                }
            }
            ParamRow::Heal => {
                let mut v = app.params.heal;
                if ui.checkbox([r[0] + PAD, y, r[2] - PAD, y + 20.0], &mut v, "Восстанавливающая") {
                    app.params.heal = v;
                }
                y += 26.0;
                if v {
                    let fr = [r[0] + PAD, y, r[2] - PAD, y + 20.0];
                    let mut st = app.params.heal_strength;
                    if ui.value_field(fr, "Сила", &mut st, 0.05, 1.0, 0.01) {
                        app.params.heal_strength = st.clamp(0.05, 1.0);
                    }
                    y += 26.0;
                }
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

/// «Муравьиные дорожки» по контуру произвольной области выделения: идём по
/// пикселям, у которых выделен сосед, и рисуем пунктир вдоль общей границы.
fn ants_by_mask(ui: &mut Ui, app: &App, mask: &[u8], cr: Rect) {
    let (w, h) = (app.doc.width, app.doc.height);
    let z = app.zoom;
    let phase = (app.ants_phase as i32) % 8;
    let at = |x: usize, y: usize| -> u8 {
        if x >= w || y >= h {
            0
        } else {
            mask[y * w + x]
        }
    };
    let tl = app.canvas_to_screen(0.0, 0.0);
    // Горизонтальные и вертикальные отрезки контура длиной 6 px.
    let seg = 6.0f32;
    for y in 0..h {
        for x in 0..w {
            if at(x, y) <= 8 {
                continue;
            }
            let on = |dx: i32, dy: i32| -> bool {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                nx < 0 || ny < 0 || at(nx as usize, ny as usize) <= 8
            };
            // Цвет «муравья» чередуется по фазе и по координате края.
            let dash = |x: usize, y: usize| -> [f32; 4] {
                if ((x + y) as i32 + phase) / 4 % 2 == 0 {
                    [1.0, 1.0, 1.0, 0.95]
                } else {
                    [0.0, 0.0, 0.0, 0.95]
                }
            };
            if on(0, -1) {
                let p0 = app.canvas_to_screen(x as f32, y as f32);
                let p1 = app.canvas_to_screen((x + 1) as f32, y as f32);
                ui.line(p0.0, p0.1, p1.0, p1.1, dash(x, y), 1.0);
            }
            if on(0, 1) {
                let p0 = app.canvas_to_screen(x as f32, (y + 1) as f32);
                let p1 = app.canvas_to_screen((x + 1) as f32, (y + 1) as f32);
                ui.line(p0.0, p0.1, p1.0, p1.1, dash(x, y), 1.0);
            }
            if on(-1, 0) {
                let p0 = app.canvas_to_screen(x as f32, y as f32);
                let p1 = app.canvas_to_screen(x as f32, (y + 1) as f32);
                ui.line(p0.0, p0.1, p1.0, p1.1, dash(x, y), 1.0);
            }
            if on(1, 0) {
                let p0 = app.canvas_to_screen((x + 1) as f32, y as f32);
                let p1 = app.canvas_to_screen((x + 1) as f32, (y + 1) as f32);
                ui.line(p0.0, p0.1, p1.0, p1.1, dash(x, y), 1.0);
            }
        }
    }
    let _ = (tl, cr, z, seg);
}

/// Рисует один отпечаток кисти заданной формы — тот же силуэт, что и мазок
/// на холсте, только маленький.
fn brush_footprint(ui: &mut Ui, cx: f32, cy: f32, rad: f32, shape: crate::tools::BrushShape, col: [f32; 4]) {
    match shape {
        crate::tools::BrushShape::Round => ui.circle(cx, cy, rad, col),
        crate::tools::BrushShape::Ellipse => {
            for dy in 0..=(rad * 2.0).ceil() as i32 {
                let t = (dy as f32 - rad) / rad.max(0.001);
                let dx = rad * 1.9 * (1.0 - t * t).max(0.0).sqrt();
                ui.quad([cx - dx, cy - rad + dy as f32, cx + dx, cy - rad + dy as f32 + 1.0], col);
            }
        }
        crate::tools::BrushShape::Square => {
            ui.quad([cx - rad, cy - rad, cx + rad, cy + rad], col);
        }
    }
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

    // Заголовок. Ниже верхнего края на 8 px, чтобы не прилипал к полю HEX
    // колорпикера, когда параметров инструмента много.
    let head = [r[0] + PAD, r[1] + 8.0, r[2] - PAD, r[1] + 24.0];
    ui.text(head[0], head[1], &format!("Слои ({})", app.doc.layers.len()), FONT_UI, theme::TEXT, true);
    let active = app.doc.active;
    let meta = app.doc.layers[active].meta.clone();

    let row_h = 30.0;
    // Список начинается на строку ниже заголовка: над первой строкой рисуется
    // шапка папки, которой тоже нужно место.
    let list_top = r[1] + 30.0 + row_h;
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
        // Цветная метка слоя — полоска у левого края строки, как в Clip Studio.
        // Клик по ней переключает цвет метки (нет → красная → … → нет).
        let tag = app.doc.layers[idx].meta.tag;
        let tag_r = [rr[0] + 1.0, rr[1] + 3.0, rr[0] + 7.0, rr[1] + row_h - 5.0];
        if hover(ui, tag_r) && ui.pressed {
            app.select_layer(idx);
            app.cycle_tag();
        }
        // Строку можно перетащить мышью, но не за кнопку видимости и не за маску.
        let vis_r = [rr[0] + 10.0, rr[1] + 6.0, rr[0] + 26.0, rr[1] + 22.0];
        let mth = [rr[0] + 56.0, rr[1] + 4.0, rr[0] + 56.0 + 22.0, rr[1] + 4.0 + 22.0];
        let mask_area = app.doc.layers[idx].mask.is_some() && hover(ui, mth);
        let on_row = hover(ui, rr);
        // Шапка папки — если слой открывает новую группу, рисуем над ним папку.
        if app.is_group_header(idx) && rr[1] - row_h + 1.0 > r[1] + 30.0 {
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
        // Метку рисуем после подложки строки, иначе её заливает выделение
        // активного слоя.
        if tag > 0 {
            ui.quad(tag_r, crate::doc::TAG_COLORS[(tag - 1) as usize % 6]);
        } else if hover(ui, tag_r) {
            // Подсказка: на пустой полоске видно, что клик поставит метку.
            ui.frame(tag_r, theme::BORDER, 1.0);
        }
        let visible = app.doc.layers[idx].meta.visible;
        let non_empty = app.doc.layers[idx].pixels.iter().any(|v| *v != 0);
        if ui.tool_button(vis_r, if visible { Icon::Eye } else { Icon::EyeOff }, "Видимость", false) {
            let i = idx;
            app.set_layer_meta(i, |m| m.visible = !m.visible);
        }
        // миниатюра слоя из атласа
        let th = [rr[0] + 30.0, rr[1] + 4.0, rr[0] + 30.0 + 22.0, rr[1] + 4.0 + 22.0];
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
        // Двойной щелчок по имени переименовывает слой прямо в строке.
        if app.rename == Some(idx) {
            // Поле ввода имени прямо в строке списка.
            let er = [name_x - 2.0, rr[1] + 5.0, rr[2] - 34.0, rr[1] + row_h - 7.0];
            ui.quad(er, theme::FIELD);
            ui.frame(er, theme::ACCENT, 1.0);
            ui.text(er[0] + 4.0, er[1] + 2.0, &app.rename_buf, FONT_SMALL, theme::TEXT, false);
            let keys: Vec<KeyEv> = std::mem::take(&mut ui.keys);
            for k in keys {
                match k {
                    KeyEv::Char(c) => app.rename_buf.push(c),
                    KeyEv::Backspace => {
                        app.rename_buf.pop();
                    }
                    KeyEv::Enter => app.finish_rename(true),
                    KeyEv::Escape => app.finish_rename(false),
                    _ => {}
                }
            }
        } else {
            ui.text(name_x, rr[1] + (row_h + lh * 0.72) / 2.0 - 2.0, &name, FONT_SMALL, col, idx == active);
            if ui.double_click(0x1A9E + idx as u32) && on_row && !hover(ui, vis_r) && !hover(ui, mth) && !hover(ui, tag_r) {
                app.select_layer(idx);
                app.begin_rename();
            }
        }
        // Прижатый слой помечаем изогнутой стрелкой — как в Clip Studio.
        if app.doc.layers[idx].meta.clipped {
            let ax = name_x - 8.0;
            let ay = rr[1] + row_h * 0.5;
            ui.line(ax, ay - 3.0, ax - 3.0, ay, theme::ACCENT, 1.5);
            ui.line(ax - 3.0, ay, ax, ay + 3.0, theme::ACCENT, 1.5);
        }
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

