//! The interface: pages, widgets, the theme and the icon set.

pub mod theme;
pub mod images;
pub mod widgets;
pub mod captions;
pub mod sidebar;
pub mod cards;
pub mod dialogs;
pub mod ambient;
pub mod settings_view;
pub mod downloads_view;
pub mod grid;
pub mod player_view;
pub mod channel_view;
pub mod panels;
pub mod keys;
pub mod surface;
pub mod icons;
pub mod views;
pub mod music_ui;
pub mod side_tools;
pub mod podcasts_view;
pub mod update_view;
pub mod i18n;

#[allow(unused_imports)]
pub(crate) use theme::*;
pub(crate) use images::*;
pub(crate) use widgets::*;
pub(crate) use captions::*;
pub(crate) use i18n::*;
pub(crate) use cards::*;
