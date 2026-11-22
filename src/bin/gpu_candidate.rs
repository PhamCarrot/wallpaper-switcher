#![windows_subsystem = "windows"]

#[path = "../appearance_theme.rs"]
mod appearance_theme;
#[path = "../apply_trace.rs"]
mod apply_trace;
#[path = "../automation.rs"]
mod automation;
#[path = "../candidate_instance.rs"]
mod candidate_instance;
#[path = "../candidate_scenario.rs"]
mod candidate_scenario;
#[path = "../candidate_state.rs"]
mod candidate_state;
#[path = "../config.rs"]
mod config;
#[path = "../coordinator.rs"]
mod coordinator;
#[path = "../d3d11_transition.rs"]
mod d3d11_transition;
#[path = "../desktop_transition.rs"]
mod desktop_transition;
#[path = "../diagnostic_apply.rs"]
mod diagnostic_apply;
#[path = "../donor_back_layout.rs"]
mod donor_back_layout;
#[path = "../donor_back_view.rs"]
mod donor_back_view;
#[path = "../donor_detail.rs"]
mod donor_detail;
#[path = "../donor_folder.rs"]
mod donor_folder;
#[path = "../donor_folio.rs"]
mod donor_folio;
#[path = "../donor_layout.rs"]
mod donor_layout;
#[path = "../donor_rail.rs"]
mod donor_rail;
#[path = "../donor_scene.rs"]
mod donor_scene;
#[path = "../donor_search.rs"]
mod donor_search;
#[path = "../donor_ui.rs"]
mod donor_ui;
#[path = "../engine_capture.rs"]
mod engine_capture;
#[path = "../engine_ipc.rs"]
mod engine_ipc;
#[path = "../engine_properties.rs"]
mod engine_properties;
#[path = "../engine_volume.rs"]
mod engine_volume;
#[path = "../file_publication.rs"]
mod file_publication;
#[path = "../gpu_gallery.rs"]
mod gpu_gallery;
#[path = "../hotkey.rs"]
mod hotkey;
#[path = "../library.rs"]
mod library;
#[path = "../monitor.rs"]
mod monitor;
#[path = "../native_wallpaper.rs"]
mod native_wallpaper;
#[path = "../poster_worker.rs"]
mod poster_worker;
#[path = "../rgb_color.rs"]
mod rgb_color;
#[path = "../scanner.rs"]
mod scanner;
#[allow(dead_code)]
#[path = "../theme_roles.rs"]
mod theme_roles;
#[path = "../transition.rs"]
mod transition;
#[path = "../tray.rs"]
mod tray;
#[path = "../upstream_slices_adapter.rs"]
mod upstream_slices_adapter;

#[cfg(test)]
#[path = "../f7_measurement.rs"]
mod f7_measurement;

#[path = "../candidate_app/mod.rs"]
mod candidate_app;

pub use config::log_debug;

#[cfg(test)]
use candidate_app::f7_support::{
    filtered_poster_aspects, gallery_layout_params, poster_indices_for_view,
};

fn main() -> iced::Result {
    engine_ipc::dispatch_readback_helper();
    candidate_app::run()
}
