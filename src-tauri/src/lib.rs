#[cfg(debug_assertions)]
use specta_typescript::Typescript;
use tauri::WindowEvent;
use tauri_specta::{Builder as SpectaBuilder, collect_commands, collect_events};

mod app;
pub mod benchmark;
mod commands;
mod download;
mod events;
mod models;
mod pipeline;
mod settings;
mod state;
mod tools;
mod tracing_setup;

use commands::{
    cancel_model_download, cancel_tool_download, check_yt_dlp_update, connect_feed,
    get_app_snapshot, get_model_catalog, get_recent_logs, get_tool_statuses, hide_window,
    install_model, install_yt_dlp, quit_app, reset_app, set_core_ml_enabled, show_window,
    start_session, stop_session, system_yt_dlp_available, uninstall_model, update_settings,
};
use events::{
    DiagnosticsEvent, MetricsEvent, ModelDownloadEvent, SessionErrorEvent, SessionStateEvent,
    SettingsUpdatedEvent, SubtitleClearEvent, SubtitleEvent, SubtitlePartialEvent,
};
use state::AppState;

fn make_specta_builder() -> SpectaBuilder<tauri::Wry> {
    SpectaBuilder::<tauri::Wry>::new()
        .commands(collect_commands![
            get_app_snapshot,
            start_session,
            stop_session,
            update_settings,
            show_window,
            quit_app,
            connect_feed,
            get_recent_logs,
            get_tool_statuses,
            get_model_catalog,
            install_model,
            uninstall_model,
            set_core_ml_enabled,
            cancel_model_download,
            hide_window,
            system_yt_dlp_available,
            check_yt_dlp_update,
            install_yt_dlp,
            cancel_tool_download,
            reset_app,
        ])
        .events(collect_events![
            SessionStateEvent,
            SessionErrorEvent,
            SubtitleEvent,
            SubtitlePartialEvent,
            SubtitleClearEvent,
            MetricsEvent,
            DiagnosticsEvent,
            SettingsUpdatedEvent,
            ModelDownloadEvent,
        ])
}

/// # Panics
///
/// Panics if the debug-only TypeScript binding export fails, or if the Tauri
/// runtime fails to start — both are fatal startup conditions.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _log_guard = tracing_setup::init();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        platform = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        "starting Kaigai"
    );

    let specta_builder = make_specta_builder();
    #[cfg(debug_assertions)]
    {
        let bindings =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/types/bindings.ts");
        specta_builder
            .export(Typescript::default(), bindings)
            .expect("failed to export TypeScript bindings");
    }

    let invoke_handler = specta_builder.invoke_handler();
    let builder_for_setup = specta_builder;

    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(invoke_handler)
        .setup(move |app| {
            builder_for_setup.mount_events(app);
            app::setup(app)
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Kaigai");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_bindings() {
        let output =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/types/bindings.ts");
        make_specta_builder()
            .export(Typescript::default(), &output)
            .expect("failed to export TypeScript bindings");
        assert!(output.exists());
    }
}
