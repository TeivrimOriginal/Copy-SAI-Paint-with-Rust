//! TPaint — растровый редактор (аналог SayPaint) на Rust + Win32/GDI.
//! Всё рисуется в 32-битный DIB буфер и выводится через BitBlt (без мерцания).

#![windows_subsystem = "windows"]

mod app;
mod canvas;

use app::{App, Tool};
use std::cell::RefCell;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateSolidBrush, DEFAULT_GUI_FONT,
    DeleteDC, DeleteObject, EndPaint, FillRect, FrameRect, GetStockObject, InflateRect,
    InvalidateRect, SelectObject, SRCCOPY, WHITE_BRUSH, BITMAPINFO, BITMAPINFOHEADER, HGDIOBJ,
    BI_RGB, HBRUSH, HDC, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows_sys::Win32::UI::Controls::Dialogs::{
    ChooseColorW, GetOpenFileNameW, GetSaveFileNameW, CHOOSECOLORW, OPENFILENAMEW,
    OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ---------------------------------------------------------------------------
// Константы интерфейса
// ---------------------------------------------------------------------------

const TOOLBAR_H: i32 = 54;
const STATUS_H: i32 = 22;

const ID_T_PENCIL: u32 = 100;
const ID_T_BRUSH: u32 = 101;
const ID_T_ERASER: u32 = 102;
const ID_T_LINE: u32 = 103;
const ID_T_RECT: u32 = 104;
const ID_T_ELLIPSE: u32 = 105;
const ID_T_FILL: u32 = 106;

const ID_UNDO: u32 = 200;
const ID_REDO: u32 = 201;
const ID_CLEAR: u32 = 202;
const ID_COLOR: u32 = 203;
const ID_SWATCH: u32 = 204;
const ID_OPEN: u32 = 205;
const ID_SAVE: u32 = 206;
const ID_COMBO: u32 = 207;
const ID_LABEL_TH: u32 = 208;
const ID_STATUS: u32 = 209;

const THICKNESSES: [i32; 8] = [1, 2, 3, 5, 8, 12, 16, 20];

// ---------------------------------------------------------------------------
// Глобальное состояние (все окна — в одном потоке)
// ---------------------------------------------------------------------------

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static WINS: RefCell<WinHandles> = RefCell::new(WinHandles::new());
}

#[derive(Clone, Copy, Default)]
struct WinHandles {
    canvas: HWND,
    swatch: HWND,
    status: HWND,
    combo: HWND,
    main: HWND,
    custom: [COLORREF; 16],
    // Mem-DC для вывода полотна
    mdc: HDC,
    dib: HGDIOBJ,
    old: HGDIOBJ,
    bits: *mut u32,
    cw: i32,
    ch: i32,
}

impl WinHandles {
    fn new() -> Self {
        Self {
            custom: [0xFF_FF_FF; 16],
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Вспомогательные функции
// ---------------------------------------------------------------------------

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn hinst() -> windows_sys::Win32::Foundation::HINSTANCE {
    unsafe { GetModuleHandleW(null()) as windows_sys::Win32::Foundation::HINSTANCE }
}

/// Уголок из LPARAM сообщений мыши (знаковые координаты).
fn lparam_pos(lparam: LPARAM) -> (i32, i32) {
    let v = lparam as u32;
    ((v & 0xFFFF) as i16 as i32, ((v >> 16) & 0xFFFF) as i16 as i32)
}

fn invalidate_rect(hwnd: HWND, l: i32, t: i32, r: i32, b: i32) {
    let rc = RECT {
        left: l,
        top: t,
        right: r,
        bottom: b,
    };
    unsafe {
        InvalidateRect(hwnd, &rc, 0);
    }
}

fn invalidate_all(hwnd: HWND) {
    unsafe {
        InvalidateRect(hwnd, null(), 1);
    }
}

fn make_button(
    parent: HWND,
    id: u32,
    text: &str,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    extra: u32,
) -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            w("BUTTON").as_ptr(),
            w(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | extra,
            x,
            y,
            cx,
            cy,
            parent,
            id as usize as *mut core::ffi::c_void as *mut core::ffi::c_void,
            hinst(),
            null(),
        )
    }
}

fn apply_font(hwnd: HWND, ids: &[u32]) {
    unsafe {
        let font = GetStockObject(DEFAULT_GUI_FONT);
        for &id in ids {
            let child = GetDlgItem(hwnd, id as i32);
            if !child.is_null() {
                SendMessageW(child, WM_SETFONT, font as WPARAM, 1);
            }
        }
    }
}

fn update_status_tool() {
    WINS.with(|wi| {
        let ws = wi.borrow();
        if ws.status.is_null() {
            return;
        }
        let (tool, color, cw, ch) = APP.with(|a| {
            let app = a.borrow();
            let app = app.as_ref().unwrap();
            (app.tool, app.color, app.canvas.width, app.canvas.height)
        });
        let text = format!(
            "{} | цвет #{:06X} | полотно {}×{} | Ctrl+Z отменить, Ctrl+Y повторить",
            tool.name(),
            color & 0xFF_FF_FF,
            cw,
            ch
        );
        unsafe {
            SetWindowTextW(ws.status, w(&text).as_ptr());
        }
    });
}

fn update_status_xy(x: i32, y: i32) {
    WINS.with(|wi| {
        let ws = wi.borrow();
        if ws.status.is_null() {
            return;
        }
        let (tool, color) = APP.with(|a| {
            let app = a.borrow();
            let app = app.as_ref().unwrap();
            (app.tool, app.color)
        });
        let text = format!("x={} y={} | {} | #{:06X}", x, y, tool.name(), color & 0xFF_FF_FF);
        unsafe {
            SetWindowTextW(ws.status, w(&text).as_ptr());
        }
    });
}

// ---------------------------------------------------------------------------
// Полотно: DIB + вывод
// ---------------------------------------------------------------------------

/// Пересоздаёт Mem-DC под размер (w, h) и синхронизирует размер полотна.
fn ensure_canvas(hwnd: HWND, w: i32, h: i32) {
    let w = w.max(1);
    let h = h.max(1);
    APP.with(|a| {
        if let Some(app) = a.borrow_mut().as_mut() {
            app.canvas.resize_preserve(w, h);
        }
    });
    WINS.with(|wi| {
        let mut ws = wi.borrow_mut();
        if !ws.mdc.is_null() {
            // вернуть старый объект и освободить
            unsafe {
                SelectObject(ws.mdc, ws.old);
                DeleteObject(ws.dib);
                DeleteDC(ws.mdc);
            }
            ws.mdc = null_mut();
        }
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return;
            }
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = w;
            bmi.bmiHeader.biHeight = -h; // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut core::ffi::c_void = null_mut();
            let hbmp = CreateDIBSection(dc, &bmi, 0, &mut bits, null_mut(), 0);
            if hbmp.is_null() {
                DeleteDC(dc);
                return;
            }
            let old = SelectObject(dc, hbmp as HGDIOBJ);
            ws.mdc = dc;
            ws.dib = hbmp as HGDIOBJ;
            ws.old = old;
            ws.bits = bits as *mut u32;
            ws.cw = w;
            ws.ch = h;
        }
    });
    unsafe {
        InvalidateRect(hwnd, null(), 0);
    }
}

/// Копирует буфер полотна в DIB и выводит на экран.
fn paint_canvas(hdc: HDC) {
    WINS.with(|wi| {
        let ws = wi.borrow();
        if ws.mdc.is_null() || ws.bits.is_null() {
            return;
        }
        let src = APP.with(|a| {
            a.borrow()
                .as_ref()
                .map(|app| app.canvas.buf.as_ptr())
                .unwrap_or(null())
        });
        if !src.is_null() {
            let n = (ws.cw as usize) * (ws.ch as usize);
            unsafe {
                std::ptr::copy_nonoverlapping(src, ws.bits, n);
            }
        }
        unsafe {
            BitBlt(hdc, 0, 0, ws.cw, ws.ch, ws.mdc, 0, 0, SRCCOPY);
        }
    });
}

extern "system" fn canvas_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            let w = (lparam & 0xFFFF) as i32;
            let h = ((lparam >> 16) & 0xFFFF) as i32;
            ensure_canvas(hwnd, w, h);
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            paint_canvas(hdc);
            unsafe {
                EndPaint(hwnd, &ps);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = lparam_pos(lparam);
            APP.with(|a| {
                if let Some(app) = a.borrow_mut().as_mut() {
                    app.begin_stroke(x, y);
                }
            });
            unsafe {
                SetCapture(hwnd);
            }
            invalidate_all(hwnd);
            update_status_full();
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = lparam_pos(lparam);
            let dirty = APP.with(|a| {
                a.borrow_mut()
                    .as_mut()
                    .and_then(|app| app.move_stroke(x, y))
            });
            if let Some((l, t, r, b)) = dirty {
                invalidate_rect(hwnd, l, t, r, b);
            }
            update_status_xy(x, y);
            0
        }
        WM_LBUTTONUP => {
            APP.with(|a| {
                if let Some(app) = a.borrow_mut().as_mut() {
                    app.end_stroke();
                }
            });
            unsafe {
                ReleaseCapture();
            }
            update_status_full();
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

// ---------------------------------------------------------------------------
// Действия приложения
// ---------------------------------------------------------------------------

fn do_undo() {
    APP.with(|a| {
        if let Some(app) = a.borrow_mut().as_mut() {
            app.undo();
        }
    });
    invalidate_all(canvas_hwnd());
    update_status_full();
}

fn do_redo() {
    APP.with(|a| {
        if let Some(app) = a.borrow_mut().as_mut() {
            app.redo();
        }
    });
    invalidate_all(canvas_hwnd());
    update_status_full();
}

fn do_clear() {
    APP.with(|a| {
        if let Some(app) = a.borrow_mut().as_mut() {
            app.clear_canvas();
        }
    });
    invalidate_all(canvas_hwnd());
    update_status_full();
}

fn do_color(hwnd: HWND) {
    // Копируем кастомные цвета в локальную переменную и не держим заимствование
    // WINS на время модального диалога (иначе реентерабельный вызов упадёт в RefCell).
    let mut custom = WINS.with(|wi| wi.borrow().custom);
    let cur = APP.with(|a| a.borrow().as_ref().map(|x| x.color).unwrap_or(0));
    let mut cc: CHOOSECOLORW = unsafe { std::mem::zeroed() };
    cc.lStructSize = size_of::<CHOOSECOLORW>() as u32;
    cc.hwndOwner = hwnd;
    cc.lpCustColors = custom.as_mut_ptr();
    cc.rgbResult = cur;
    cc.Flags = 0x0000_0001 | 0x0000_0002; // CC_RGBINIT | CC_FULLOPEN
    let ok = unsafe { ChooseColorW(&mut cc) } != 0;
    WINS.with(|wi| wi.borrow_mut().custom = custom);
    if ok {
        APP.with(|a| {
            if let Some(app) = a.borrow_mut().as_mut() {
                app.color = cc.rgbResult;
            }
        });
    }
}

fn do_open(hwnd: HWND) {
    let filter = w("PNG (*.png)\0*.png\0\0");
    let mut path = [0u16; 1024];
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = path.as_mut_ptr();
    ofn.nMaxFile = 1024;
    ofn.nFilterIndex = 1;
    ofn.Flags = OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST | OFN_HIDEREADONLY;
    let ok = unsafe { GetOpenFileNameW(&mut ofn) } != 0;
    if ok {
        let len = path.iter().position(|&c| c == 0).unwrap_or(0);
        let fname = String::from_utf16_lossy(&path[..len]);
        let res = APP.with(|a| {
            a.borrow_mut()
                .as_mut()
                .map(|app| app.load_file(&fname))
                .unwrap_or(Err("нет состояния".into()))
        });
        match res {
            Ok(()) => {
                // сброс размера DIB под новое полотно
                WINS.with(|wi| {
                    let ws = wi.borrow();
                    if !ws.canvas.is_null() {
                        unsafe { InvalidateRect(ws.canvas, null(), 1); }
                    }
                });
                // пересоздать mem-dc, если размер поменялся
                let (w, h) = APP.with(|a| {
                    let app = a.borrow();
                    let app = app.as_ref().unwrap();
                    (app.canvas.width, app.canvas.height)
                });
                let canvas = canvas_hwnd();
                ensure_canvas(canvas, w, h);
                update_status_full();
            }
            Err(e) => update_status_msg(&format!("Ошибка открытия: {}", e)),
        }
    }
}

fn do_save(hwnd: HWND) {
    let filter = w("PNG (*.png)\0*.png\0\0");
    let default_ext = w("png");
    let mut path = [0u16; 1024];
    let default_name = w("рисунок.png");
    path[..default_name.len().min(1023)].copy_from_slice(&default_name[..default_name.len().min(1023)]);
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = path.as_mut_ptr();
    ofn.nMaxFile = 1024;
    ofn.nFilterIndex = 1;
    ofn.lpstrDefExt = default_ext.as_ptr();
    ofn.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY;
    let ok = unsafe { GetSaveFileNameW(&mut ofn) } != 0;
    if ok {
        let len = path.iter().position(|&c| c == 0).unwrap_or(0);
        let fname = String::from_utf16_lossy(&path[..len]);
        let res = APP.with(|a| {
            a.borrow()
                .as_ref()
                .map(|app| app.save_file(&fname))
                .unwrap_or(Err("нет состояния".into()))
        });
        match res {
            Ok(()) => update_status_msg(&format!("Сохранено: {}", fname)),
            Err(e) => update_status_msg(&format!("Ошибка сохранения: {}", e)),
        }
    }
}

fn update_status_msg(text: &str) {
    WINS.with(|wi| {
        let ws = wi.borrow();
        if !ws.status.is_null() {
            unsafe {
                SetWindowTextW(ws.status, w(text).as_ptr());
            }
        }
    });
}

fn update_status_full() {
    update_status_tool();
}

fn canvas_hwnd() -> HWND {
    WINS.with(|wi| wi.borrow().canvas)
}

// ---------------------------------------------------------------------------
// Создание интерфейса
// ---------------------------------------------------------------------------

fn create_ui(hwnd: HWND) {
    // --- ряд 1: инструменты ---
    make_button(hwnd, ID_T_PENCIL, "Карандаш", 6, 4, 90, 24, (BS_AUTORADIOBUTTON as u32) | WS_GROUP);
    make_button(hwnd, ID_T_BRUSH, "Кисть", 102, 4, 56, 24, BS_AUTORADIOBUTTON as u32);
    make_button(hwnd, ID_T_ERASER, "Ластик", 162, 4, 62, 24, BS_AUTORADIOBUTTON as u32);
    make_button(hwnd, ID_T_LINE, "Линия", 228, 4, 56, 24, BS_AUTORADIOBUTTON as u32);
    make_button(hwnd, ID_T_RECT, "Прямоугольник", 288, 4, 122, 24, BS_AUTORADIOBUTTON as u32);
    make_button(hwnd, ID_T_ELLIPSE, "Эллипс", 414, 4, 60, 24, BS_AUTORADIOBUTTON as u32);
    make_button(hwnd, ID_T_FILL, "Заливка", 478, 4, 66, 24, BS_AUTORADIOBUTTON as u32);
    make_button(hwnd, ID_UNDO, "Отменить", 560, 4, 78, 24, 0);
    make_button(hwnd, ID_REDO, "Повторить", 642, 4, 78, 24, 0);

    // --- ряд 2: толщина, цвет, файл ---
    unsafe {
        let lbl = CreateWindowExW(
            0,
            w("STATIC").as_ptr(),
            w("Толщина:").as_ptr(),
            WS_CHILD | WS_VISIBLE,
            6,
            34,
            64,
            20,
            hwnd,
            ID_LABEL_TH as usize as *mut core::ffi::c_void,
            hinst(),
            null(),
        );
        SendMessageW(lbl, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as WPARAM, 1);
    }
    let combo = unsafe {
        CreateWindowExW(
            0,
            w("ComboBox").as_ptr(),
            null(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | (CBS_DROPDOWNLIST as u32),
            74,
            30,
            56,
            200,
            hwnd,
            ID_COMBO as usize as *mut core::ffi::c_void,
            hinst(),
            null(),
        )
    };
    for t in THICKNESSES {
        unsafe {
            SendMessageW(combo, CB_ADDSTRING, 0, w(&t.to_string()).as_ptr() as LPARAM);
        }
    }
    unsafe {
        SendMessageW(combo, CB_SETCURSEL, 2, 0); // толщина 3
        SendMessageW(combo, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as WPARAM, 1);
    }
    make_button(hwnd, ID_COLOR, "Цвет…", 136, 30, 66, 24, 0);
    let swatch = unsafe {
        CreateWindowExW(
            0,
            w("BUTTON").as_ptr(),
            null(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_OWNERDRAW as u32),
            206,
            30,
            28,
            24,
            hwnd,
            ID_SWATCH as usize as *mut core::ffi::c_void,
            hinst(),
            null(),
        )
    };
    make_button(hwnd, ID_CLEAR, "Очистить", 240, 30, 84, 24, 0);
    make_button(hwnd, ID_OPEN, "Открыть…", 330, 30, 84, 24, 0);
    make_button(hwnd, ID_SAVE, "Сохранить…", 420, 30, 96, 24, 0);

    apply_font(
        hwnd,
        &[
            ID_T_PENCIL, ID_T_BRUSH, ID_T_ERASER, ID_T_LINE, ID_T_RECT, ID_T_ELLIPSE, ID_T_FILL,
            ID_UNDO, ID_REDO, ID_CLEAR, ID_COLOR, ID_OPEN, ID_SAVE, ID_COMBO, ID_LABEL_TH,
        ],
    );

    // --- статус ---
    let status = unsafe {
        CreateWindowExW(
            0,
            w("STATIC").as_ptr(),
            w("Готово").as_ptr(),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            1,
            STATUS_H,
            hwnd,
            ID_STATUS as usize as *mut core::ffi::c_void,
            hinst(),
            null(),
        )
    };
    unsafe {
        SendMessageW(status, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as WPARAM, 1);
    }

    // --- полотно ---
    let mut rc: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        GetClientRect(hwnd, &mut rc);
    }
    let (cw, ch) = (rc.right, rc.bottom);
    let canvas = unsafe {
        CreateWindowExW(
            0,
            w("TPaintCanvas").as_ptr(),
            null(),
            WS_CHILD | WS_VISIBLE,
            0,
            TOOLBAR_H,
            cw,
            ch - TOOLBAR_H - STATUS_H,
            hwnd,
            null_mut(),
            hinst(),
            null(),
        )
    };

    WINS.with(|wi| {
        let mut ws = wi.borrow_mut();
        ws.main = hwnd;
        ws.canvas = canvas;
        ws.swatch = swatch;
        ws.status = status;
        ws.combo = combo;
    });

    // состояние приложения
    APP.with(|a| {
        *a.borrow_mut() = Some(App::new(cw, ch - TOOLBAR_H - STATUS_H));
    });

    // стартовое состояние инструментов
    unsafe {
        SendMessageW(GetDlgItem(hwnd, ID_T_PENCIL as i32), BM_SETCHECK, 1, 0);
    }
    update_status_tool();
}

// ---------------------------------------------------------------------------
// Главное окно
// ---------------------------------------------------------------------------

extern "system" fn main_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_ui(hwnd);
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as u32;
            let notify = (wparam >> 16) as u32;
            if notify == CBN_SELCHANGE && id == ID_COMBO {
                let sel = unsafe { SendMessageW(WINS.with(|wi| wi.borrow().combo), CB_GETCURSEL, 0, 0) };
                if sel >= 0 && (sel as usize) < THICKNESSES.len() {
                    let t = THICKNESSES[sel as usize];
                    APP.with(|a| {
                        if let Some(app) = a.borrow_mut().as_mut() {
                            app.thickness = t;
                        }
                    });
                }
                update_status_full();
                0
            } else {
                match id {
                    ID_T_PENCIL..=ID_T_FILL => {
                        let idx = id - ID_T_PENCIL;
                        APP.with(|a| {
                            if let Some(app) = a.borrow_mut().as_mut() {
                                app.set_tool(Tool::from_idx(idx));
                            }
                        });
                        update_status_full();
                        0
                    }
                    ID_UNDO => {
                        do_undo();
                        0
                    }
                    ID_REDO => {
                        do_redo();
                        0
                    }
                    ID_CLEAR => {
                        do_clear();
                        0
                    }
                    ID_COLOR => {
                        do_color(hwnd);
                        let sw = WINS.with(|wi| wi.borrow().swatch);
                        if !sw.is_null() {
                            unsafe {
                                InvalidateRect(sw, null(), 1);
                            }
                        }
                        update_status_full();
                        0
                    }
                    ID_OPEN => {
                        do_open(hwnd);
                        0
                    }
                    ID_SAVE => {
                        do_save(hwnd);
                        0
                    }
                    _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
                }
            }
        }
        WM_DRAWITEM => {
            let dis = lparam as *const DRAWITEMSTRUCT;
            if dis.is_null() {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            unsafe {
                draw_swatch(&*dis);
            }
            0
        }
        WM_KEYDOWN => {
            let ctrl = unsafe { GetKeyState(VK_CONTROL as i32) } as i32 & 0x8000 != 0;
            if ctrl {
                match wparam as u32 {
                    0x5A => do_undo(),        // Z
                    0x59 => do_redo(),        // Y
                    0x4F => do_open(hwnd),    // O
                    0x53 => do_save(hwnd),    // S
                    _ => {}
                }
            }
            0
        }
        WM_SIZE => {
            let cw = (lparam & 0xFFFF) as i32;
            let ch = ((lparam >> 16) & 0xFFFF) as i32;
            // Забираем хэндлы заранее: MoveWindow синхронно шлёт WM_SIZE дочернему
            // окну, а тот заходит в ensure_canvas -> WINS.borrow_mut() (реентерабельность).
            let (status, canvas) = WINS.with(|wi| {
                let ws = wi.borrow();
                (ws.status, ws.canvas)
            });
            if !status.is_null() {
                unsafe {
                    MoveWindow(status, 0, ch - STATUS_H, cw, STATUS_H, 1);
                }
            }
            if !canvas.is_null() {
                unsafe {
                    MoveWindow(canvas, 0, TOOLBAR_H, cw, (ch - TOOLBAR_H - STATUS_H).max(1), 1);
                }
            }
            0
        }
        WM_GETMINMAXINFO => {
            let mm = lparam as *mut MINMAXINFO;
            unsafe {
                (*mm).ptMinTrackSize = POINT { x: 960, y: 640 };
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn draw_swatch(dis: &DRAWITEMSTRUCT) {
    let color = APP.with(|a| a.borrow().as_ref().map(|x| x.color).unwrap_or(0x00_FF_FF_FF));
    unsafe {
        let brush = CreateSolidBrush(color);
        let rc = dis.rcItem;
        FillRect(dis.hDC, &rc, brush as HBRUSH);
        DeleteObject(brush as HGDIOBJ);
        let mut rc2 = rc;
        InflateRect(&mut rc2, -1, -1);
        let border = CreateSolidBrush(0x00_80_80_80);
        FrameRect(dis.hDC, &rc2, border as HBRUSH);
        DeleteObject(border as HGDIOBJ);
    }
}

// ---------------------------------------------------------------------------
// Точка входа
// ---------------------------------------------------------------------------

fn main() {
    unsafe {
        let hinst = hinst();

        let main_cls = w("TPaintMain");
        let wc: WNDCLASSW = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(main_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: GetStockObject(WHITE_BRUSH) as HBRUSH,
            lpszMenuName: null(),
            lpszClassName: main_cls.as_ptr(),
        };
        RegisterClassW(&wc);

        let canvas_cls = w("TPaintCanvas");
        let cc: WNDCLASSW = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(canvas_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_CROSS),
            hbrBackground: GetStockObject(WHITE_BRUSH) as HBRUSH,
            lpszMenuName: null(),
            lpszClassName: canvas_cls.as_ptr(),
        };
        RegisterClassW(&cc);

        let title = w("TPaint — растровый редактор (аналог SayPaint)");
        let hwnd = CreateWindowExW(
            0,
            main_cls.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1080,
            740,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        );
        if hwnd.is_null() {
            return;
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}