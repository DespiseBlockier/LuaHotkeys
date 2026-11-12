# LuaHotkeys

A Rust automation app for global hotkeys and macros in Lua 5.1.5. The Windows app includes a syntax-colored editor, system tray control, and activity log. The [Linux runner](packaging/Linux.md) runs scripts from the terminal on X11 and installs through Pacman on Arch Linux or Zypper on openSUSE Tumbleweed.

## Quick start

1. Choose the installer or the portable ZIP from [Releases](https://github.com/DespiseBlockier/LuaHotkeys/releases). In the installer, use **Change** to pick a folder on any writable drive. For the portable build, extract the ZIP where you want to keep the app.
2. Open LuaHotkeys from the Start menu or the extracted folder and click **Run** to start the included example.
3. Switch to a text editor and press **Ctrl+Alt+J**. The script types a line of text. **Ctrl+Alt+N** opens Notepad, and **Ctrl+Alt+Shift+M** logs the pointer position.
4. Click **Stop** or choose **Stop script** from the tray menu to release the hotkeys.

## Update LuaHotkeys

Click **Update** in the editor to check for updates. You can also open **Update LuaHotkeys** from the Start menu for an installed copy. For the portable build, run **LuaHotkeysUpdater.exe** beside **LuaHotkeys.exe** in the extracted folder.

The updater checks the latest stable GitHub release, verifies the package hash, and applies it to that copy. The installed build reuses your original install folder. The portable build replaces its program files and keeps your scripts in place. If LuaHotkeys has unsaved edits, save them and close the app when asked.

For a copy from before 0.3.1, run the newer MSI or extract the newer portable ZIP into the same folder once to get the updater.

## Run or edit a script

New scripts use the `.lhk` extension. Existing `.luahotkeys` scripts work the same way. The MSI registers both extensions during setup. For the portable build, choose **Associate files** from the LuaHotkeys tray menu once.

Use **New** in LuaHotkeys or **New → LuaHotkeys Script** in Explorer to make an empty `.lhk` file.

- **Double-click** to run the script in the tray. A script that finishes without declaring hotkeys exits on its own.
- **Right-click → Edit in LuaHotkeys** to open the editor. On Windows 11, look under **Show more options** if needed.
- While a script runs in the tray, use **Edit script** to open its editor or **Exit** to stop it and close LuaHotkeys.

The editor also opens ordinary `.lua` files. Use **Restart** to apply changes to a running script. If you move the portable build, use **Associate files** from its tray menu again.

```powershell
.\LuaHotkeys.exe --run ".\my shortcuts.lhk"
.\LuaHotkeys.exe --edit ".\my shortcuts.lhk"
```

## Editor tools

Click **Docs** to browse the offline LuaHotkeys reference by category. Each function has a syntax-colored signature, a description, and a copyable example. The editor and the reference use the same colors for Lua keywords, built-in libraries and their members, and LuaHotkeys functions.

The editor suggests Lua keywords and functions as you type. Use **Ctrl+Space** to open suggestions on demand, the arrow keys to select, and **Tab** or **Enter** to insert. **Escape** closes the list.

With the suggestion list closed, **Tab** indents to four-space stops and **Shift+Tab** dedents. **Enter** follows the current Lua scope.

Function completions add parentheses and place the caret inside. For example, completing `hotkey` inserts `hotkey()`. Completion also recognizes Lua library members such as `math.sqrt` and suggests block templates in code.

Inside a shortcut string in `hotkey`, `press`, or a local `Combo` assignment, completion suggests valid modifiers and keys for the current `+`-separated part. Open **Docs → Types → Combo** for all supported keys, aliases, and registration limits.

## Script types

LuaHotkeys scripts accept optional Luau-style annotations on local variables, function parameters, and return values. Define aliases with `type Name = ...`.

```lua
type SaveShortcut = Combo
local save: SaveShortcut = "Ctrl+Shift+S"

hotkey(save, function()
    log("Save shortcut pressed")
end)
```

Annotations and aliases are erased before the entry script runs in Lua 5.1. API functions validate arguments at runtime. Modules loaded by Lua's `require` use standard Lua 5.1 syntax.

## Macro windows

Use `gui:window("Macro controls")` for an ordinary window or `defineGui("Imgui")` to use Dear ImGui. Both engines provide buttons, hover events, sliders, toggles, labels, text fields, and keybind pickers. Connect Lua callbacks to each control's signals.

Open **Docs → Macro windows** in the editor or see the [GUI reference and examples](packaging/Gui.md).

## Lua functions

```lua
local greeting = "Hello from Lua!"

hotkey("Ctrl+Alt+J", function()
    type_text(greeting)
    press("Enter")
end)
```

| Function | Behavior |
| --- | --- |
| `hotkey(combo, callback)` | Declares a system-wide shortcut at script startup. |
| `type_text(text)` | Types UTF-8 text into the focused window. Line breaks send Enter. |
| `press(combo)` | Sends a key or a combination such as `"Ctrl+S"` or `"Enter"`. |
| `click([button])` | Clicks at the pointer. Use `"left"`, `"right"`, or `"middle"`. The default is left. |
| `move_mouse(x, y)` | Moves the pointer to desktop coordinates in pixels. |
| `mouse_pos()` | Returns the pointer's x and y coordinates. |
| `sleep(ms)` | Pauses the script for an integer number of milliseconds. |
| `run(program, ...)` | Opens an app, file, or URL. Pass command-line arguments as separate strings. |
| `log(...)` or `print(...)` | Writes values to the Activity panel. |

Key names are case-insensitive. Modifiers include `Ctrl`, `Alt`, `Shift`, and `Win`. Main keys include punctuation such as `Backslash`, `Hash`, and `Plus`, numpad operators such as `NumPadEnter`, and standalone keys such as `Win`, `Pause`, and `ScrollLock`. Use `press("Win")` to send the left Windows key. Choose a different combination if the Activity panel reports that Windows or another app already registered it. Input goes to the focused window and uses the current keyboard layout.

## Build from source

Install Rust, the MSVC C++ build tools, and the Windows SDK. Add the 32-bit Windows target with `rustup target add i686-pc-windows-msvc`.

The build links the Lua 5.1 import library from Lua for Windows. Set `LUA51_DIR` if the directory containing `lua5.1.dll`, `lib\lua5.1.lib`, and `LICENSE.txt` is outside the default location. Run `.\build.ps1` to build the app and updater in `dist`.

Build the release packages with [WiX Toolset 3.14.1](https://github.com/wixtoolset/wix3/releases/tag/wix3141rtm). Put its tools on PATH or set `WIX_BIN` to the folder containing `candle.exe` and `light.exe`, then run `.\build-release.ps1`. It uses the version in `VERSION` and writes both the MSI and the portable ZIP to `dist`. Include both files in each release so both update paths are available.
