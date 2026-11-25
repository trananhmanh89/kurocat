# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

KuroCat is a transparent, always-on-top desktop pet for **Windows 10/11**: a cat over a keyboard and mouse that reacts to the user's global keyboard and mouse input. It is **pure Rust with no WebView**:
- Win32 global input hooks.
- A Canvas-2D-like CPU renderer built on `tiny-skia`, drawn into a per-pixel-alpha layered window with `UpdateLayeredWindow`.
- Native menus (`muda`, `tray-icon`) and a purr synth (`cpal`).

macOS, Linux, Win7 and the old web demo were dropped in v5.

## Commands

```bash
cargo build --release               # build target/release/KuroCat.exe
pwsh scripts/set-exe-icon.ps1       # ALWAYS run after every build: writes assets/icons/icon.ico into the exe
cargo run --release                 # build and run the pet
cargo run --release -- --demo       # scripted fake input (test rendering without real input)
cargo run --release -- --mcp        # stdio MCP server only, no window
cargo build --release --features layout-108 --target-dir target/108   # 108-key variant, as CI builds it
pwsh scripts/set-exe-icon.ps1 -ExePath target/108/release/KuroCat.exe  # icon for that variant
cargo test                          # unit tests (inline `#[cfg(test)] mod tests` in each module; CI runs `cargo test --release`)
cargo test <test_name>              # a single test
makensis /DVERSION=x.y.z installer\kurocat.nsi   # installer (NSIS 3, after a release build); optional /DEXE_PATH=… /DOUTPUT=…
```

Notes:
- **Debug builds are slow** because rendering is on the CPU (dependencies are built at `opt-level = 3` even in dev, but our own code is not). Judge smoothness and CPU use with `--release`.
- Release builds use `windows_subsystem = "windows"`, so they have no console; debug builds keep one, which is where `println!`/`eprintln!` output shows up.
- **Exe icon**: this machine has no `rc.exe` / `windres`, so `build.rs` (which would embed `assets/app.rc` through `embed-resource`) only warns and the exe comes out without an icon. **After every build, run `scripts/set-exe-icon.ps1`.** It writes the icon into the exe with the Windows resource update API, so the icon travels with the exe. Run it before code signing and before `makensis`. DPI awareness is set in code (`SetProcessDpiAwarenessContext`), so the missing manifest does not matter.
- **Icon artwork**: `scripts/generate-icon.py` draws the red cat head and writes every file in `assets/icons/`.
- **Cargo feature `layout-108`**: starts with the 108-key keyboard. The updater also uses it to pick the matching release asset.

## Architecture (`src/`)

### Threads and events
- **UI thread** (`window.rs`):
  - Owns the layered window, the `Engine` and the frame loop.
  - The loop waits in `MsgWaitForMultipleObjectsEx` on a high-resolution waitable timer: 60 fps, or 30 fps when `engine.is_idle()`.
  - Window state is in a thread-local `RefCell`. The context menu is shown while no borrow is held, and a `WM_TIMER` keeps frames coming during the modal menu loop.
- **Worker threads** (`app::run` starts them):
  - input hooks, privacy watcher, hit-test loop;
  - storage watcher, which polls `settings.json` / `cats.json` mtimes every 400 ms;
  - MCP HTTP server, updater.
- **Talking to the UI thread**: workers call `events::send(AppEvent::…)`, which pushes onto a channel and posts `WM_APP_EVENT`. They never touch the window directly; window operations go through `AppEvent::Window(WindowCommand)`.
- **Menu events**: `muda` menu and `tray-icon` events are polled on each frame. Tray ids start with `tray:`, context-menu ids with `ctx:`.

### Modules
- **`app.rs`**: `AppState` (settings plus atomic flags: menu open, dragging, MCP enabled, update banner, paused), settings changes (`update_settings`, `change_settings`, `apply_*_and_save`), `reload_and_emit_all`, window size per layout and scale.
- **`hit_test.rs`**: a 25 ms loop that toggles `WS_EX_TRANSPARENT` (click-through) depending on whether the cursor is over the cat or desk. Alt, open menus, dragging and the update banner override it. It also holds the z-order "topmost guard" for full-screen games.
- **`window.rs`**: also handles the cursor (petting hand), manual drag (`SetCapture`), Ctrl+wheel scaling (committed 400 ms after the last wheel event), `WM_DPICHANGED` and the clipboard.
- **`audio.rs`**: the purr synth on its own `cpal` thread; the engine drives it through `set_purr_intensity` / `set_sound_enabled`.
- **`menus.rs`**: right-click context menu. **`tray.rs`**: tray icon and menu. **`banner.rs`**: update and toast banner drawn on the canvas, with clickable buttons.
- **`input_hook.rs`**: low-level keyboard and mouse hooks. Emits `Input` events with Windows VK codes.
- **`privacy.rs`**: detects password fields (UI Automation `IsPassword`, `ES_PASSWORD`). While one is focused, the hooks send `PrivacyTyping` instead of the real keys.
- **`mcp.rs`**: one JSON-RPC handler shared by stdio mode and the local Streamable HTTP server (`127.0.0.1:27420/mcp`). Tools: `list_cats`, `get_cat_schema`, `upsert_cat`, `switch_cat`, `delete_cat`, `get_settings`, `update_settings`.
- **`cats.rs` / `config.rs`**: the `CatSkinConfig` schema and built-in cats, and persistence under `%APPDATA%\kuro-cat-pet`. Serde uses `camelCase`. `skins.rs` resolves a custom cat into drawing colours.
- **`updater.rs`**: checks GitHub Releases and picks the asset for this install kind: `*-x64-setup.exe` when `uninstall.exe` sits next to the exe, otherwise the portable exe. It verifies the SHA-256, then:
  - installed copy: runs the setup with `/S`;
  - portable copy: replaces the exe.

  In both cases a hidden PowerShell script finishes the job and relaunches after this process exits.
- **`engine/`**: the animation state machine and drawing, ported from the old TypeScript engine.
  - `animation.rs`: state, input handlers, `update`, `render`, `is_idle`.
  - `body.rs`: the black and ragdoll cats.
  - `custom_skin.rs`: pattern-based cats.
  - `desk.rs`: keyboard, mouse, bubbles, badges.
  - `dafeiyu.rs`: the sprite-based whale girl, using PNGs from `assets/dafeiyu`.
  - The keyboard at rest is cached in a layer. Only moving keys are drawn on top of it each frame.
- **`render/`**: `Canvas` imitates the HTML Canvas 2D API (save/restore, transforms, paths, gradients, clip, global alpha, shadows, text), so the engine reads like the original.
  - Shadows are a coverage mask, blurred (`blur.rs`, downsampled for large blur radii) and composited with integer math.
  - Text uses `ab_glyph` with the Segoe UI fonts from `C:\Windows\Fonts`.
  - Colours are CSS strings (`color.rs`).
- **`keyboard_layout.rs`**: the 65% and 108-key layouts, keyed by VK code.

### How state flows
- **Stdio MCP** (`--mcp`) is a separate process with no IPC. It only writes `cats.json` / settings, and the running pet picks up the change through the storage watcher.
- **Changes from the menus, the tray or HTTP MCP** go through `app.rs`, which saves them and sends `AppEvent::Settings` / `AppEvent::Cats`.
- **Adding a setting** means updating:
  - `AppSettings` in `config.rs`;
  - `update_settings` in `app.rs`;
  - the MCP `update_settings` tool;
  - the menus and the window's settings handler.

## Releases

The version lives in `Cargo.toml`; CI fails if the pushed `v*` tag does not match it. It is passed to NSIS as `/DVERSION`. Pushing a `v*` tag runs `.github/workflows/release.yml`, which builds the 65% and 108-key exes, writes the icon, runs the tests and an MCP smoke test, builds both installers, and attaches `KuroCat-<ver>[-108key]-x64-setup.exe` and `KuroCat-<ver>[-108key]-windows-x64.exe` to a draft release. The updater relies on these names. The README also has versioned download links. `website/` is the separate project site (Chinese), maintained by hand.
