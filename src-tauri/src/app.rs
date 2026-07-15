use tauri::{
    App, AppHandle, Manager,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

use crate::{commands::apply_overlay_settings, settings, state::AppState, tools};

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(debug_assertions)]
    setup_developer_window(app)?;

    if let Err(error) = settings::migrate_legacy_storage(app.handle()) {
        tracing::warn!(%error, "could not migrate legacy Kaigai storage");
    }
    let current_settings = settings::load(app.handle()).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not load settings; using defaults");
        settings::AppSettings::default()
    });
    *app.state::<AppState>()
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")? = current_settings.clone();
    apply_overlay_settings(app.handle(), &current_settings)?;
    setup_tray(app)?;

    let handle = app.handle().clone();
    let settings_for_update = current_settings.clone();
    tauri::async_runtime::spawn(async move {
        tools::maybe_auto_update_yt_dlp(handle, &settings_for_update).await;
    });

    // First run shows the setup tour; afterwards the bar opens directly.
    let first_window = if current_settings.onboarded {
        "main"
    } else {
        "onboarding"
    };
    show_and_focus(app.handle(), first_window);
    Ok(())
}

#[cfg(debug_assertions)]
fn setup_developer_window(app: &mut App) -> tauri::Result<()> {
    tauri::WebviewWindowBuilder::new(
        app,
        "developer",
        tauri::WebviewUrl::App("developer/".into()),
    )
    .title("Kaigai Developer")
    .inner_size(1240.0, 960.0)
    .min_inner_size(960.0, 720.0)
    .center()
    .visible(false)
    .build()?;
    Ok(())
}

pub fn show_and_focus(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let show_bar = MenuItem::with_id(app, "show_bar", "Show Kaigai", true, None::<&str>)?;
    let open_settings = MenuItem::with_id(app, "open_settings", "Settings…", true, None::<&str>)?;
    #[cfg(debug_assertions)]
    let open_developer = MenuItem::with_id(
        app,
        "open_developer",
        "Developer console",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    #[cfg(debug_assertions)]
    let menu = Menu::with_items(app, &[&show_bar, &open_settings, &open_developer, &quit])?;
    #[cfg(not(debug_assertions))]
    let menu = Menu::with_items(app, &[&show_bar, &open_settings, &quit])?;

    TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or("missing app icon")?,
        )
        .tooltip("Kaigai")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_bar" => show_and_focus(app, "main"),
            "open_settings" => show_and_focus(app, "settings"),
            #[cfg(debug_assertions)]
            "open_developer" => show_and_focus(app, "developer"),
            "quit" => crate::commands::shutdown(app),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
