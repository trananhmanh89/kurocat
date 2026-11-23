# 🐈‍⬛ KuroCat

> 🌐 **Website and downloads (China mirror)**: [**https://qianyi.one/kurocat**](https://qianyi.one/kurocat)

KuroCat is a small, transparent, always-on-top desktop pet for **Windows 10 / 11**: a cat sitting behind a keyboard and mouse that types and clicks along with you.

It comes with **6 hand-drawn cats and 🐳 Dafeiyu (the whale girl)**: black cat, ragdoll, orange tabby, calico, tuxedo, Siamese and Dafeiyu. It **closes its eyes while you type a password**, and it includes an **MCP server**, so an AI assistant can design new cats for you in one sentence.

Version 5 is written in **pure Rust**:
- native global input hooks;
- a CPU renderer built on tiny-skia, drawn into a per-pixel-alpha layered window;
- native Windows menus.

There is **no WebView and no runtime to install**: one small exe.

## 📥 Download

Get the latest version from the [Releases](https://github.com/qianyiai/kurocat/releases) page:

- **`KuroCat-<version>-x64-setup.exe`**: the installer. It installs for the current user, without admin rights, into `%LOCALAPPDATA%\KuroCat`, and adds a Start menu entry and an uninstaller. It upgrades an existing v4.x install in place.
- **`KuroCat-<version>-windows-x64.exe`**: the portable version. Run it from anywhere.
- The **`108key`** files are the same app, starting with the full-size 108-key keyboard.

Your settings and cats are kept in `%APPDATA%\kuro-cat-pet` and survive upgrades and uninstalls.

**Games, administrator rights and streaming**
- **Full-screen games**: a z-order guard keeps the pet above borderless and full-screen-optimised games without stealing focus. Turn on "Lock click-through" and hold `Alt` whenever you want to interact with the cat.
- **Games running as administrator**: if the game runs as administrator, KuroCat cannot see its input. Use **"Restart as administrator (capture input in games)"** from the tray.
- **Streaming**: add the pet in OBS as a "Window Capture" source.
- **Strict kernel anti-cheat games** (Valorant, Delta Force, ...): use **"Pause listening and hide (anti-cheat safe)"** from the tray.

## ✨ Features

- **7 built-in skins plus unlimited custom cats over MCP**
  - Black cat, ragdoll, orange tabby (forehead `M` and ringed tail), calico, tuxedo (white gloves), Siamese (colour points).
  - Dafeiyu, the whale girl: three limbs follow the keys and the mouse, her tail slaps the keys on the right with a splash, and she has four expressions and blushes when you type fast.
  - Switch skins instantly from the right-click menu or the tray.
- **Password guard**
  - KuroCat detects password fields through UI Automation (`IsPassword`) and the Win32 `ES_PASSWORD` style.
  - While you type in one, the real keys never reach the pet. The cat closes its eyes, and Dafeiyu covers hers, while a privacy cover hides the keyboard.
- **Built-in MCP server**: send a photo of your own cat to Claude Desktop, Cursor, Windsurf, Cline or any other MCP client, and the AI creates a matching pet through the built-in tools. The new cat appears on your desktop within half a second, with no restart.
- **65% or 108-key keyboard, facing either way**
  - Choose a compact 65% keyboard or a full-size 108-key one, with the F row, navigation cluster and numpad.
  - The keyboard faces either **the cat** (a realistic 180° view) or **you** (readable legends).
  - The left, right and middle mouse buttons each light up separately, with small `LMB`/`RMB` badges.
- **Petting and purring**
  - Over the cat's head, the cursor turns into a small hand. Stroke the head and the cat squints happily.
  - It purrs too: a soft sound synthesised in real time. This can be turned off.
- **Key bubbles**: little 3D key caps pop up above the keyboard as you type. They can be hidden.
- **Smart click-through**: clicks pass through the transparent areas, so the pet never blocks your work or your game.
- **Size from 20% to 150%**: pick a preset from the right-click menu, or hold `Ctrl` and scroll over the cat.
- **Automatic updates**: KuroCat checks GitHub Releases in the background and shows a small banner above the cat when a new version is out. "Check for updates" downloads it, verifies its SHA-256 and installs it silently. This works for both the installed and the portable version.
- **Smooth and light**: up to 60 fps while you type, and 30 fps when idle.

## 🤖 MCP setup (change or add cats with AI)

`KuroCat.exe` contains an MCP server with two transports, **stdio** and **local Streamable HTTP** (`127.0.0.1:27420`), and needs nothing else installed.

### Quick start
1. Run KuroCat, right-click the cat, make sure **"Enable MCP extension server"** is ticked, then click **"Copy MCP config"**.
2. Paste the config into your AI client's MCP settings (Claude Desktop, Cursor, Windsurf, Cline, ...).

**Option A: stdio (works with every MCP client)**
```json
{
  "mcpServers": {
    "kurocat": {
      "command": "C:\\Users\\<your user name>\\AppData\\Local\\KuroCat\\KuroCat.exe",
      "args": ["--mcp"]
    }
  }
}
```

**Option B: local HTTP (while the pet is running)**
```json
{
  "mcpServers": {
    "kurocat": {
      "url": "http://127.0.0.1:27420/mcp"
    }
  }
}
```

### Things to ask your AI
- *"Use KuroCat MCP to add an orange tabby with white gloves and an orange keyboard highlight, then switch to it."*
- *"Here is a photo of my calico. Use KuroCat MCP to make a desktop pet with her colours and spots!"*
- *"Make the pet 125%, turn off the key bubbles and switch back to the black cat."*

### Built-in MCP tools
| Tool | What it does |
| --- | --- |
| `list_cats` | Lists the built-in and custom cats and the one that is active |
| `get_cat_schema` | Returns the full skin schema: the 6 patterns (`solid`/`bicolor`/`tuxedo`/`tabby`/`calico`/`siamese`), custom spots and breed templates |
| `upsert_cat` | Adds or changes a custom cat (fur, pattern, ears, eye colour, tail, white gloves, accent colour, custom patches); it appears on screen right away |
| `switch_cat` | Switches the cat shown on the desktop by id |
| `delete_cat` | Deletes a custom cat (or resets a built-in one) |
| `get_settings` / `update_settings` | Reads or changes the size, key bubbles, purring, always-on-top, click-through lock and autostart |

Custom cats are stored in `%APPDATA%\kuro-cat-pet\cats.json`. You can also edit this file by hand; the running pet reloads it automatically.

## 🛠️ Build from source

You need Windows 10/11 and the Rust toolchain (`cargo`).

```bash
git clone https://github.com/qianyiai/kurocat.git
cd kurocat
cargo run --release              # build and run
cargo run --release -- --demo    # run with a scripted demo of fake input
cargo test                       # unit tests
```

**Exe icon**: after building, run

```bash
pwsh scripts/set-exe-icon.ps1
```

This writes `assets/icons/icon.ico` into `target/release/KuroCat.exe`. It needs nothing installed and the icon stays in the exe on any machine. If a resource compiler is on `PATH` (`rc.exe` from the Windows SDK, or `windres` for GNU), the build embeds the icon by itself.

**Installer**: build it with [NSIS](https://nsis.sourceforge.io/) 3:

```bash
makensis /DVERSION=5.0.0 installer\kurocat.nsi
```

## 📄 License and credits

KuroCat is released under the [MIT License](LICENSE).

- The built-in **🐳 Dafeiyu (whale girl)** skin uses artwork from the MIT-licensed project [PC2005-cloud/dsh-pet](https://github.com/PC2005-cloud/dsh-pet) (see also [QCYTSN/dsh-dafeiyu](https://github.com/QCYTSN/dsh-dafeiyu)). The full licence text is in `assets/dafeiyu/LICENSE-MIT.txt`.
