//! Tpaint — растровый редактор (в духе Clip Studio) на Rust + GLFW + OpenGL.
//!
//! Библиотека вынесена отдельно от бинарников, чтобы один и тот же движок
//! интерфейса работал и в редакторе (`tpaint`), и в лаунчере (`TpaintLauncher`):
//! окно, шрифты, OpenGL-рендерер и все виджеты у них общие.

pub mod app;
pub mod doc;
pub mod layout;
pub mod palette;
pub mod project;
pub mod raster;
pub mod renderer;
pub mod text;
pub mod tools;
pub mod ui;

/// Имя приложения во всех заголовках и сообщениях.
pub const APP_NAME: &str = "Tpaint";
