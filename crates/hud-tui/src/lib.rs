//! Terminal user interface of hud, built with the Elm architecture.
//!
//! [`update`] and [`view`] are pure: no I/O, no clock, no terminal access.
//! The binary owns the event loop and the terminal.

mod i18n;
pub mod icons;
mod model;
mod msg;
mod settings;
mod theme;
mod update;
mod view;

pub use model::{Model, Tab};
pub use msg::Msg;
pub use settings::{Appearance, ColorDepth, IconSet, Language, UiSettings};
pub use theme::Theme;
pub use update::update;
pub use view::view;
