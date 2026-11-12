use std::fmt::Write;

use super::types::{TYPES, TYPE_EXAMPLE};
use crate::{completion, highlight, shortcut, theme};

pub struct Category {
    pub name: &'static str,
    description: &'static str,
    pub functions: &'static [Function],
}

pub struct Function {
    pub name: &'static str,
    description: &'static str,
    parameters: &'static [(&'static str, &'static str)],
    returns: Option<&'static str>,
    example: &'static str,
}

const STARTER: &str = r#"hotkey("Ctrl+Alt+J", function()
    type_text("Hello from LuaHotkeys!")
    press("Enter")
end)"#;

const MACRO_GUI: &str = include_str!("../../packaging/macro-gui.lhk");
const IMGUI_DEMO: &str = include_str!("../../packaging/imgui-demo.lhk");

pub const CATEGORIES: &[Category] = &[
    Category {
        name: "Hotkeys",
        description: "Declare global shortcuts when a script starts. Each shortcut runs a Lua function when you press it.",
        functions: &[Function {
            name: "hotkey",
            description: "Registers a global key combination at script startup. The callback normally runs when you release the shortcut. Pass \"press\" as the third argument to run it as soon as you complete the combination. A held shortcut fires once.",
            parameters: &[
                ("combo: Combo", "The keyboard key to listen for. See Types → Combo for the key names."),
                ("callback: HotkeyCallback", "The Lua function to run."),
                ("trigger: string?", "Use \"release\" (the default) or \"press\". Modifiers can still be held during a press callback."),
            ],
            returns: None,
            example: STARTER,
        }],
    },
    Category {
        name: "Keyboard and clicks",
        description: "Send text and keystrokes to the focused window or click at the pointer.",
        functions: &[
            Function {
                name: "type_text",
                description: "Types text into the focused window. Newlines send Enter.",
                parameters: &[("text: string", "The text to type.")],
                returns: None,
                example: r#"type_text("First line\nSecond line")"#,
            },
            Function {
                name: "press",
                description: "Sends one keyboard key to the focused window or clicks a mouse button at the pointer. Either can be combined with modifiers.",
                parameters: &[("combo: Combo", "Use a key name or MouseLeft, MouseRight, or MouseMiddle. Separate modifiers with +.")],
                returns: None,
                example: r#"press("Ctrl+S")
press("MouseRight")
press("Ctrl+MouseLeft")"#,
            },
        ],
    },
    Category {
        name: "Mouse",
        description: "Read the pointer position, move it, or click a mouse button.",
        functions: &[
            Function {
                name: "click",
                description: "Clicks at the pointer. Existing scripts can keep using click. Use press with a mouse Combo in new scripts.",
                parameters: &[("button: MouseButton?", "Use left, right, or middle. Omit the argument for a left click.")],
                returns: None,
                example: r#"click("right")"#,
            },
            Function {
                name: "move_mouse",
                description: "Moves the pointer to desktop coordinates. Coordinates are measured in pixels.",
                parameters: &[("x: Pixel", "Horizontal position."), ("y: Pixel", "Vertical position.")],
                returns: None,
                example: "move_mouse(400, 300)",
            },
            Function {
                name: "mouse_pos",
                description: "Reads the pointer’s current desktop coordinates.",
                parameters: &[],
                returns: Some("Two Pixel values in order, x and y."),
                example: "local x, y = mouse_pos()\nlog(x, y)",
            },
        ],
    },
    Category {
        name: "Timing",
        description: "Pause between steps in a macro.",
        functions: &[Function {
            name: "sleep",
            description: "Pauses the running script for the requested duration.",
            parameters: &[("milliseconds: Milliseconds", "The number of milliseconds to wait.")],
            returns: None,
            example: "local delay = math.floor(0.25 * 1000)\nsleep(delay)\npress(\"Enter\")",
        }],
    },
    Category {
        name: "Apps and logging",
        description: "Open an app, file, or URL and inspect script output in Activity.",
        functions: &[
            Function {
                name: "run",
                description: "Opens an app, file, or URL through Windows. Relative paths use the script’s folder.",
                parameters: &[
                    ("program: string", "The app, file, or URL to open."),
                    ("...: string", "Optional command-line arguments as separate strings."),
                ],
                returns: None,
                example: r#"run("notepad.exe")"#,
            },
            Function {
                name: "log",
                description: "Writes values to the Activity pane. Values are converted to text and separated by tabs.",
                parameters: &[("...: any", "The values to write.")],
                returns: None,
                example: r#"log("Macro started", os.date("%H:%M:%S"))"#,
            },
            Function {
                name: "print",
                description: "Writes values to the Activity pane, just like log.",
                parameters: &[("...: any", "The values to write.")],
                returns: None,
                example: r#"print(string.format("Result: %d", 42))"#,
            },
        ],
    },
    Category {
        name: "Macro windows",
        description: "Build a window with native controls, or choose the MainPink-style Dear ImGui engine. Connect Lua functions to control signals. Global hotkeys keep working while a macro window is open or collapsed.",
        functions: &[
            Function {
                name: "defineGui",
                description: "Chooses the Basic or Imgui engine. Basic is also available as the global gui. Imgui defaults to a pink header, plum body, Merriweather type, and a chevron that collapses to the draggable header. Window options include width, height, visible, collapsed, animation in milliseconds, x and y position, and theme. Theme colors use #RRGGBB. Set header, background, tabs, control, accent, and text independently, and set rounding from 0 to 32.",
                parameters: &[("engine: \"Basic\" | \"Imgui\"", "The window engine. Names are case-insensitive.")],
                returns: Some("An engine with the window method."),
                example: r##"local engine = defineGui("Imgui")
local window = engine:window("Macro controls", {
    width = 460,
    height = 320,
    theme = {
        header = "#243956",
        background = "#131C2B",
        accent = "#74B4FF",
        rounding = 14,
    },
})
window:label("Dear ImGui is running")"##,
            },
            Function {
                name: "gui:window",
                description: "Declares one macro window at startup. Basic controls are declared at startup. Imgui controls can also be added from callbacks. Use show(), hide(), or close() to change window visibility. Closing the window leaves registered hotkeys running.",
                parameters: &[
                    ("title: string", "Title shown in the system window."),
                    ("options: (boolean | table)?", "Pass false to open the window later with show(). The Imgui engine also accepts an options table. See defineGui for a size and theme example."),
                ],
                returns: Some("The macro window."),
                example: MACRO_GUI,
            },
            Function {
                name: "window:tab",
                description: "Adds an Imgui tab. The first tab starts selected. Call select() on a tab to switch to it. Add controls directly to a tab, or group them using folders and horizontal docks.",
                parameters: &[("label: string", "The text in the tab strip.")],
                returns: Some("A tab that accepts the window's control constructors."),
                example: IMGUI_DEMO,
            },
            Function {
                name: "tab:folder",
                description: "Adds a collapsible group in the Imgui engine. Folders can also be added directly to the window or nested inside other groups. Use get(), set(boolean), or switch() to change its state. changed receives the new open state.",
                parameters: &[("label: string", "Text in the folder bar."), ("open: boolean?", "Whether the folder starts open. Defaults to false.")],
                returns: Some("A folder that accepts controls."),
                example: "local advanced = tab:folder(\"Advanced\", true)\nadvanced:label(\"Extra options\")",
            },
            Function {
                name: "folder:dock",
                description: "Adds an Imgui row that places its controls side by side. Call dock() on the window, a tab, or a folder.",
                parameters: &[],
                returns: Some("A row that accepts controls."),
                example: "local row = folder:dock()\nlocal start = row:button(\"Start\")\nlocal stop = row:button(\"Stop\")",
            },
            Function {
                name: "tab:dropdown",
                description: "Adds a searchable Imgui dropdown. get() and changed return the selected name or nil. Use set(name) to select, set(nil) to clear, and add_option(name) or remove_option(name) to update the list.",
                parameters: &[("label: string", "Label next to the dropdown."), ("options: {string}", "Unique names in display order."), ("initial: string?", "The starting selection, if any.")],
                returns: Some("A dropdown with get(), set(), and changed."),
                example: "local mode = tab:dropdown(\"Mode\", {\"Type\", \"Press\"}, \"Type\")\nmode.changed:connect(function(name) log(name) end)",
            },
            Function {
                name: "tab:color",
                description: "Adds an Imgui color swatch that opens its own movable desktop palette. Values are #RRGGBB strings. Use set_color(value) on another Imgui control to change its display color, or set_color(nil) to restore the theme color.",
                parameters: &[("label: string", "Label next to the picker."), ("initial: string?", "Starting color. Defaults to #FF85A2.")],
                returns: Some("A picker with get(), set(), and changed."),
                example: r##"local color = tab:color("Button color", "#FF85A2")
color.changed:connect(function(value)
    button:set_color(value)
end)"##,
            },
            Function {
                name: "window:collapse",
                description: "Collapses an Imgui window to its draggable header. Call expand() to restore its configured size or is_collapsed() to read its state. The chevron performs the same actions. hide() still hides the entire window.",
                parameters: &[],
                returns: None,
                example: "local window = defineGui(\"Imgui\"):window(\"Settings\", { animation = 120 })\nwindow:collapse()",
            },
            Function {
                name: "window:button",
                description: "Adds a button. pressed fires on activation. hovered and unhovered fire when the pointer enters and leaves the button. Each signal connects with :connect(function).",
                parameters: &[("text: string", "The caption on the button.")],
                returns: Some("A button with pressed, hovered, and unhovered signals."),
                example: r#"local window = gui:window("Macro controls")
local button = window:button("Run macro")
button.pressed:connect(function()
    log("Run requested")
end)
button.hovered:connect(function() log("Pointer entered") end)
button.unhovered:connect(function() log("Pointer left") end)"#,
            },
            Function {
                name: "window:slider",
                description: "Adds a whole-number slider. changed(value) fires for each different value, including while dragging. get() reads its value. set(value) updates it and fires changed if the value changes.",
                parameters: &[
                    ("label: string", "The caption beside the slider."),
                    ("min: number", "The minimum whole number."),
                    ("max: number", "The maximum whole number. It must exceed min by at most 2,147,483,647."),
                    ("initial: number?", "The starting value. Defaults to min."),
                ],
                returns: Some("A slider with get(), set(value), and a changed signal."),
                example: r#"local window = gui:window("Settings")
local delay = window:slider("Delay (ms)", 0, 1000, 250)
delay.changed:connect(function(value)
    log("Delay changed to", value)
end)"#,
            },
            Function {
                name: "window:toggle",
                description: "Adds a checkbox. changed(value) receives true or false when the checked state changes. Use get() or set(value) in a macro callback.",
                parameters: &[
                    ("label: string", "The caption beside the checkbox."),
                    ("initial: boolean?", "The starting state. Defaults to false."),
                ],
                returns: Some("A toggle with get(), set(value), and a changed signal."),
                example: r#"local window = gui:window("Settings")
local enabled = window:toggle("Enabled", true)
enabled.changed:connect(function(value) log("Enabled", value) end)"#,
            },
            Function {
                name: "window:input",
                description: "Adds a single-line text field. changed(text) fires as the text changes. Use get() or set(text) to read or replace its contents.",
                parameters: &[
                    ("label: string", "The caption for the field."),
                    ("initial: string?", "The starting text. Defaults to an empty string."),
                ],
                returns: Some("A text field with get(), set(text), and a changed signal."),
                example: r#"local window = gui:window("Message")
local message = window:input("Text", "Hello")
message.changed:connect(function(text) log(text) end)"#,
            },
            Function {
                name: "window:label",
                description: "Adds a line of text. Use set(text) to display status from a hotkey or a GUI callback.",
                parameters: &[("text: string", "The initial label text.")],
                returns: Some("A label with get() and set(text)."),
                example: r#"local window = gui:window("Status")
local status = window:label("Ready")
local button = window:button("Update status")
button.pressed:connect(function() status:set("Done") end)"#,
            },
            Function {
                name: "window:keybind",
                description: "Adds a keybind picker. Activate it, then press a supported keyboard combination. Hold Win as a modifier or tap it to record it alone. Escape, a second click, or loss of focus cancels. changed(combo) receives the recorded combination or nil when cleared with set(nil). get() returns the current combo. Use it with press() to change the keys a macro sends.",
                parameters: &[
                    ("label: string", "The caption on the picker."),
                    ("initial: Combo?", "The starting combo. Omit it to start unassigned."),
                ],
                returns: Some("A keybind picker with get(), set(combo), and a changed signal."),
                example: r#"local window = gui:window("Choose what to send")
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
end)"#,
            },
            Function {
                name: "signal:connect",
                description: "Connects a Lua function to a signal. Connections run in registration order. pressed, hovered, unhovered, and closed pass no arguments. changed passes the control's new value. A callback can update other controls. Errors appear in the script log.",
                parameters: &[("callback: (...any) -> ()", "The function to run when this signal fires.")],
                returns: Some("A connection. Call disconnect() to stop receiving the signal."),
                example: r#"local window = gui:window("Settings")
local delay = window:slider("Delay", 0, 1000, 250)
delay.changed:connect(function(value)
    log("New delay", value)
end)
delay:set(500)"#,
            },
            Function {
                name: "window:show",
                description: "Shows a window created with visible set to false or hidden using hide(). Register a global hotkey at startup to show a window from the background.",
                parameters: &[],
                returns: None,
                example: r#"local window = gui:window("Settings", false)
window:label("Opened with a hotkey")
local closed = false
window.closed:connect(function() closed = true end)
hotkey("Ctrl+Alt+M", function()
    if not closed then window:show() end
end)"#,
            },
            Function {
                name: "window:hide",
                description: "Hides the window and keeps its values, signals, and hotkeys active. The window can be shown again with show(). A hidden window still keeps the script running.",
                parameters: &[],
                returns: None,
                example: r#"local window = gui:window("Settings")
local hide = window:button("Hide settings")
hide.pressed:connect(function() window:hide() end)
local closed = false
window.closed:connect(function() closed = true end)
hotkey("Ctrl+Alt+M", function()
    if not closed then window:show() end
end)"#,
            },
            Function {
                name: "window:close",
                description: "Permanently closes the macro window and fires closed once. The Basic window's Close button has the same effect. A script with no hotkeys then exits. A script with hotkeys continues until you stop it.",
                parameters: &[],
                returns: None,
                example: r#"local window = gui:window("Finished")
local done = window:button("Close")
window.closed:connect(function() log("Window closed") end)
done.pressed:connect(function() window:close() end)"#,
            },
        ],
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Page {
    Start,
    Types,
    Type(usize),
    Category(usize),
    Function(usize, usize),
}

impl Page {
    pub fn id(self) -> isize {
        match self {
            Self::Start => 0,
            Self::Types => 1,
            Self::Type(index) => (index + 2) as isize,
            Self::Category(group) => ((group + 1) << 8) as isize,
            Self::Function(group, function) => (((group + 1) << 8) | (function + 1)) as isize,
        }
    }

    pub fn from_id(id: isize) -> Option<Self> {
        let id = usize::try_from(id).ok()?;
        match id {
            0 => return Some(Self::Start),
            1 => return Some(Self::Types),
            2..=255 => return TYPES.get(id - 2).map(|_| Self::Type(id - 2)),
            _ => {}
        }
        let group = (id >> 8).checked_sub(1)?;
        let category = CATEGORIES.get(group)?;
        let item = id & 0xff;
        if item == 0 {
            Some(Self::Category(group))
        } else {
            category.functions.get(item - 1)?;
            Some(Self::Function(group, item - 1))
        }
    }

    pub fn example(self) -> Option<&'static str> {
        match self {
            Self::Start => Some(STARTER),
            Self::Types => Some(TYPE_EXAMPLE),
            Self::Type(index) => Some(TYPES[index].example),
            Self::Category(_) => None,
            Self::Function(group, index) => Some(CATEGORIES[group].functions[index].example),
        }
    }

    pub fn rtf(self, width: i32) -> String {
        let mut page = Document::new(width);
        match self {
            Self::Start => {
                page.title("Getting started");
                page.paragraph("Write a Lua 5.1 script and click Run. Stop releases its shortcuts. New scripts use .lhk, and you can open .luahotkeys and .lua files too.");
                page.heading("Your first hotkey");
                page.code(STARTER);
                page.paragraph("Run the example, switch to a text editor, then press Ctrl+Alt+J.");
                page.heading("Autocomplete");
                page.paragraph("Type a function or keyword, or press Ctrl+Space. Use the arrow keys to select a suggestion, then press Tab or Enter. Escape closes the list.");
                page.paragraph("Tab indents to four-space stops and Shift+Tab removes an indent. Enter follows the enclosing Lua block, including functions, loops, and tables.");
                page.heading("Key combinations");
                page.paragraph("Open Types → Combo for the full list of accepted keys, modifiers, and example shortcuts.");
                page.heading("Explorer and updates");
                page.paragraph("Setup registers both LuaHotkeys extensions. In the portable build, choose Associate files from the tray menu for Explorer’s New and Edit actions. Click Update in the editor to check for a new release.");
            }
            Self::Types => {
                page.title("Types");
                page.paragraph("Open a type in the sidebar to see which values it describes. Function signatures show each argument’s type. A question mark marks an optional value.");
                page.heading("Annotate variables and functions");
                page.paragraph("Add : Type after a local variable or function parameter. Add : Type after the closing parenthesis of a function to describe its return value.");
                page.heading("Define your own types");
                page.paragraph("Use type Name = ... to define an alias. Alias definitions can describe existing types, tables with named fields, optional values, and unions such as string | number.");
                page.code(TYPE_EXAMPLE);
                page.paragraph("Annotations describe the source. LuaHotkeys removes them before running the script. Functions such as hotkey and press validate their arguments at runtime.");
            }
            Self::Type(index) => {
                let doc = &TYPES[index];
                page.paragraph_with("Types", 20, 2, false, 70);
                page.title(doc.name);
                page.paragraph(doc.description);
                page.code(doc.definition);
                if doc.name == "Combo" {
                    page.combo();
                }
                page.heading("Example");
                page.code(doc.example);
            }
            Self::Category(group) => {
                let category = &CATEGORIES[group];
                page.title(category.name);
                page.paragraph(category.description);
                for function in category.functions {
                    page.heading(function.name);
                    page.code(&signature(function.name));
                    page.paragraph(function.description);
                }
            }
            Self::Function(group, index) => {
                let category = &CATEGORIES[group];
                let function = &category.functions[index];
                page.paragraph_with(category.name, 20, 2, false, 70);
                page.title(function.name);
                page.paragraph(function.description);
                page.heading("Signature");
                page.code(&signature(function.name));
                if !function.parameters.is_empty() {
                    page.heading("Parameters");
                    for (name, description) in function.parameters {
                        page.argument(name, description);
                    }
                }
                if let Some(returns) = function.returns {
                    page.heading("Returns");
                    page.paragraph(returns);
                }
                page.heading("Example");
                page.code(function.example);
            }
        }
        page.finish()
    }
}

fn signature(name: &str) -> String {
    format!("{name}{}", completion::signature(name).unwrap_or("()"))
}

const SYNTAX_FIRST: usize = 4;

struct Document {
    rtf: String,
    width: i32,
}

impl Document {
    fn new(width: i32) -> Self {
        let mut rtf = String::from(
            r"{\rtf1\ansi\deff0\uc1{\fonttbl{\f0 Segoe UI;}{\f1 Consolas;}}{\colortbl;",
        );
        for color in [theme::TEXT, theme::MUTED, theme::BORDER]
            .into_iter()
            .chain(highlight::COLORS)
        {
            let _ = write!(
                rtf,
                r"\red{}\green{}\blue{};",
                color & 255,
                (color >> 8) & 255,
                (color >> 16) & 255
            );
        }
        rtf.push('}');
        Self { rtf, width }
    }

    fn finish(mut self) -> String {
        self.rtf.push('}');
        self.rtf
    }

    fn title(&mut self, text: &str) {
        self.paragraph_with(text, 40, 1, true, 150);
    }

    fn heading(&mut self, text: &str) {
        self.paragraph_with(text, 27, 1, true, 100);
    }

    fn combo(&mut self) {
        self.heading("Write a combination");
        self.paragraph("Write the modifiers first, then one main key or mouse button. Separate the parts with +. Spaces around the parts and letter case are ignored. Ctrl+Alt+J and alt + control + j are the same combination. A single key such as Enter also works.");
        self.heading("Modifiers");
        self.paragraph("Ctrl (or Control), Alt, Shift, and Win (or Windows). Each modifier may appear once, in any order. Win alone means the left Windows or Super key. Use names such as RightWin and LeftCtrl to select a specific side.");
        self.heading("Accepted main keys");
        self.argument("Letters and digits", "A through Z and 0 through 9.");
        self.argument("Function keys", "F1 through F24. F12 can be sent with press, but Windows reserves it for the debugger and hotkey cannot register it.");
        self.argument("Number pad", "NumPad0 through NumPad9.");
        for (group, keys) in shortcut::NAMED_KEYS {
            let names = keys
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", ");
            self.argument(group, &names);
        }
        self.paragraph("Use press with MouseLeft, MouseRight, or MouseMiddle to click at the pointer. Ctrl+MouseLeft holds Ctrl during the click. hotkey registers keyboard combinations.");
        self.heading("Troubleshooting");
        self.paragraph("Ctrl+S+T has two main keys. Ctrl++ and Ctrl+Plus both use plus as the main key. Use Backslash or an escaped backslash in a Lua string. Hash and # both name the number sign. Symbol shortcuts use the active keyboard layout.");
        self.paragraph("Windows and other apps can reserve shortcuts, especially combinations with Win. Choose another combination if Activity reports a registration error. For press, switch focus to the window you want to receive the keystroke.");
        self.heading("Autocomplete");
        self.paragraph("Suggestions appear in quoted shortcuts passed to hotkey, press, or a keybind picker, and in local Combo assignments. Type Ct to get Ctrl, then type + to start the next part. Use the arrow keys and Tab to choose a suggestion.");
    }

    fn paragraph(&mut self, text: &str) {
        self.paragraph_with(text, 23, 2, false, 170);
    }

    fn paragraph_with(&mut self, text: &str, size: u32, color: u32, bold: bool, spacing: u32) {
        let bold = if bold { r"\b" } else { r"\b0" };
        let _ = write!(
            self.rtf,
            r"{{\pard\li180\ri180\sa{spacing}\f0\fs{size}\cf{color}{bold} "
        );
        escaped(&mut self.rtf, text);
        self.rtf.push_str(r"\par}");
    }

    fn argument(&mut self, name: &str, description: &str) {
        self.rtf.push_str(r"{\pard\li180\ri180\sa95\f1\fs21\cf1\b ");
        escaped(&mut self.rtf, name);
        self.rtf.push_str(r" \b0\f0\fs22\cf2  ");
        escaped(&mut self.rtf, description);
        self.rtf.push_str(r"\par}");
    }

    fn code(&mut self, text: &str) {
        // RichEdit paints shaded cells with a single foreground color, overriding syntax colors.
        let edge = (self.width - 180).max(720);
        let _ = write!(
            self.rtf,
            concat!(
                r"{{\trowd\trleft180\trgaph180",
                r"\clbrdrt\brdrs\brdrw15\brdrcf3\clbrdrb\brdrs\brdrw15\brdrcf3",
                r"\clbrdrl\brdrs\brdrw15\brdrcf3\clbrdrr\brdrs\brdrw15\brdrcf3",
                r"\cellx{edge}\pard\intbl\sb120\sa120\sl320\slmult0\f1\fs21\cf1 "
            ),
            edge = edge
        );
        let mut end = 0;
        for (start, next, color) in highlight::tokens(text) {
            escaped(&mut self.rtf, &text[end..start]);
            let index = highlight::COLORS
                .iter()
                .position(|&value| value == color)
                .map_or(1, |index| index + SYNTAX_FIRST);
            let _ = write!(self.rtf, r"\cf{index} ");
            escaped(&mut self.rtf, &text[start..next]);
            self.rtf.push_str(r"\cf1 ");
            end = next;
        }
        escaped(&mut self.rtf, &text[end..]);
        self.rtf.push_str(r"\cell\row}{\pard\fs4\sa110\par}");
    }
}

fn escaped(rtf: &mut String, text: &str) {
    for unit in text.encode_utf16() {
        match unit {
            92 | 123 | 125 => {
                rtf.push('\\');
                rtf.push(char::from(unit as u8));
            }
            10 => rtf.push_str(r"\line "),
            9 => rtf.push_str(r"\tab "),
            32..=126 => rtf.push(char::from(unit as u8)),
            unit => {
                let _ = write!(rtf, r"\u{}?", unit as i16);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_covers_the_api_and_shares_editor_syntax_colors() {
        assert_eq!(
            CATEGORIES
                .iter()
                .map(|category| category.functions.len())
                .sum::<usize>(),
            28
        );
        for (group, category) in CATEGORIES.iter().enumerate() {
            let page = Page::Category(group);
            assert_eq!(Page::from_id(page.id()), Some(page));
            let reference = page.rtf(6000);
            for (index, function) in category.functions.iter().enumerate() {
                assert!(completion::signature(function.name).is_some());
                let page = Page::Function(group, index);
                assert_eq!(Page::from_id(page.id()), Some(page));
                assert!(reference.contains(function.name));
                assert!(page.example().is_some());
                assert!(page.rtf(6000).contains(r"\brdrcf3\cellx5820"));
            }
        }
        let timing = Page::Function(3, 0).rtf(6000);
        assert!(timing.contains(r"\cf9 math"));
        assert!(timing.contains(r"\cf8 floor"));
    }
}
