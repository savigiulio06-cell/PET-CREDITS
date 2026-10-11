//! Tauri application: window, commands, tray, background threads.

use std::sync::mpsc;

use pet_core::{paths, UsageSnapshot};
use serde::Serialize;
use tauri::{Emitter, Manager, State, WebviewWindow, WindowEvent};

use crate::config::Config;
use crate::state::AppState;
use crate::{poller, tray, window};

/// Payload of the `pet-moved` event: physical position of the window.
#[derive(Clone, Serialize)]
struct PetMoved {
    x: i32,
    y: i32,
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> UsageSnapshot {
    state.snapshot()
}

/// The saved skin id; the UI asks for it on load (an event emitted at start-up
/// could fire before the page is listening).
#[tauri::command]
fn get_variant(state: State<'_, AppState>) -> String {
    state.config().variant
}

#[tauri::command]
fn set_expanded(window: WebviewWindow, expanded: bool) -> Result<(), String> {
    window::set_expanded(&window, expanded)
}

/// Right click on the pet: open the same menu as the tray, at the cursor.
/// Synchronous on purpose: Tauri runs it on the main thread, which the native
/// popup needs.
#[tauri::command]
fn show_menu(window: WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    match state.menu.get() {
        Some(menu) => window.popup_menu(menu).map_err(|e| e.to_string()),
        None => Err("menu not available".into()),
    }
}

pub fn run() {
    let app_home = paths::app_home();
    let config = Config::load(&app_home);

    let result = tauri::Builder::default()
        // Must be the first plugin: a second launch shows the running pet and exits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show_main(app);
        }))
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_variant,
            set_expanded,
            show_menu
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let (poll_tx, poll_rx) = mpsc::channel();
            let (moved_tx, moved_rx) = mpsc::channel();
            app.manage(AppState::new(config.clone(), app_home, poll_tx, moved_tx));

            let handle = app.handle().clone();
            let win = app
                .get_webview_window(window::LABEL)
                .ok_or("main window missing")?;

            let moved = handle.clone();
            win.on_window_event(move |event| match event {
                WindowEvent::Moved(pos) => {
                    // Every step, undebounced: the UI animates the pet while it is dragged.
                    let _ = moved.emit("pet-moved", PetMoved { x: pos.x, y: pos.y });
                    // Only the saving is debounced.
                    let _ = moved.state::<AppState>().moved.send(());
                }
                WindowEvent::CloseRequested { api, .. } => {
                    // Alt+F4 hides the pet; "Esci" in the tray quits.
                    api.prevent_close();
                    if let Some(w) = window::main_window(&moved) {
                        let _ = w.hide();
                    }
                }
                _ => {}
            });

            window::place_and_show(&win, config.position);
            tray::build_guarded(&handle, &config);
            window::spawn_position_saver(handle.clone(), moved_rx);
            poller::spawn(handle, poll_rx, config.use_api);
            Ok(())
        })
        .run(tauri::generate_context!());

    if result.is_err() {
        std::process::exit(1);
    }
}
