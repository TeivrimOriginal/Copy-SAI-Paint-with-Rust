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
    Gradient,
    Select,
    /// Выделение эллипсом — форма задаётся маской, а не рамкой.
    EllipseSelect,
    /// «Волшебная палочка»: выделение по цвету под курсором.
    Wand,
    /// Многоугольное выделение: клики ставят вершины, Enter замыкает.
    Polygon,
    Text,
    Eyedropper,
    Pan,
}

pub const TOOLS: [Tool; 15] = [
    Tool::Pencil,
    Tool::Brush,
    Tool::Eraser,
    Tool::Line,
    Tool::Rect,
    Tool::Ellipse,
    Tool::Fill,
    Tool::Gradient,
    Tool::Select,
    Tool::EllipseSelect,
    Tool::Wand,
    Tool::Polygon,
    Tool::Text,
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
            Tool::Gradient => "Градиент",
            Tool::Select => "Выделение",
            Tool::EllipseSelect => "Эллипс-выделение",
            Tool::Wand => "Волшебная палочка",
            Tool::Polygon => "Многоугольник",
            Tool::Text => "Текст",
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
            Tool::Gradient => "Линейный градиент от первого цвета ко второму",
            Tool::Select => "Рамка выделения: копировать, вырезать, вставить",
            Tool::EllipseSelect => "Протянуть эллипс — выделение любой формы",
            Tool::Wand => "Выделение по цвету под курсором",
            Tool::Polygon => "Клик — вершина, Enter — замкнуть контур",
            Tool::Text => "Клик по холсту — печатать, Enter — нарисовать",
            Tool::Eyedropper => "Взять цвет из холста",
            Tool::Pan => "Перемещение холста",
        }
    }

    /// Кисть рисует непрерывным мазком (у карандаша и фигур — иначе).
    pub fn is_freehand(self) -> bool {
        matches!(self, Tool::Pencil | Tool::Brush | Tool::Eraser)
    }

    /// Инструмент выделения не рисует пиксели, а двигает рамку.
    pub fn is_selection(self) -> bool {
        matches!(self, Tool::Select | Tool::EllipseSelect)
    }

    /// Выделение, которое набирается кликами по вершинам.
    pub fn is_polygon(self) -> bool {
        matches!(self, Tool::Polygon)
    }

    pub fn needs_color(self) -> bool {
        !matches!(self, Tool::Eraser | Tool::Eyedropper | Tool::Pan | Tool::Select)
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
    /// Мягкость градиента 0..=1.
    pub gradient_soft: f32,
    /// Сглаживание мазка 0..=1 (стабилизатор, как в Clip Studio).
    pub smoothing: f32,
    /// Шаг между отпечатками кисти в долях её диаметра: меньше — плотнее
    /// мазок. Задан в пикселях холста, поэтому не зависит от скорости мыши.
    pub spacing: f32,
    /// Жирный шрифт (инструмент «Текст»).
    pub bold_text: bool,
    /// Форма кисти: круг, эллипс или квадрат.
    pub shape: BrushShape,
    /// Симметрия мазка: по центру, по оси или по лучам.
    pub symmetry: Symmetry,
    /// Число лучей для радиальной симметрии (2…12).
    pub sym_sides: f32,
}

/// Симметрия мазка — как симметричное рисование в Clip Studio: мазок
/// отражается относительно центра холста или повторяется по лучам.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Symmetry {
    Off,
    /// Отражение относительно вертикали через центр (старое «зеркально»).
    Center,
    /// Вертикальная ось: мазок зеркалится слева направо.
    Vertical,
    /// Горизонтальная ось: мазок зеркалится сверху вниз.
    Horizontal,
    /// Радиальная: сектор повторяется по лучам вокруг центра.
    Radial,
}

impl Symmetry {
    pub const ALL: [Symmetry; 5] = [
        Symmetry::Off,
        Symmetry::Center,
        Symmetry::Vertical,
        Symmetry::Horizontal,
        Symmetry::Radial,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Symmetry::Off => "Нет",
            Symmetry::Center => "Центр",
            Symmetry::Vertical => "Вертикаль",
            Symmetry::Horizontal => "Горизонталь",
            Symmetry::Radial => "По лучам",
        }
    }

    /// Следующий режим по кругу — переключение одной кнопкой.
    pub fn next(self) -> Self {
        match self {
            Symmetry::Off => Symmetry::Center,
            Symmetry::Center => Symmetry::Vertical,
            Symmetry::Vertical => Symmetry::Horizontal,
            Symmetry::Horizontal => Symmetry::Radial,
            Symmetry::Radial => Symmetry::Off,
        }
    }

    pub fn on(self) -> bool {
        self != Symmetry::Off
    }
}

/// Форма отпечатка кисти.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BrushShape {
    Round,
    Ellipse,
    Square,
}

impl BrushShape {
    pub fn name(self) -> &'static str {
        match self {
            BrushShape::Round => "Круг",
            BrushShape::Ellipse => "Эллипс",
            BrushShape::Square => "Квадрат",
        }
    }

    /// Следующая форма по кругу — переключается одной кнопкой в панели.
    pub fn next(self) -> Self {
        match self {
            BrushShape::Round => BrushShape::Ellipse,
            BrushShape::Ellipse => BrushShape::Square,
            BrushShape::Square => BrushShape::Round,
        }
    }
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
            gradient_soft: 0.5,
            smoothing: 0.0,
            spacing: 0.08,
            bold_text: false,
            symmetry: Symmetry::Off,
            sym_sides: 6.0,
            shape: BrushShape::Round,
        }
    }
}

/// Готовые кисти — как палитра кистей в Clip Studio. Хранят только те
/// параметры, которые есть у кисти, остальное инструмент задаёт сам.
pub struct BrushPreset {
    pub name: &'static str,
    pub size: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub smoothing: f32,
}

pub const PRESETS: &[BrushPreset] = &[
    BrushPreset { name: "Перо", size: 6.0, opacity: 1.0, hardness: 1.0, smoothing: 0.35 },
    BrushPreset { name: "Тушь", size: 22.0, opacity: 1.0, hardness: 0.95, smoothing: 0.2 },
    BrushPreset { name: "Акварель", size: 70.0, opacity: 0.35, hardness: 0.25, smoothing: 0.4 },
    BrushPreset { name: "Карандаш", size: 3.0, opacity: 0.9, hardness: 1.0, smoothing: 0.0 },
    BrushPreset { name: "Воздух", size: 200.0, opacity: 0.12, hardness: 0.05, smoothing: 0.5 },
    BrushPreset { name: "Маркер", size: 40.0, opacity: 0.7, hardness: 0.6, smoothing: 0.1 },
];

impl BrushPreset {
    /// Применяет пресет к параметрам инструмента.
    pub fn apply(&self, p: &mut Params) {
        p.size = self.size;
        p.opacity = self.opacity;
        p.hardness = self.hardness;
        p.smoothing = self.smoothing;
    }
}

/// Строка панели параметров: что именно показать для активного инструмента.
pub enum ParamRow {
    Size { label: &'static str, min: f32, max: f32 },
    Opacity,
    Hardness,
    Tolerance,
    Smoothing,
    GradientSoft,
    /// Флажок; какой именно параметр он включает — решает инструмент.
    Checkbox { label: &'static str, on: bool },
    /// Симметрия мазка: выпадающий список режимов и число лучей.
    Symmetry,
    /// Форма кисти: круг, эллипс или квадрат.
    Shape,
    /// Шаг между отпечатками кисти.
    Spacing,
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
            ParamRow::Spacing,
            ParamRow::Shape,
            ParamRow::Symmetry,
        ],
        Tool::Eraser => vec![
            ParamRow::Size { label: "Размер", min: 1.0, max: 500.0 },
            ParamRow::Opacity,
            ParamRow::Hardness,
            ParamRow::Smoothing,
            ParamRow::Spacing,
            ParamRow::Shape,
            ParamRow::Symmetry,
        ],
        Tool::Line | Tool::Rect | Tool::Ellipse => {
            let mut v = vec![ParamRow::Size { label: "Толщина", min: 1.0, max: 200.0 }, ParamRow::Opacity];
            if tool != Tool::Line {
                v.push(ParamRow::Checkbox { label: "Заливка фигуры", on: false });
            }
            v
        }
        Tool::Fill => vec![ParamRow::Opacity, ParamRow::Tolerance, ParamRow::Checkbox { label: "Ограничить область", on: true }],
        Tool::Gradient => vec![ParamRow::Opacity, ParamRow::GradientSoft],
        Tool::Select => vec![
            ParamRow::Checkbox { label: "Залить выделение", on: false },
            ParamRow::Tolerance,
        ],
        Tool::EllipseSelect => vec![],
        Tool::Wand => vec![
            ParamRow::Tolerance,
            ParamRow::Checkbox { label: "Ограничить область", on: true },
        ],
        Tool::Polygon => vec![],
        Tool::Text => vec![
            ParamRow::Size { label: "Кегль", min: 8.0, max: 240.0 },
            ParamRow::Opacity,
            ParamRow::Checkbox { label: "Жирный", on: false },
        ],
        Tool::Eyedropper | Tool::Pan => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_presets_change_all_brush_params() {
        let mut p = Params::default();
        for preset in PRESETS {
            preset.apply(&mut p);
            assert_eq!(p.size, preset.size, "размер пресета «{}»", preset.name);
            assert_eq!(p.opacity, preset.opacity, "непрозрачность «{}»", preset.name);
            assert_eq!(p.hardness, preset.hardness, "мягкость «{}»", preset.name);
            assert_eq!(p.smoothing, preset.smoothing, "сглаживание «{}»", preset.name);
            assert!((0.0..=1.0).contains(&p.opacity), "непрозрачность вне 0..1");
            assert!((0.0..=1.0).contains(&p.hardness), "мягкость вне 0..1");
            assert!((0.0..=1.0).contains(&p.smoothing), "сглаживание вне 0..1");
        }
    }

    #[test]
    fn brush_shape_cycles_through_all_three() {
        let mut s = BrushShape::Round;
        assert_eq!(s.next(), BrushShape::Ellipse);
        s = s.next();
        assert_eq!(s.next(), BrushShape::Square);
        s = s.next();
        assert_eq!(s.next(), BrushShape::Round, "цикл замыкается");
        for shape in [BrushShape::Round, BrushShape::Ellipse, BrushShape::Square] {
            assert!(!shape.name().is_empty(), "у формы должно быть имя");
        }
    }

    #[test]
    fn freehand_tools_offer_a_brush() {
        for t in [Tool::Pencil, Tool::Brush, Tool::Eraser] {
            assert!(t.is_freehand(), "{} должен быть мазковым", t.name());
        }
        for t in [Tool::Line, Tool::Rect, Tool::Select, Tool::Text] {
            assert!(!t.is_freehand(), "{} не мазковый", t.name());
        }
    }
}
