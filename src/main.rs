//! KuroCat: a transparent desktop pet for Windows that reacts to your keyboard and mouse.
//!
//! Run without arguments to start the pet. Run with `--mcp` to start only the stdio MCP server
//! (no window); it edits the same settings and cat files that the running pet watches.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "256"]

mod app;
mod audio;
mod banner;
mod cats;
mod config;
mod engine;
mod events;
mod hit_test;
mod input_hook;
mod keyboard_layout;
mod mcp;
mod menus;
mod privacy;
mod render;
mod skins;
mod tray;
mod window;

/// Program entry point.
fn main() {
    if std::env::args().any(|argument| argument == "--mcp") {
        mcp::run_stdio_mcp();
        return;
    }
    app::run();
}
