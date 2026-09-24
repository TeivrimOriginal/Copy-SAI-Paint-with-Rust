//! Инструменты и их параметры.
//!
//! Набор параметров зависит от активного инструмента: у кисти есть мягкость,
//! у заливки — допуск, у фигур — заливка контура, у карандаша — только
//! толщина, непрозрачность и сглаживание.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pencil,
    Brush,
    Eraser,
    Line,
    Rect,
    Ellipse,
    Fill,
    Eyedropper,
    Pan,
}

pub const TOOLS: [Tool; 9] = [
    Tool::Pencil,
    Tool::Brush,
    Tool::Eraser,
    Tool::Line,
    Tool::Rect,
    Tool::Ellipse,
    Tool::Fill,
    Tool::Eyedropper,
    Tool::Pan,
];

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pencil => "Карандаш",
            Tool::Brush => "Кисть",
            Tool::Eraser => "Ластик",
            Tool::Line => "Линия",
            Tool::Rect => "Прямоугольник",
            Tool::Ellipse => "Эллипс",
            Tool::Fill => "Заливка",
            Tool::Eyedropper => "Пипетка",
            Tool::Pan => "Рука",
        }
    }

    /// Короткая подсказка для панели параметров.
    pub fn hint(self) -> &'static str {
        match self {
            Tool::Pencil => "Тонкая линия, сглаживание",
            Tool::Brush => "Мягкая кисть с нажимом",
            Tool::Eraser => "Стирает до прозрачности",
            Tool::Line => "Прямая по нажатию и протягиванию",
            Tool::Rect => "Прямоугольник: рамка или заливка",
            Tool::Ellipse => "Эллипс: рамка или заливка",
            Tool::Fill => "Заливка области с допуском",
            Tool::Eyedropper => "Взять цвет из холста",
            Tool::Pan => "Перемещение холста",
        }
    }

    /// Кисть рисует непрерывным мазком (у карандаша и фигур — иначе).
    pub fn is_freehand(self) -> bool {
        matches!(self, Tool::Pencil | Tool::Brush | Tool::Eraser)
    }

    pub fn needs_color(self) -> bool {
        !matches!(self, Tool::Eraser | Tool::Eyedropper | Tool::Pan)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Толщина/размер кисти в пикселях.
    pub size: f32,
    /// Непрозрачность мазка 0..=1.
    pub opacity: f32,
    /// Мягкость края 0..=1 (1 — резкий край).
    pub hardness: f32,
    /// Допуск заливки 0..=255.
    pub tolerance: f32,
    /// Заливать только соприкасающуюся область.
    pub contiguous: bool,
    /// Заливать фигуру целиком (иначе — контур).
    pub shape_fill: bool,
    /// Сглаживание мазка 0..=1 (стабилизатор, как в Clip Studio).
    pub smoothing: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            size: 24.0,
            opacity: 1.0,
            hardness: 0.8,
            tolerance: 0.0,
            contiguous: true,
            shape_fill: false,
            smoothing: 0.0,
        }
    }
}

/// Строка панели параметров: что именно показать для активного инструмента.
pub enum ParamRow {
    Size { label: &'static str, min: f32, max: f32 },
    Opacity,
    Hardness,
    Tolerance,
    Smoothing,
    Checkbox { label: &'static str, on: bool },
}

pub fn rows_for(tool: Tool) -> Vec<ParamRow> {
    match tool {
        Tool::Pencil => vec![
            ParamRow::Size { label: "Толщина", min: 1.0, max: 200.0 },
            ParamRow::Opacity,
            ParamRow::Smoothing,
        ],
        Tool::Brush => vec![
            ParamRow::Size { label: "Размер", min: 1.0, max: 500.0 },
            ParamRow::Opacity,
            ParamRow::Hardness,
            ParamRow::Smoothing,
        ],
        Tool::Eraser => vec![
            ParamRow::Size { label: "Размер", min: 1.0, max: 500.0 },
            ParamRow::Opacity,
            ParamRow::Hardness,
            ParamRow::Smoothing,
        ],
        Tool::Line | Tool::Rect | Tool::Ellipse => {
            let mut v = vec![ParamRow::Size { label: "Толщина", min: 1.0, max: 200.0 }, ParamRow::Opacity];
            if tool != Tool::Line {
                v.push(ParamRow::Checkbox { label: "Заливка фигуры", on: false });
            }
            v
        }
        Tool::Fill => vec![ParamRow::Opacity, ParamRow::Tolerance, ParamRow::Checkbox { label: "Ограничить область", on: true }],
        Tool::Eyedropper | Tool::Pan => vec![],
    }
}
