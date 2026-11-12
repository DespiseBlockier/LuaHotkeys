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

The Imgui window uses a compact pink header, plum background, and Merriweather type. Click the chevron on the left to collapse the window to its header. Click it again to restore the content and any selected tab. Drag the title to move the window. The Basic engine uses the system's window decorations.

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
| `width`, `height` | Expanded size in logical pixels. Width accepts 240–4096 and height accepts 130–4096. Defaults to 300 by 222. |
| `visible` | Set to `false` to open the window later with `show()`. |
| `collapsed` | Set to `true` to start with only the 22-pixel header visible. |
| `animation` | Collapse and expand duration in milliseconds, from 0 to 2000. The default is 100. |
| `x`, `y` | Initial position in logical desktop pixels. Provide both coordinates. |
| `theme.header` | Color of the title bar. |
| `theme.background` | Color behind the controls. |
| `theme.tabs` | Color of the strip behind tab names. |
| `theme.control` | Fill color of buttons, sliders, and text fields. |
| `theme.dropdown` | Fill color of dropdowns. |
| `theme.accent` | Color of checkmarks, slider handles, keyboard focus, and active states. |
| `theme.text` | Color of the title, labels, and control text. |
| `theme.rounding` | Corner radius from 0 to 32 logical pixels. |

Colors use `#RRGGBB`. Omit any option to use the default. Completion suggests `"Basic"` and `"Imgui"` inside `defineGui()`.

## Window and control reference

Declare one window at the top level of the script. Basic controls are declared at startup. Imgui controls can also be added or removed from callbacks. Controls appear in the order you create them.

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
| `window:collapse()`, `window:expand()` | Collapse to the header or restore the contents. Imgui only. |
| `window:is_collapsed()` | Returns whether the Imgui panel is collapsed. |
| `window:set_position(x, y)` | Moves the Imgui window to logical desktop coordinates. |

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
| `dropdown.changed` | A different item is selected, or the selection is cleared | Item name or `nil` |
| `color.changed` | The selected color changes | Uppercase `#RRGGBB` string |
| `folder.changed` | The folder opens or closes | New boolean |
| `window.closed` | The Basic title bar closes the window, or the script calls `window:close()` | None |

Calling `set` also fires `changed` when the value differs. Starting values and repeated assignments of the same value produce no event. The callback argument is the value that triggered the signal. `get()` reads the control's current value.

`signal:connect(callback)` returns a connection. Call `connection:disconnect()` to stop receiving the signal.

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

## Imgui tabs, folders, and pickers

A tab, folder, or dock accepts the same control constructors as the window. Add tabs to the window with `window:tab(name)`. The first tab starts selected, and `tab:select()` selects another one from Lua. Controls added directly to the window stay visible above the selected tab's controls.

| Call | What it adds |
| --- | --- |
| `panel:folder(label, open?)` | A collapsible group. Use `get()`, `set(boolean)`, or `switch()` to read or change its state. |
| `panel:dock()` | A row that puts its controls side by side. |
| `panel:dropdown(label, items, initial?)` | A dropdown with case-insensitive search. Pass an array of unique names and an optional starting selection. |
| `panel:color(label, initial?)` | A color swatch that opens a movable desktop palette. Its value is a `#RRGGBB` string. |

Use `dropdown:add_option(name)` and `dropdown:remove_option(name)` to update the list. Removing the selected item clears the selection and emits `changed(nil)`. Call `dropdown:set(nil)` to clear it while keeping the option. `control:set_color("#FF85A2")` sets a control's display color. `set_color(nil)` restores its theme color. For a color picker, `get()` and `set(color)` read and set the selected color. `control:destroy()` removes a control, including all children of a group.

```lua
local window = defineGui("Imgui"):window("Settings")
local tab = window:tab("General")
local folder = tab:folder("Actions", true)
local row = folder:dock()
local apply = row:button("Apply")
local reset = row:button("Reset")
local mode = tab:dropdown("Mode", {"Type", "Press", "Preview"}, "Type")
local color = tab:color("Button color", "#FF85A2")

color.changed:connect(function(value)
    apply:set_color(value)
    reset:set_color(value)
end)
apply.pressed:connect(function() log(mode:get()) end)
reset.pressed:connect(function() mode:set("Type") end)
```

Run [imgui-demo.lhk](imgui-demo.lhk) to try the MainPink-style window and its controls.

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
    if combo and combo ~= trigger then
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
