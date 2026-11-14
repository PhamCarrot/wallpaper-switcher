//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
use super::input::InputMap;

pub use super::capabilities::{GraphicsCard, GraphicsDevice, GraphicsTier};
use crate::candidate_app::localization::Catalog;

pub trait SettingsSource {
    fn catalog(&self) -> &Catalog;
    fn language_persistence_note(&self) -> Option<&'static str> {
        None
    }
    fn stash_available(&self) -> bool {
        true
    }
    fn flag(&self, path: &str) -> bool;
    fn flag_default_true(&self, path: &str) -> bool;
    fn text(&self, path: &str) -> String;
    fn number(&self, path: &str) -> f64;
    fn array_len(&self, path: &str) -> usize;
    fn default_semantic_model_available(&self) -> bool;

    fn display_mode(&self) -> String;
    fn selector_mode(&self) -> String;
    fn mode_profile_rows(
        &self,
        mode: crate::candidate_state::GalleryKind,
    ) -> Vec<crate::candidate_state::mode_profiles::ProfileRow>;
    fn launch_animation(&self) -> String;
    fn theme_backend(&self) -> String;

    fn motion_fast_ms(&self) -> f32;
    fn motion_standard_ms(&self) -> f32;
    fn motion_slow_ms(&self) -> f32;

    fn saved_theme_names(&self) -> Vec<String>;
    fn palette_colors(&self) -> Vec<(String, Vec<String>)>;
    fn palette_presets(&self) -> Vec<(String, String)>;
    fn bindings(&self) -> InputMap;
    fn graphics_card(&self) -> GraphicsCard;
    fn graphics_devices(&self) -> Vec<super::capabilities::GraphicsDevice>;
    fn is_niri(&self) -> bool;
    fn on_battery_power(&self) -> Option<bool>;
}
