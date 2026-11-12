use std::fmt::Write;

use crate::{completion, editor::color, highlight, theme};

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

pub const CATEGORIES: &[Category] = &[
    Category {
        name: "Hotkeys",
        description: "Declare global shortcuts when a script starts. Each shortcut runs a Lua function when you press it.",
        functions: &[Function {
            name: "hotkey",
            description: "Registers a global key combination. Declare it at the top level of your script so LuaHotkeys can bind it when you click Run.",
            parameters: &[
                ("combo", "A shortcut such as Ctrl+Alt+J. Key names are case-insensitive."),
                ("callback", "The Lua function to run when the shortcut is pressed."),
            ],
            returns: None,
            example: STARTER,
        }],
    },
    Category {
        name: "Keyboard",
        description: "Send literal text or keyboard shortcuts to the focused window.",
        functions: &[
            Function {
                name: "type_text",
                description: "Types text into the focused window. Newlines send Enter.",
                parameters: &[("text", "The text to type.")],
                returns: None,
                example: r#"type_text("First line\nSecond line")"#,
            },
            Function {
                name: "press",
                description: "Sends one key or a key combination to the focused window.",
                parameters: &[("combo", "A key or combination such as Enter, Escape, or Ctrl+S.")],
                returns: None,
                example: r#"press("Ctrl+S")"#,
            },
        ],
    },
    Category {
        name: "Mouse",
        description: "Read the pointer position, move it, or click a mouse button.",
        functions: &[
            Function {
                name: "click",
                description: "Clicks a button at the current pointer position.",
                parameters: &[("button", "Use left, right, or middle. Omit the argument for a left click.")],
                returns: None,
                example: r#"click("right")"#,
            },
            Function {
                name: "move_mouse",
                description: "Moves the pointer to desktop coordinates. Coordinates are measured in pixels.",
                parameters: &[("x", "Horizontal position."), ("y", "Vertical position.")],
                returns: None,
                example: "move_mouse(400, 300)",
            },
            Function {
                name: "mouse_pos",
                description: "Reads the pointer’s current desktop coordinates.",
                parameters: &[],
                returns: Some("Two numbers in order, x and y."),
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
            parameters: &[("milliseconds", "A nonnegative whole number of milliseconds.")],
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
                    ("program", "The app, file, or URL to open."),
                    ("...", "Optional command-line arguments as separate strings."),
                ],
                returns: None,
                example: r#"run("notepad.exe")"#,
            },
            Function {
                name: "log",
                description: "Writes values to the Activity pane. Values are converted to text and separated by tabs.",
                parameters: &[("...", "The values to write.")],
                returns: None,
                example: r#"log("Macro started", os.date("%H:%M:%S"))"#,
            },
            Function {
                name: "print",
                description: "Writes values to the Activity pane, just like log.",
                parameters: &[("...", "The values to write.")],
                returns: None,
                example: r#"print(string.format("Result: %d", 42))"#,
            },
        ],
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Page {
    Start,
    Category(usize),
    Function(usize, usize),
}

impl Page {
    pub fn id(self) -> isize {
        match self {
            Self::Start => 0,
            Self::Category(group) => ((group + 1) << 8) as isize,
            Self::Function(group, function) => (((group + 1) << 8) | (function + 1)) as isize,
        }
    }

    pub fn from_id(id: isize) -> Option<Self> {
        let id = usize::try_from(id).ok()?;
        if id == 0 {
            return Some(Self::Start);
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
            Self::Category(_) => None,
            Self::Function(group, index) => Some(CATEGORIES[group].functions[index].example),
        }
    }

    pub fn rtf(self) -> String {
        let mut page = Document::new();
        match self {
            Self::Start => {
                page.title("Getting started");
                page.paragraph("Write a Lua 5.1 script and click Run. Stop releases its shortcuts. New scripts use .lhk, and you can open .luahotkeys and .lua files too.");
                page.heading("Your first hotkey");
                page.code(STARTER);
                page.paragraph("Run the example, switch to a text editor, then press Ctrl+Alt+J.");
                page.heading("Autocomplete");
                page.paragraph("Type a function or keyword, or press Ctrl+Space. Use the arrow keys to select a suggestion, then press Tab or Enter. Escape closes the list.");
                page.heading("Key combinations");
                page.paragraph("Combine Ctrl, Alt, Shift, or Win with a letter, digit, function key, or name such as Left, PageDown, Space, or NumPad1. Choose a different combination if Windows reports a conflict.");
                page.heading("Explorer and updates");
                page.paragraph("Setup registers both LuaHotkeys extensions. In the portable build, choose Associate files from the tray menu for Explorer’s New and Edit actions. Click Update in the editor to check for a new release.");
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

const CODE_BACKGROUND: u32 = color(29, 40, 58);
const SYNTAX_FIRST: usize = 4;

struct Document(String);

impl Document {
    fn new() -> Self {
        let mut rtf = String::from(
            r"{\rtf1\ansi\deff0\uc1{\fonttbl{\f0 Segoe UI;}{\f1 Consolas;}}{\colortbl;",
        );
        for color in [theme::TEXT, theme::MUTED, CODE_BACKGROUND]
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
        Self(rtf)
    }

    fn finish(mut self) -> String {
        self.0.push('}');
        self.0
    }

    fn title(&mut self, text: &str) {
        self.paragraph_with(text, 40, 1, true, 150);
    }

    fn heading(&mut self, text: &str) {
        self.paragraph_with(text, 27, 1, true, 100);
    }

    fn paragraph(&mut self, text: &str) {
        self.paragraph_with(text, 23, 2, false, 170);
    }

    fn paragraph_with(&mut self, text: &str, size: u32, color: u32, bold: bool, spacing: u32) {
        let bold = if bold { r"\b" } else { r"\b0" };
        let _ = write!(
            self.0,
            r"{{\pard\li180\ri180\sa{spacing}\f0\fs{size}\cf{color}{bold} "
        );
        escaped(&mut self.0, text);
        self.0.push_str(r"\par}");
    }

    fn argument(&mut self, name: &str, description: &str) {
        self.0.push_str(r"{\pard\li180\ri180\sa95\f1\fs21\cf1\b ");
        escaped(&mut self.0, name);
        self.0.push_str(r" \b0\f0\fs22\cf2  ");
        escaped(&mut self.0, description);
        self.0.push_str(r"\par}");
    }

    fn code(&mut self, text: &str) {
        self.0.push_str(r"{\pard\li360\ri360\sb100\sa190\sl320\slmult0\f1\fs21\cf1\shading10000\cfpat3\cbpat3\highlight3 ");
        let mut end = 0;
        for (start, next, color) in highlight::tokens(text) {
            escaped(&mut self.0, &text[end..start]);
            let index = highlight::COLORS
                .iter()
                .position(|&value| value == color)
                .map_or(1, |index| index + SYNTAX_FIRST);
            let _ = write!(self.0, r"\cf{index} ");
            escaped(&mut self.0, &text[start..next]);
            self.0.push_str(r"\cf1 ");
            end = next;
        }
        escaped(&mut self.0, &text[end..]);
        self.0.push_str(r"\par}");
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
            10
        );
        for (group, category) in CATEGORIES.iter().enumerate() {
            let page = Page::Category(group);
            assert_eq!(Page::from_id(page.id()), Some(page));
            let reference = page.rtf();
            for (index, function) in category.functions.iter().enumerate() {
                assert!(completion::signature(function.name).is_some());
                let page = Page::Function(group, index);
                assert_eq!(Page::from_id(page.id()), Some(page));
                assert!(reference.contains(function.name));
                assert!(page.example().is_some());
                assert!(page.rtf().contains(r"\highlight3"));
            }
        }
        let timing = Page::Function(3, 0).rtf();
        assert!(timing.contains(r"\cf9 math"));
        assert!(timing.contains(r"\cf8 floor"));
    }
}
