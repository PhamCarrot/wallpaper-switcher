//! Source-derived pure control projection; explicit locale and -owned consumers.
use crate::candidate_app::view::settings::composed::keys;

use super::language_choice;
use super::tables::{motion_speed_options, NIRI_SNIPPET, POST_TYPES};
use super::{ActionId, Builder, Card, Control, Row, PRESET_NAME_KEY};

mod app_themes;
mod builder;
mod launch_tab;
mod media_tabs;
mod motion_tab;
mod position_tab;
mod selector_tab;
mod source_defaults;
mod system_tabs;
mod theme_tabs;

pub use builder::build_tab;
#[cfg(test)]
pub(crate) use builder::build_tab_with_output_statuses;
#[cfg(test)]
pub(crate) use builder::build_tab_with_outputs;
pub(crate) use builder::build_tab_with_runtime_status;
#[cfg(test)]
pub(crate) use builder::compatibility_owner_rows;
