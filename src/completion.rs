use crate::lua_syntax::{is_word_byte, Kind, Token, Tokens};
use std::{borrow::Cow, collections::VecDeque, ops::Range};

mod access;
mod combo;
mod gui_api;
mod values;
mod variables;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Style {
    Call,
    Keyword,
    Value,
    TableStringKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item {
    pub name: Cow<'static, str>,
    pub detail: &'static str,
    style: Style,
}

const fn call(name: &'static str, detail: &'static str) -> Item {
    Item {
        name: Cow::Borrowed(name),
        detail,
        style: Style::Call,
    }
}

const fn keyword(name: &'static str) -> Item {
    Item {
        name: Cow::Borrowed(name),
        detail: "Lua keyword",
        style: Style::Keyword,
    }
}

const fn value(name: &'static str, detail: &'static str) -> Item {
    Item {
        name: Cow::Borrowed(name),
        detail,
        style: Style::Value,
    }
}

const API: &[Item] = &[
    call(
        "hotkey",
        "(combo: Combo, callback: HotkeyCallback, trigger: \"release\" | \"press\"?)",
    ),
    call("type_text", "(text: string)"),
    call("press", "(combo: Combo)"),
    call("click", "(button: MouseButton?)"),
    call("move_mouse", "(x: Pixel, y: Pixel)"),
    call("mouse_pos", "(): (Pixel, Pixel)"),
    call("sleep", "(milliseconds: Milliseconds)"),
    call("run", "(program: string, ...: string)"),
    call("log", "(...: any)"),
    call("defineGui", "(engine: \"Basic\" | \"Imgui\")"),
    value("gui", "Basic GUI engine"),
];

const ENGINE_METHODS: &[Item] = &[call(
    "window",
    "(title: string, options: (boolean | table)?)",
)];
const WINDOW_METHODS: &[Item] = &[
    call("label", "(text: string)"),
    call("button", "(text: string)"),
    call(
        "slider",
        "(label: string, min: number, max: number, initial: number?)",
    ),
    call("toggle", "(label: string, initial: boolean?)"),
    call("input", "(label: string, initial: string?)"),
    call("keybind", "(label: string, initial: Combo?)"),
    call("tab", "(label: string)"),
    call("folder", "(label: string, open: boolean?)"),
    call("dock", "()"),
    call(
        "dropdown",
        "(label: string, options: {string}, initial: string?)",
    ),
    call("color", "(label: string, initial: string?)"),
    call("collapse", "()"),
    call("expand", "()"),
    call("is_collapsed", "(): boolean"),
    call("set_position", "(x: number, y: number)"),
    call("show", "()"),
    call("hide", "()"),
    call("close", "()"),
];
const SIGNAL_METHODS: &[Item] = &[call("connect", "(callback: (...any) -> ())")];
const CONNECTION_METHODS: &[Item] = &[call("disconnect", "()")];
const CONTROL_METHODS: &[Item] = &[
    call("get", "()"),
    call("set", "(value: any)"),
    call("switch", "()"),
    call("select", "()"),
    call("set_color", "(color: string?)"),
    call("get_color", "(): string?"),
    call("add_option", "(name: string)"),
    call("remove_option", "(name: string)"),
    call("destroy", "()"),
];

const GLOBALS: &[Item] = &[
    call("assert", "(value, [message])"),
    call("collectgarbage", "([option, arg])"),
    call("dofile", "([filename])"),
    call("error", "(message, [level])"),
    call("getfenv", "([function])"),
    call("getmetatable", "(object)"),
    call("ipairs", "(table)"),
    call("load", "(reader, [name])"),
    call("loadfile", "([filename])"),
    call("loadstring", "(text, [name])"),
    call("module", "(name, ...)"),
    call("next", "(table, [key])"),
    call("pairs", "(table)"),
    call("pcall", "(function, ...)"),
    call("print", "(...: any)"),
    call("rawequal", "(a, b)"),
    call("rawget", "(table, key)"),
    call("rawset", "(table, key, value)"),
    call("require", "(name)"),
    call("select", "(index, ...)"),
    call("setfenv", "(function, environment)"),
    call("setmetatable", "(table, metatable)"),
    call("tonumber", "(value, [base])"),
    call("tostring", "(value)"),
    call("type", "(value)"),
    call("unpack", "(table, [i, j])"),
    call("xpcall", "(function, handler)"),
    value("_G", "Global table"),
    value("_VERSION", "Lua version"),
    value("coroutine", "Lua table"),
    value("debug", "Lua table"),
    value("io", "Lua table"),
    value("math", "Lua table"),
    value("os", "Lua table"),
    value("package", "Lua table"),
    value("string", "Lua table"),
    value("table", "Lua table"),
];

const EXPRESSIONS: &[Item] = &[
    keyword("false"),
    keyword("function"),
    keyword("nil"),
    keyword("not"),
    keyword("true"),
];
const STATEMENTS: &[Item] = &[
    keyword("break"),
    keyword("do"),
    keyword("else"),
    keyword("elseif"),
    keyword("end"),
    keyword("for"),
    keyword("function"),
    keyword("if"),
    keyword("local"),
    keyword("repeat"),
    keyword("return"),
    keyword("until"),
    keyword("while"),
];
const OPERATORS: &[Item] = &[keyword("and"), keyword("or")];

fn members(namespace: &str) -> &'static [Item] {
    match namespace {
        "gui" => ENGINE_METHODS,
        "coroutine" => {
            const {
                &[
                    call("create", "(function)"),
                    call("resume", "(thread, ...)"),
                    call("running", "()"),
                    call("status", "(thread)"),
                    call("wrap", "(function)"),
                    call("yield", "(...)"),
                ]
            }
        }
        "debug" => {
            const {
                &[
                    call("debug", "()"),
                    call("getfenv", "(object)"),
                    call("gethook", "([thread])"),
                    call("getinfo", "(function, [what])"),
                    call("getlocal", "(level, index)"),
                    call("getmetatable", "(object)"),
                    call("getregistry", "()"),
                    call("getupvalue", "(function, index)"),
                    call("setfenv", "(object, table)"),
                    call("sethook", "(hook, mask, [count])"),
                    call("setlocal", "(level, index, value)"),
                    call("setmetatable", "(object, table)"),
                    call("setupvalue", "(function, index, value)"),
                    call("traceback", "([message, level])"),
                ]
            }
        }
        "io" => {
            const {
                &[
                    call("close", "([file])"),
                    call("flush", "()"),
                    call("input", "([file])"),
                    call("lines", "([filename])"),
                    call("open", "(filename, [mode])"),
                    call("output", "([file])"),
                    call("popen", "(program, [mode])"),
                    call("read", "(...)"),
                    call("tmpfile", "()"),
                    call("type", "(object)"),
                    call("write", "(...)"),
                    value("stdin", "File handle"),
                    value("stdout", "File handle"),
                    value("stderr", "File handle"),
                ]
            }
        }
        "math" => {
            const {
                &[
                    call("abs", "(x)"),
                    call("acos", "(x)"),
                    call("asin", "(x)"),
                    call("atan", "(x)"),
                    call("atan2", "(y, x)"),
                    call("ceil", "(x)"),
                    call("cos", "(x)"),
                    call("cosh", "(x)"),
                    call("deg", "(x)"),
                    call("exp", "(x)"),
                    call("floor", "(x)"),
                    call("fmod", "(x, y)"),
                    call("frexp", "(x)"),
                    call("ldexp", "(m, e)"),
                    call("log", "(x)"),
                    call("log10", "(x)"),
                    call("max", "(x, ...)"),
                    call("min", "(x, ...)"),
                    call("modf", "(x)"),
                    call("pow", "(x, y)"),
                    call("rad", "(x)"),
                    call("random", "([m, n])"),
                    call("randomseed", "(seed)"),
                    call("sin", "(x)"),
                    call("sinh", "(x)"),
                    call("sqrt", "(x)"),
                    call("tan", "(x)"),
                    call("tanh", "(x)"),
                    value("huge", "Number"),
                    value("pi", "Number"),
                ]
            }
        }
        "os" => {
            const {
                &[
                    call("clock", "()"),
                    call("date", "([format, time])"),
                    call("difftime", "(t2, t1)"),
                    call("execute", "([command])"),
                    call("exit", "([code])"),
                    call("getenv", "(name)"),
                    call("remove", "(filename)"),
                    call("rename", "(oldname, newname)"),
                    call("setlocale", "(locale, [category])"),
                    call("time", "([table])"),
                    call("tmpname", "()"),
                ]
            }
        }
        "package" => {
            const {
                &[
                    value("cpath", "String"),
                    value("loaded", "Lua table"),
                    value("loaders", "Lua table"),
                    call("loadlib", "(path, symbol)"),
                    value("path", "String"),
                    value("preload", "Lua table"),
                    call("seeall", "(module)"),
                ]
            }
        }
        "string" => {
            const {
                &[
                    call("byte", "(s, [i, j])"),
                    call("char", "(...)"),
                    call("dump", "(function)"),
                    call("find", "(s, pattern, [init, plain])"),
                    call("format", "(format, ...)"),
                    call("gmatch", "(s, pattern)"),
                    call("gsub", "(s, pattern, replacement, [n])"),
                    call("len", "(s)"),
                    call("lower", "(s)"),
                    call("match", "(s, pattern, [init])"),
                    call("rep", "(s, n)"),
                    call("reverse", "(s)"),
                    call("sub", "(s, i, [j])"),
                    call("upper", "(s)"),
                ]
            }
        }
        "table" => {
            const {
                &[
                    call("concat", "(table, [sep, i, j])"),
                    call("insert", "(table, [pos,] value)"),
                    call("maxn", "(table)"),
                    call("remove", "(table, [pos])"),
                    call("sort", "(table, [comp])"),
                ]
            }
        }
        _ => &[],
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Builtin {
    Function,
    Library,
    Value,
}

fn global(name: &str) -> Option<&'static Item> {
    API.iter().chain(GLOBALS).find(|item| item.name == name)
}

pub fn signature(name: &str) -> Option<&'static str> {
    if let Some((object, name)) = name.split_once(':') {
        let methods = match object {
            "gui" | "engine" => ENGINE_METHODS,
            "window" => WINDOW_METHODS,
            "tab" | "folder" | "dock" => {
                return WINDOW_METHODS
                    .iter()
                    .chain(CONTROL_METHODS)
                    .find(|item| item.name == name)
                    .map(|item| item.detail)
            }
            "control" | "dropdown" | "picker" => CONTROL_METHODS,
            "connection" => CONNECTION_METHODS,
            "signal" => SIGNAL_METHODS,
            _ => return None,
        };
        return methods
            .iter()
            .find(|item| item.name == name)
            .map(|item| item.detail);
    }
    global(name)
        .filter(|item| item.style == Style::Call)
        .map(|item| item.detail)
}

pub fn builtin(namespace: Option<&str>, name: &str) -> Option<Builtin> {
    let item = match namespace {
        None | Some("_G") => global(name),
        Some(namespace) => members(namespace).iter().find(|item| item.name == name),
    }?;
    match item.style {
        Style::Call => Some(Builtin::Function),
        Style::Value if name == "_G" || !members(name).is_empty() => Some(Builtin::Library),
        Style::Value | Style::TableStringKey => Some(Builtin::Value),
        Style::Keyword => None,
    }
}

pub struct Suggestions {
    pub range: Range<usize>,
    pub items: Vec<Item>,
    expression: bool,
    access: Access,
}

#[derive(Clone, Copy, Default)]
enum Access {
    #[default]
    Plain,
    GuiDot,
    Index,
    QuotedIndex(char),
}

pub struct Edit {
    pub replacement: String,
    pub caret: usize,
}

pub fn suggest(source: &str, caret: usize, manual: bool) -> Option<Suggestions> {
    if !source.is_char_boundary(caret) {
        return None;
    }
    let bytes = source.as_bytes();
    let mut start = caret;
    let mut end = caret;
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    while bytes.get(end).copied().is_some_and(is_word_byte) {
        end += 1;
    }
    let prefix = &source[start..caret];
    let mut previous = VecDeque::new();
    let mut blocks = Vec::new();
    for token in Tokens::new(source).take_while(|token| token.start <= caret) {
        if token.blocks_completion(caret) {
            return gui_engine_suggestions(source, caret, token, &previous)
                .or_else(|| combo::suggest(source, caret, token, &previous, manual))
                .or_else(|| access::quoted_index(source, caret, token, previous.back().copied()));
        }
        if token.kind == Kind::Number && token.start < caret && caret <= token.end {
            let open = previous.back().filter(|open| open.text == "[")?;
            return access::index(
                source,
                *open,
                token.start..token.end,
                &source[token.start..caret],
                true,
            );
        }
        if token.end <= start && token.kind != Kind::Comment {
            if token.kind == Kind::Word {
                observe_block(&mut blocks, token.text);
            }
            if previous.len() == 24 {
                previous.pop_front();
            }
            previous.push_back(token);
        }
    }
    if bytes.get(start).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let last = previous.back().map_or("", |token| token.text);
    if prefix.is_empty() && define_gui_argument(&previous) {
        return Some(Suggestions {
            range: start..end,
            items: [r#""Basic""#, r#""Imgui""#]
                .into_iter()
                .map(|name| value(name, "GUI engine"))
                .collect(),
            expression: true,
            access: Access::Plain,
        });
    }
    if prefix.is_empty() && !manual && !matches!(last, "." | ":" | "[") {
        return None;
    }

    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let on_line = || {
        previous
            .iter()
            .filter(move |token| token.start >= line_start)
    };
    let has = |word| on_line().any(|token| token.text == word);
    let mut candidates = Vec::new();
    let mut expression = false;

    if matches!(last, "." | ":") {
        if last == ":" && ((has("local") && !has("=")) || in_parameter_list(&previous)) {
            return None;
        }
        return access::members(source, &previous, start..end, prefix);
    } else if last == "local" {
        candidates.push(keyword("function"));
    } else if last != ":" && last != "function" && !in_parameter_list(&previous) {
        let needs_then = blocks.last() == Some(&Block::IfHeader);
        let needs_do = matches!(blocks.last(), Some(Block::ForHeader | Block::WhileHeader));
        let binding = (has("local") && !has("=") && !has("function"))
            || (blocks.last() == Some(&Block::ForHeader) && !has("in") && !has("="));
        if binding {
            if blocks.last() == Some(&Block::ForHeader) && !matches!(last, "for" | ",") {
                candidates.push(keyword("in"));
            }
        } else {
            expression = needs_then || needs_do || expects_expression(last);
            let statement_start = source[line_start..start].trim().is_empty()
                || matches!(last, ";" | "then" | "do" | "else" | "repeat");
            let mut symbols = variables::collect(source, start);
            if last == "[" {
                if let Some(items) = access::index_with_symbols(
                    source,
                    &mut symbols,
                    *previous.back()?,
                    start..end,
                    prefix,
                    false,
                ) {
                    return Some(items);
                }
            }
            let names = &symbols.names;
            candidates.extend(
                names
                    .iter()
                    .filter(|name| prefix_matches(name.text, prefix))
                    .map(|name| Item {
                        name: Cow::Owned(name.text.to_owned()),
                        detail: name.value.detail(name.detail),
                        style: Style::Value,
                    }),
            );
            if statement_start || expression || !prefix.is_empty() || manual {
                candidates.extend(
                    API.iter()
                        .chain(GLOBALS)
                        .filter(|item| !names.iter().any(|name| name.text == item.name))
                        .cloned(),
                );
            }
            if statement_start && !expression {
                candidates.extend(
                    STATEMENTS
                        .iter()
                        .filter(|item| match item.name.as_ref() {
                            "break" => blocks
                                .iter()
                                .rev()
                                .take_while(|&&block| block != Block::Function)
                                .any(|block| matches!(block, Block::Loop | Block::Repeat)),
                            "else" | "elseif" => blocks.last() == Some(&Block::IfBody),
                            "end" => blocks.last().is_some_and(|block| *block != Block::Repeat),
                            "until" => blocks.last() == Some(&Block::Repeat),
                            _ => true,
                        })
                        .cloned(),
                );
            } else {
                candidates.extend_from_slice(EXPRESSIONS);
                if !expects_expression(last) {
                    candidates.extend_from_slice(OPERATORS);
                }
            }
            if needs_then {
                candidates.push(keyword("then"));
            }
            if needs_do {
                candidates.push(keyword("do"));
            }
        }
    }

    candidates.retain(|item| {
        prefix_matches(&item.name, prefix)
            && (item.name != source[start..end]
                || item.style == Style::Call
                || (item.style == Style::Keyword && is_template(&item.name)))
    });
    candidates.sort_by_key(|item| item.name != prefix);
    (!candidates.is_empty()).then_some(Suggestions {
        range: start..end,
        items: candidates,
        expression,
        access: Access::Plain,
    })
}

fn prefix_matches(name: &str, prefix: &str) -> bool {
    if name.is_ascii() && prefix.is_ascii() {
        name.get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    } else {
        name.to_lowercase().starts_with(&prefix.to_lowercase())
    }
}

fn gui_engine_suggestions(
    source: &str,
    caret: usize,
    token: Token<'_>,
    previous: &VecDeque<Token<'_>>,
) -> Option<Suggestions> {
    if token.kind != Kind::String
        || !token.text.starts_with(['"', '\''])
        || !define_gui_argument(previous)
    {
        return None;
    }
    let start = token.start + 1;
    let end =
        token.end - usize::from(token.text.len() > 1 && token.text.ends_with(&token.text[..1]));
    let prefix = source.get(start..caret)?;
    let items = ["Basic", "Imgui"]
        .into_iter()
        .filter(|name| {
            name.get(..prefix.len())
                .is_some_and(|part| part.eq_ignore_ascii_case(prefix))
        })
        .filter(|&name| name != &source[start..end])
        .map(|name| value(name, "GUI engine"))
        .collect::<Vec<_>>();
    (!items.is_empty()).then_some(Suggestions {
        range: start..end,
        items,
        expression: false,
        access: Access::Plain,
    })
}

fn define_gui_argument(previous: &VecDeque<Token<'_>>) -> bool {
    let mut preceding = previous.iter().rev().map(|token| token.text);
    preceding.next() == Some("(")
        && preceding.next() == Some("defineGui")
        && match preceding.next() {
            Some(".") => preceding.next() == Some("_G"),
            Some(":" | "function") => false,
            _ => true,
        }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Block {
    IfHeader,
    IfBody,
    ElseBody,
    ForHeader,
    WhileHeader,
    Loop,
    Function,
    Repeat,
    Do,
}

fn observe_block(blocks: &mut Vec<Block>, word: &str) {
    match word {
        "if" => blocks.push(Block::IfHeader),
        "for" => blocks.push(Block::ForHeader),
        "while" => blocks.push(Block::WhileHeader),
        "function" => blocks.push(Block::Function),
        "repeat" => blocks.push(Block::Repeat),
        "elseif" if blocks.last() == Some(&Block::IfBody) => {
            *blocks.last_mut().unwrap() = Block::IfHeader;
        }
        "then" if blocks.last() == Some(&Block::IfHeader) => {
            *blocks.last_mut().unwrap() = Block::IfBody;
        }
        "else" if blocks.last() == Some(&Block::IfBody) => {
            *blocks.last_mut().unwrap() = Block::ElseBody;
        }
        "do" if matches!(blocks.last(), Some(Block::ForHeader | Block::WhileHeader)) => {
            *blocks.last_mut().unwrap() = Block::Loop;
        }
        "do" => blocks.push(Block::Do),
        "until" if blocks.last() == Some(&Block::Repeat) => {
            blocks.pop();
        }
        "end" if blocks.last() != Some(&Block::Repeat) => {
            blocks.pop();
        }
        _ => {}
    }
}

fn in_parameter_list(previous: &VecDeque<Token<'_>>) -> bool {
    let Some(index) = previous.iter().rposition(|token| token.text == "function") else {
        return false;
    };
    let mut header = previous.iter().skip(index + 1).map(|token| token.text);
    header.any(|token| token == "(") && !header.any(|token| token == ")")
}

fn expects_expression(previous: &str) -> bool {
    matches!(
        previous,
        "=" | "("
            | "["
            | "{"
            | ","
            | "+"
            | "-"
            | "*"
            | "/"
            | "%"
            | "^"
            | "#"
            | ".."
            | "=="
            | "~="
            | "<"
            | ">"
            | "<="
            | ">="
            | "and"
            | "or"
            | "not"
            | "in"
            | "return"
            | "until"
    )
}

fn is_template(word: &str) -> bool {
    matches!(
        word,
        "if" | "elseif" | "for" | "while" | "function" | "repeat"
    )
}

impl Suggestions {
    pub fn edit(&self, source: &str, item: &Item) -> Edit {
        let end = self.range.end;
        let tail = &source[end..];
        let edit = |replacement: String, offset| Edit {
            replacement,
            caret: self.range.start + offset,
        };
        if matches!(self.access, Access::Index | Access::QuotedIndex(_)) {
            let mut replacement = match self.access {
                Access::QuotedIndex(quote) => {
                    format!("{}{quote}", access::escape_key(&item.name, quote))
                }
                _ if item.style == Style::TableStringKey => {
                    format!("\"{}\"", access::escape_key(&item.name, '"'))
                }
                _ => item.name.to_string(),
            };
            let trimmed = tail.trim_start_matches([' ', '\t']);
            if trimmed.starts_with(']') {
                let offset = replacement.len() + tail.len() - trimmed.len() + 1;
                return edit(replacement, offset);
            }
            replacement.push(']');
            let offset = replacement.len();
            return edit(replacement, offset);
        }
        let name = if matches!(self.access, Access::GuiDot) {
            let separator = if item.style == Style::Call { ':' } else { '.' };
            format!("{separator}{}", item.name)
        } else {
            item.name.to_string()
        };
        if item.style == Style::Call {
            let length = name.len();
            if tail.trim_start().starts_with('(') {
                let distance = tail.len() - tail.trim_start().len();
                return edit(name, length + distance + 1);
            }
            return edit(format!("{name}()"), length + 1);
        }
        if item.style != Style::Keyword || !tail.split('\n').next().unwrap_or("").trim().is_empty()
        {
            let length = name.len();
            return edit(name, length);
        }

        let before = &source[..self.range.start];
        let line = before.rsplit('\n').next().unwrap_or("");
        let indent = &line[..line
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count()];
        let block = |head: &str, cursor| edit(format!("{head}\n{indent}    \n{indent}end"), cursor);
        match item.name.as_ref() {
            "if" => block("if  then", 3),
            "elseif" => edit("elseif  then".into(), 7),
            "for" => block("for  do", 4),
            "while" => block("while  do", 6),
            "function" if self.expression => block("function()", 9),
            "function" => block("function ()", 9),
            "repeat" => edit(
                format!("repeat\n{indent}    \n{indent}until "),
                7 + indent.len() + 4,
            ),
            "local" | "return" | "until" | "in" | "and" | "or" | "not"
                if !tail.starts_with([' ', '\t']) =>
            {
                edit(format!("{} ", item.name), item.name.len() + 1)
            }
            _ => edit(item.name.to_string(), item.name.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_defined_names_only_in_their_scope() {
        let cases = [
            ("local macroDelay = 250\nprint(macroD", "macroDelay"),
            ("local first, second: number = 1, 2\nprint(sec", "second"),
            (
                "local function run(message: string)\n    print(messa",
                "message",
            ),
            (
                "for index, entry in pairs(items) do\n    print(entr",
                "entry",
            ),
            ("globalDelay = 100\nlog(globalD", "globalDelay"),
            ("local function sendMessage()\n    log(sendM", "sendMessage"),
            ("local macroName = 3; print(macroN", "macroName"),
        ];
        for (source, name) in cases {
            let list = suggest(source, source.len(), false).unwrap();
            let item = list
                .items
                .iter()
                .find(|item| item.name == name)
                .expect(name);
            assert_eq!(list.edit(source, item).replacement, name);
        }
        for (source, name) in [
            ("do\n    local hidden = 1\nend\nprint(hidd", "hidden"),
            (
                "if true then\n    local sibling = 1\nelse\n    print(sibl",
                "sibling",
            ),
            ("local recent = rece", "recent"),
            (
                "local data = {\n    property = true\n}\nprint(prope",
                "property",
            ),
            ("for index, entry in getValues(entr", "entry"),
        ] {
            assert!(
                suggest(source, source.len(), false)
                    .is_none_or(|list| { !list.items.iter().any(|item| item.name == name) }),
                "{name}"
            );
        }
    }

    #[test]
    fn suggests_methods_and_signals_for_inferred_gui_objects() {
        let declarations = concat!(
            "local engine = defineGui(\"Imgui\")\n",
            "local window = engine:window(\"Controls\")\n",
            "local panel = window:tab(\"Main\"):folder(\"Settings\")\n",
            "local mode = panel:dropdown(\"Mode\", {\"One\"})\n",
            "local button = panel:button(\"Apply\")\n",
            "local connection = button.pressed:connect(function() end)\n",
        );
        for (suffix, method, expected) in [
            ("mode:ADD", "add_option", "mode:add_option(|)"),
            ("mode.REM", "remove_option", "mode:remove_option(|)"),
            ("button.pr", "pressed", "button.pressed|"),
            ("button.pressed:CON", "connect", "button.pressed:connect(|)"),
            ("connection:di", "disconnect", "connection:disconnect(|)"),
            ("panel:dr", "dropdown", "panel:dropdown(|)"),
        ] {
            let source = format!("{declarations}{suffix}");
            assert_eq!(
                completed(&source, source.len(), method),
                format!("{declarations}{expected}")
            );
        }
        let source = format!("{declarations}mode:");
        let choices = suggest(&source, source.len(), false).unwrap();
        assert!(choices.items.iter().any(|item| item.name == "get"));
        assert!(!choices.items.iter().any(|item| item.name == "slider"));
        let source = "local window = gui:window(\"Basic\")\nwindow:";
        let choices = suggest(source, source.len(), false).unwrap();
        assert!(choices.items.iter().any(|item| item.name == "button"));
        assert!(!choices.items.iter().any(|item| item.name == "dropdown"));
        assert!(!choices.items.iter().any(|item| item.name == "collapse"));
        let source = format!("{declarations}do\n    local mode = {}\n    mode:ad", "{}");
        assert!(suggest(&source, source.len(), false).is_none());
    }

    #[test]
    fn suggests_table_keys_and_keeps_their_original_spelling() {
        let declarations = concat!(
            "local Config = { DisplayName = 'demo', [\"Max-Speed\"] = 8, ",
            "Nested = { Enabled = true }, [12] = 'explicit', 'first' }\n",
            "local Alias = Config\n",
            "Alias.Nested.ExtraKey = false\n",
        );
        for (suffix, key, expected) in [
            ("Config.dis", "DisplayName", "Config.DisplayName|"),
            ("Config.Nested.en", "Enabled", "Config.Nested.Enabled|"),
            ("Config.Nested.extra", "ExtraKey", "Config.Nested.ExtraKey|"),
            ("Config[\"max", "Max-Speed", "Config[\"Max-Speed\"]|"),
            ("Config[", "Max-Speed", "Config[\"Max-Speed\"]|"),
            ("Config[1", "12", "Config[12]|"),
            ("Config[", "1", "Config[1]|"),
            ("print(con", "Config", "print(Config|"),
        ] {
            let source = format!("{declarations}{suffix}");
            assert_eq!(
                completed(&source, source.len(), key),
                format!("{declarations}{expected}")
            );
        }
        let source = format!("{declarations}Config[\"ma\"]");
        let caret = source.len() - 2;
        assert_eq!(
            completed(&source, caret, "Max-Speed"),
            format!("{declarations}Config[\"Max-Speed\"]|")
        );
        let source = format!("{declarations}Config.ma");
        assert!(suggest(&source, source.len(), false).is_none());
        assert_eq!(completed("MATH", 4, "math"), "math|");
        assert_eq!(completed("math.SI", 7, "sin"), "math.sin(|)");
    }

    fn completed(source: &str, position: usize, name: &str) -> String {
        let suggestions = suggest(source, position, false).unwrap();
        let item = suggestions
            .items
            .iter()
            .find(|item| item.name == name)
            .unwrap();
        let edit = suggestions.edit(source, item);
        let mut updated = source.to_owned();
        updated.replace_range(suggestions.range, &edit.replacement);
        updated.insert(edit.caret, '|');
        updated
    }

    #[test]
    fn calls_insert_arguments_and_keep_existing_parentheses() {
        assert_eq!(completed("hotkey", 6, "hotkey"), "hotkey(|)");
        assert_eq!(
            completed("  hotk  (\"F8\", cb)", 6, "hotkey"),
            "  hotkey  (|\"F8\", cb)"
        );
        assert_eq!(
            completed("local result = math.sq", 22, "sqrt"),
            "local result = math.sqrt(|)"
        );
        assert_eq!(
            completed("-- café 😀\nhotk", "-- café 😀\nhotk".len(), "hotkey"),
            "-- café 😀\nhotkey(|)"
        );
    }

    #[test]
    fn completes_gui_engine_and_picker_options_in_context() {
        let source = "defineGui(";
        assert_eq!(
            completed(source, source.len(), r#""Basic""#),
            r#"defineGui("Basic"|"#
        );
        let source = r#"defineGui("im")"#;
        assert_eq!(
            completed(source, source.len() - 2, "Imgui"),
            r#"defineGui("Imgui|")"#
        );
        let source = r#"window:keybind("Send", "Ctrl+Bac")"#;
        assert_eq!(
            completed(source, source.len() - 2, "Backslash"),
            r#"window:keybind("Send", "Ctrl+Backslash|")"#
        );
    }

    #[test]
    fn contexts_distinguish_literals_members_and_declarations() {
        for source in [
            "-- hotk",
            "--[=[hotk",
            "local x = 'hotk",
            "x = [==[ hotk",
            "object.hotk",
            "local hotk",
            "function f(hotk",
        ] {
            assert!(suggest(source, source.len(), false).is_none(), "{source}");
        }
        let member = suggest("math.", 5, false).unwrap();
        assert!(member.items.iter().any(|item| item.name == "sin"));
        assert!(!member.items.iter().any(|item| item.name == "hotkey"));
        assert!(suggest("x = if", 6, false).is_none());
        assert!(suggest("br", 2, false).is_none());
        assert!(suggest("while true do\nbr", 16, false)
            .unwrap()
            .items
            .iter()
            .any(|item| item.name == "break"));
        assert!(suggest("if\n    ready\nth", 15, false)
            .unwrap()
            .items
            .iter()
            .any(|item| item.name == "then"));
        assert_eq!(
            completed("    if", 6, "if"),
            "    if | then\n        \n    end"
        );
        assert_eq!(
            completed("local fn = fun", 14, "function"),
            "local fn = function(|)\n    \nend"
        );
        assert_eq!(
            completed("local fun", 9, "function"),
            "local function |()\n    \nend"
        );
    }

    #[test]
    fn completes_the_current_combo_segment_in_shortcut_strings() {
        let source = r#"hotkey("Ct", callback)"#;
        assert_eq!(
            completed(source, 10, "Ctrl"),
            r#"hotkey("Ctrl|", callback)"#
        );
        let source = r#"press("Ctrl+A")"#;
        assert_eq!(completed(source, 13, "Alt"), r#"press("Ctrl+Alt|")"#);
        let source = "type SaveKey = Combo\nlocal save: SaveKey = 'Shift+nu'";
        assert_eq!(
            completed(source, source.len() - 1, "NumPad2"),
            "type SaveKey = Combo\nlocal save: SaveKey = 'Shift+NumPad2|'"
        );
        let source = r#"press("Ctrl+Sh+F1")"#;
        assert_eq!(completed(source, 14, "Shift"), r#"press("Ctrl+Shift|+F1")"#);
        let source = r#"press("Ctrl+")"#;
        let list = suggest(source, 12, false).unwrap();
        assert!(list.items.iter().any(|item| item.name == "Alt"));
        assert!(!list.items.iter().any(|item| item.name == "Ctrl"));
        for source in [
            r#"log("Ct"#,
            r#"obj:hotkey("Ct"#,
            r#"hotkey("F1", "Ct"#,
            r#"press("F1+Ct"#,
            "type Message = string\nlocal message: Message = \"Ct",
        ] {
            assert!(suggest(source, source.len(), false).is_none(), "{source}");
        }
    }
}
