//! Pet menu (Italian UI), shown by the system-tray icon and by a right click
//! on the pet itself.

use std::panic::{catch_unwind, AssertUnwindSafe};

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{include_image, AppHandle, Emitter, Manager, Wry};

use crate::cli;
use crate::config::Config;
use crate::poller::Cmd;
use crate::state::AppState;
use crate::variants;
use crate::window;

const VARIANT_PREFIX: &str = "variant:";
/// Embedded at compile time, so the tray never depends on the window icon.
const TRAY_ICON: Image<'static> = include_image!("icons/32x32.png");

/// Build the menu (stored in [`AppState`] for the right-click popup) and the
/// tray icon. Failures are reported on stderr and never stop the pet: on Linux
/// the appindicator library is loaded at run time and its absence panics
/// inside the tray crate, so that panic is caught too.
pub fn build_guarded(app: &AppHandle, cfg: &Config) {
    let menu = match build_menu(app, cfg) {
        Ok(menu) => menu,
        Err(e) => {
            eprintln!("pet-credits: could not build the menu: {e}");
            return;
        }
    };
    let _ = app.state::<AppState>().menu.set(menu.clone());
    match catch_unwind(AssertUnwindSafe(|| build_tray(app, &menu))) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("pet-credits: could not create the tray icon: {e}"),
        Err(_) => eprintln!("pet-credits: the tray icon panicked; continuing without it"),
    }
}

fn build_tray(app: &AppHandle, menu: &Menu<Wry>) -> tauri::Result<()> {
    TrayIconBuilder::with_id("main")
        .icon(TRAY_ICON)
        .tooltip("PET-CREDITS")
        .menu(menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Create the menu and register its (app-wide) event handler, which serves
/// both the tray and the popup on the pet.
fn build_menu(app: &AppHandle, cfg: &Config) -> tauri::Result<Menu<Wry>> {
    let toggle = MenuItem::with_id(app, "toggle", "Mostra/Nascondi", true, None::<&str>)?;

    let variant_items: Vec<CheckMenuItem<_>> = variants::VARIANTS
        .iter()
        .map(|(id, label)| {
            CheckMenuItem::with_id(
                app,
                format!("{VARIANT_PREFIX}{id}"),
                *label,
                true,
                cfg.variant == *id,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    let variant_refs: Vec<&dyn IsMenuItem<_>> = variant_items
        .iter()
        .map(|v| v as &dyn IsMenuItem<_>)
        .collect();
    let look = Submenu::with_items(app, "Aspetto", true, &variant_refs)?;

    let refresh = MenuItem::with_id(app, "refresh", "Aggiorna ora", true, None::<&str>)?;
    let use_api = CheckMenuItem::with_id(
        app,
        "use_api",
        "Usa dati dal server Anthropic (non ufficiale)",
        true,
        cfg.use_api,
        None::<&str>,
    )?;
    let status = MenuItem::with_id(
        app,
        "status",
        "Collegamento a Claude Code: -",
        false,
        None::<&str>,
    )?;
    let install = MenuItem::with_id(app, "install", "Collega a Claude Code", true, None::<&str>)?;
    let uninstall = MenuItem::with_id(
        app,
        "uninstall",
        "Scollega da Claude Code",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Esci", true, None::<&str>)?;
    let sep = |app: &AppHandle| PredefinedMenuItem::separator(app);
    let (s1, s2, s3) = (sep(app)?, sep(app)?, sep(app)?);

    let menu = Menu::with_items(
        app,
        &[
            &toggle, &look, &refresh, &use_api, &s1, &status, &install, &uninstall, &s2, &s3, &quit,
        ],
    )?;

    app.on_menu_event(move |app, event| {
        let id = event.id().as_ref();
        let state = app.state::<AppState>();
        match id {
            "toggle" => window::toggle_main(app),
            "refresh" => {
                let _ = state.poller.send(Cmd::RefreshNow);
            }
            "use_api" => {
                let on = use_api.is_checked().unwrap_or(true);
                state.update_config(|c| c.use_api = on);
                let _ = state.poller.send(Cmd::SetUseApi(on));
            }
            "install" | "uninstall" => {
                let app = app.clone();
                let status = status.clone();
                let do_install = id == "install";
                // File I/O off the event loop.
                let _ = std::thread::Builder::new()
                    .name("claude-link".into())
                    .spawn(move || {
                        let text = match if do_install {
                            cli::install_here()
                        } else {
                            cli::uninstall_here()
                        } {
                            Ok(_) if do_install => "Collegato a Claude Code".to_string(),
                            Ok(_) => "Scollegato da Claude Code".to_string(),
                            Err(e) => format!("Errore: {e}"),
                        };
                        let _ = status.set_text(format!("Collegamento a Claude Code: {text}"));
                        if let Some(tray) = app.tray_by_id("main") {
                            let _ = tray.set_tooltip(Some(format!("PET-CREDITS - {text}")));
                        }
                    });
            }
            "quit" => app.exit(0),
            other => {
                if let Some(variant) = other.strip_prefix(VARIANT_PREFIX) {
                    if variants::is_known(variant) {
                        for item in &variant_items {
                            let selected = item.id().as_ref() == other;
                            let _ = item.set_checked(selected);
                        }
                        state.update_config(|c| c.variant = variant.to_string());
                        let _ = app.emit("set-variant", variant);
                    }
                }
            }
        }
    });
    Ok(menu)
}
