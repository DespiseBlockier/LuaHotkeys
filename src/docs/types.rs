pub struct TypePage {
    pub name: &'static str,
    pub definition: &'static str,
    pub description: &'static str,
    pub example: &'static str,
}

pub const TYPE_EXAMPLE: &str = r#"type SaveShortcut = Combo
type Point = { x: number, y: number }

local save: SaveShortcut = "Ctrl+Shift+S"

local function describe(point: Point): string
    return string.format("%d, %d", point.x, point.y)
end

log(save, describe({ x = 40, y = 70 }))"#;

pub const TYPES: &[TypePage] = &[
    TypePage {
        name: "Combo",
        definition: "type Combo = string",
        description: "A string that names one key, optionally preceded by modifiers. hotkey listens for the combination globally. press sends it to the focused window.",
        example: r#"local shortcut: Combo = "Ctrl+Alt+J"

hotkey(shortcut, function()
    type_text("Shortcut pressed")
end)"#,
    },
    TypePage {
        name: "HotkeyCallback",
        definition: "type HotkeyCallback = () -> ()",
        description: "A Lua function called when its hotkey fires. LuaHotkeys passes zero arguments and ignores return values. The callback may capture variables from its surrounding script.",
        example: r#"local message = "Shortcut pressed"
local callback: HotkeyCallback = function()
    log(message)
end

hotkey("Ctrl+Alt+J", callback)"#,
    },
    TypePage {
        name: "MouseButton",
        definition: r#"type MouseButton = "left" | "right" | "middle""#,
        description: "A string naming a mouse button. click accepts left, right, or middle, ignoring case. Pass nil or omit the argument for a left click.",
        example: r#"local button: MouseButton = "right"
click(button)"#,
    },
    TypePage {
        name: "Pixel",
        definition: "type Pixel = number",
        description: "A signed 32-bit whole number measuring a desktop coordinate in pixels. x increases to the right and y increases downward. Monitors to the left or above the primary monitor can have negative coordinates. mouse_pos returns x, then y.",
        example: "local x: Pixel, y: Pixel = mouse_pos()\nlog(x, y)",
    },
    TypePage {
        name: "Milliseconds",
        definition: "type Milliseconds = number",
        description: "A whole number from 0 through 2,147,483,647. One thousand milliseconds is one second. sleep pauses the current script for the given duration.",
        example: "local delay: Milliseconds = 250\nsleep(delay)",
    },
    TypePage {
        name: "string",
        definition: r#"local message: string = "Hello""#,
        description: "Text in quotes. type_text sends literal text to the focused window, including Enter for line breaks. run takes a string for the app, file, or URL, followed by separate string arguments if needed.",
        example: "local message: string = \"First line\\nSecond line\"\ntype_text(message)",
    },
    TypePage {
        name: "number",
        definition: "local distance: number = 3.5",
        description: "Lua 5.1 uses one number type for whole numbers and decimals. APIs such as sleep and move_mouse require whole numbers within the ranges described by their types.",
        example: "local seconds: number = 0.5\nlocal delay: Milliseconds = math.floor(seconds * 1000)\nsleep(delay)",
    },
    TypePage {
        name: "any",
        definition: "local value: any = true",
        description: "A value of any Lua type, such as a string, number, boolean, table, function, or nil. log and print convert each argument to text and display them in Activity.",
        example: r#"local value: any = 42
log("Value", value, true, nil)"#,
    },
];
