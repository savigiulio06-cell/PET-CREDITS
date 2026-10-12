// No console window for the release build on Windows. The CLI modes that print
// attach to the parent console explicitly (see `cli`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod cli;
mod config;
mod poller;
mod prefs;
mod state;
mod tray;
mod variants;
mod watch;
mod window;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::dispatch(&args) {
        std::process::exit(code);
    }
    app::run();
}
