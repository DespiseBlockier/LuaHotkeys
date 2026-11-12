# Macro windows

Use a macro window to start an action, adjust a setting, or show the state of a running script. The **Basic** engine creates an ordinary window with native controls on Windows and Tk controls on Linux. **Imgui** draws the same controls with Dear ImGui. Both engines use the same Lua API.

## Create a window

The global `gui` is the Basic engine.

```lua
local window = gui:window("Macro controls")
local status = window:label("Ready")
local start = window:button("Run")

start.pressed:connect(function()
    status:set("Macro started")
    log("Run requested")
end)
```

Select the engine explicitly with `defineGui("Basic")` or `defineGui("Imgui")`. The names are case-insensitive.

```lua
local engine = defineGui("Imgui")
local window = engine:window("Macro controls")
local enabled = window:toggle("Enabled", true)

enabled.changed:connect(function(value)
    log("Enabled", value)
end)
```

On Windows, open an `.lhk` script in the editor and click **Run**, or open the script directly. On Linux, run `luahotkeys --run ./my-script.lhk` from an X11 session. The Basic engine on Linux needs `wish` from the `tk` package. The Imgui engine needs an OpenGL 3.3 or OpenGL ES 3.0 driver.

## Size and theme an Imgui window

The Imgui window uses a draggable title bar with one minimize button. Set its fixed size and colors when you create it. The Basic engine uses the system's window decorations.

```lua
local engine = defineGui("Imgui")
local window = engine:window("Macro panel", {
    width = 480,
    height = 320,
    theme = {
        header = "#243956",
        background = "#131C2B",
        control = "#29384E",
        accent = "#74B4FF",
        text = "#EEF4FF",
        rounding = 14,
    },
})
window:label("Ready")
```

| Option | What it changes |
| --- | --- |
| `width`, `height` | Fixed size in logical pixels. Width accepts 240–4096 and height accepts 130–4096. |
| `visible` | Set to `false` to open the window later with `show()`. |
| `theme.header` | Color of the title bar. |
| `theme.background` | Color behind the controls. |
| `theme.control` | Fill color of buttons, sliders, and text fields. |
| `theme.accent` | Color of checkmarks, slider handles, keyboard focus, and active states. |
| `theme.text` | Color of the title, labels, and control text. |
| `theme.rounding` | Corner radius from 0 to 32 logical pixels. |

Colors use `#RRGGBB`. Omit any option to use the default. Completion suggests `"Basic"` and `"Imgui"` inside `defineGui()`.

## Window and control reference

Declare one window and its controls at the top level of the script. Controls appear from top to bottom in the order you create them.

| Call | Arguments and result |
| --- | --- |
| `engine:window(title, options?)` | Creates the script's window. Pass `false` to start hidden, or an Imgui options table to set size, visibility, and theme. |
| `window:label(text)` | Adds a line of text. `label:get()` returns the text and `label:set(text)` replaces it. |
| `window:button(text)` | Adds a button with `pressed`, `hovered`, and `unhovered` signals. |
| `window:slider(label, min, max, initial?)` | Adds a whole-number slider. The starting value defaults to `min`. All three numbers must be signed 32-bit integers. `max` must exceed `min` by at most 2,147,483,647. |
| `window:toggle(label, initial?)` | Adds a checkbox. The starting value defaults to `false`. |
| `window:input(label, initial?)` | Adds a single-line text field. The starting text defaults to an empty string. |
| `window:keybind(label, initial?)` | Adds a keybind picker. The starting value is a `Combo` string or `nil` for an unassigned picker. |
| `window:show()` | Shows a hidden window. |
| `window:hide()` | Hides the window and keeps its controls and values available. A hotkey can show it again. |
| `window:close()` | Closes the window permanently and emits `closed` once. |

Sliders, toggles, text fields, and keybind pickers have `get()` and `set(value)` methods. A slider accepts a number within its range, a toggle accepts a boolean, and a text field accepts a string. Labels and text fields take single-line text. A picker accepts a `Combo` string or `nil` to clear it.

## Signals and callbacks

Connect a Lua function with `signal:connect(function)`. You can connect more than one function to a signal. LuaHotkeys calls them in the order they were connected. An error in a callback goes to the script log.

| Signal | When it fires | Callback argument |
| --- | --- | --- |
| `button.pressed` | The user activates the button with the mouse or keyboard | None |
| `button.hovered` | The pointer enters the button | None |
| `button.unhovered` | The pointer leaves the button, or the window hides or closes while the button is hovered | None |
| `slider.changed` | The value changes, including each distinct value while dragging | New whole number |
| `toggle.changed` | The checked state changes | New boolean |
| `input.changed` | The text changes as the user edits it | New string |
| `keybind.changed` | The picker records a different combo, or the script changes its value | New `Combo` string or `nil` |
| `window.closed` | The window closes through its title bar or `window:close()` | None |

Calling `set` also fires `changed` when the value differs. Starting values and repeated assignments of the same value produce no event. The callback argument is the value that triggered the signal. `get()` reads the control's current value.

```lua
local window = gui:window("Delay")
local delay = window:slider("Delay (ms)", 0, 1000, 250)
local label = window:label("Delay is 250 ms")
local reset = window:button("Reset")

delay.changed:connect(function(value)
    label:set("Delay is " .. value .. " ms")
end)

reset.pressed:connect(function()
    delay:set(250)
end)
```

## Record the keys a macro sends

Activate a picker and press the new combination. Hold Win with another key to record a chord, or tap Win to record it by itself. Press Escape by itself, click the picker again, or move focus away to cancel and keep the previous value. LuaHotkeys' own registered hotkeys can be recorded while the picker is waiting. System shortcuts may be intercepted by the desktop before the picker receives them.

Use `get()` to read the chosen combo in your macro. In this example, set the outgoing keys in the window, switch to the target app, then press Ctrl+Alt+J to send them.

```lua
local window = gui:window("Choose what to send")
local outgoing = window:keybind("Keys to send", "Ctrl+S")
local trigger = "Ctrl+Alt+J"

outgoing.changed:connect(function(combo)
    log("Keys to send", combo)
end)

hotkey(trigger, function()
    local combo = outgoing:get()
    if combo and combo `= trigger then
        press(combo)
    end
end)
```

The picker changes a Lua value for the current run. Save it from `changed` in your own settings if you want to restore it on the next run. Registered hotkeys are declared when the script starts. Use the picker value as an argument to `press()` when you want to change the keys the macro sends.

## Use a window with hotkeys

GUI callbacks and hotkeys share the same Lua variables. Create the window with `false` to let the script run in the background until a hotkey opens it.

```lua
local window = gui:window("Macro settings", false)
local message = window:input("Message", "Hello")
local enabled = window:toggle("Enabled", true)
local hide = window:button("Hide settings")
local closed = false

hide.pressed:connect(function() window:hide() end)
window.closed:connect(function() closed = true end)

hotkey("Ctrl+Alt+M", function()
    if not closed then window:show() end
end)

hotkey("Ctrl+Alt+J", function()
    if enabled:get() then type_text(message:get()) end
end)
```

Keyboard and mouse macros act on the current desktop focus and pointer. Switch to the target app before using a shortcut that types into it. Keep GUI callbacks short so the script can respond to the next signal or shortcut.

A visible or hidden window keeps the script alive. Closing its window ends a script that has no global hotkeys. If the script has hotkeys, those stay registered until you stop the script. Existing control values remain readable in its callbacks.

The [macro-gui.lhk example](macro-gui.lhk) combines a button, hover events, a slider, a toggle, a text field, and a picker that changes the key sent after typing a message.
