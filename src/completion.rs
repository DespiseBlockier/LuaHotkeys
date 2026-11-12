use crate::lua_syntax::{is_word_byte, Kind, Token, Tokens};
use std::{collections::VecDeque, ops::Range};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Style {
    Call,
    Keyword,
    Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Item {
    pub name: &'static str,
    pub detail: &'static str,
    style: Style,
}

const fn call(name: &'static str, detail: &'static str) -> Item {
    Item {
        name,
        detail,
        style: Style::Call,
    }
}

const fn keyword(name: &'static str) -> Item {
    Item {
        name,
        detail: "Lua keyword",
        style: Style::Keyword,
    }
}

const fn value(name: &'static str, detail: &'static str) -> Item {
    Item {
        name,
        detail,
        style: Style::Value,
    }
}

const API: &[Item] = &[
    call("hotkey", "(combo, callback)"),
    call("type_text", "(text)"),
    call("press", "(combo)"),
    call("click", "([button])"),
    call("move_mouse", "(x, y)"),
    call("mouse_pos", "()"),
    call("sleep", "(milliseconds)"),
    call("run", "(program, ...)"),
    call("log", "(...)"),
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
    call("print", "(...)"),
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

pub fn is_global_call(name: &str) -> bool {
    API.iter()
        .chain(GLOBALS)
        .any(|item| item.name == name && item.style == Style::Call)
}

pub struct Suggestions {
    pub range: Range<usize>,
    pub items: Vec<Item>,
    expression: bool,
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
    if bytes.get(start).is_some_and(u8::is_ascii_digit) {
        return None;
    }

    let mut previous = VecDeque::new();
    let mut blocks = Vec::new();
    for token in Tokens::new(source).take_while(|token| token.start <= caret) {
        if token.blocks_completion(caret)
            || (token.kind == Kind::Number && token.start < caret && caret <= token.end)
        {
            return None;
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
    let last = previous.back().map_or("", |token| token.text);
    if prefix.is_empty() && !manual && last != "." {
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

    if last == "." {
        let namespace = previous
            .iter()
            .rev()
            .nth(1)
            .filter(|token| token.kind == Kind::Word)?
            .text;
        if namespace == "_G" {
            candidates.extend_from_slice(API);
            candidates.extend_from_slice(GLOBALS);
        } else {
            candidates.extend_from_slice(members(namespace));
        }
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
            if statement_start || expression || !prefix.is_empty() || manual {
                candidates.extend_from_slice(API);
                candidates.extend_from_slice(GLOBALS);
            }
            if statement_start && !expression {
                candidates.extend(STATEMENTS.iter().copied().filter(|item| {
                    match item.name {
                        "break" => blocks
                            .iter()
                            .rev()
                            .take_while(|&&block| block != Block::Function)
                            .any(|block| matches!(block, Block::Loop | Block::Repeat)),
                        "else" | "elseif" => blocks.last() == Some(&Block::IfBody),
                        "end" => blocks.last().is_some_and(|block| *block != Block::Repeat),
                        "until" => blocks.last() == Some(&Block::Repeat),
                        _ => true,
                    }
                }));
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
        item.name.starts_with(prefix)
            && (item.name != &source[start..end]
                || item.style == Style::Call
                || (item.style == Style::Keyword && is_template(item.name)))
    });
    candidates.sort_by_key(|item| item.name != prefix);
    (!candidates.is_empty()).then_some(Suggestions {
        range: start..end,
        items: candidates,
        expression,
    })
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
    pub fn edit(&self, source: &str, item: Item) -> Edit {
        let end = self.range.end;
        let tail = &source[end..];
        let edit = |replacement: String, offset| Edit {
            replacement,
            caret: self.range.start + offset,
        };
        if item.style == Style::Call {
            if tail.trim_start().starts_with('(') {
                let distance = tail.len() - tail.trim_start().len();
                return edit(item.name.into(), item.name.len() + distance + 1);
            }
            return edit(format!("{}()", item.name), item.name.len() + 1);
        }
        if item.style != Style::Keyword || !tail.split('\n').next().unwrap_or("").trim().is_empty()
        {
            return edit(item.name.into(), item.name.len());
        }

        let before = &source[..self.range.start];
        let line = before.rsplit('\n').next().unwrap_or("");
        let indent = &line[..line
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count()];
        let block = |head: &str, cursor| edit(format!("{head}\n{indent}    \n{indent}end"), cursor);
        match item.name {
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
            _ => edit(item.name.into(), item.name.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed(source: &str, position: usize, name: &str) -> String {
        let suggestions = suggest(source, position, false).unwrap();
        let item = *suggestions
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
}
