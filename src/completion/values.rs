use std::borrow::Cow;

use crate::lua_syntax::{Kind, Token};

use super::{
    gui_api::{Engine, GuiType},
    observe_block,
    variables::identifier,
};

#[derive(Clone, Copy, Default)]
pub(super) enum ValueType {
    #[default]
    Unknown,
    Gui(GuiType),
    Table(usize),
    DefineGui,
    Globals,
}

impl ValueType {
    pub(super) fn detail(self, fallback: &'static str) -> &'static str {
        match self {
            Self::Gui(gui) => gui.detail(),
            Self::Table(_) => "Table",
            _ => fallback,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) enum Key<'a> {
    Text(Cow<'a, str>),
    Number(i64),
}

#[derive(Default)]
pub(super) struct Tables<'a> {
    tables: Vec<Vec<(Key<'a>, ValueType)>>,
}

impl<'a> Tables<'a> {
    pub(super) fn keys(&self, index: usize) -> impl Iterator<Item = &Key<'a>> {
        self.tables[index].iter().map(|(key, _)| key)
    }

    fn get(&self, index: usize, key: &Key<'_>) -> ValueType {
        self.tables[index]
            .iter()
            .find(|(name, _)| name == key)
            .map_or(ValueType::Unknown, |(_, value)| *value)
    }

    pub(super) fn insert(&mut self, index: usize, key: Key<'a>, value: ValueType) {
        let table = &mut self.tables[index];
        if let Some((_, slot)) = table.iter_mut().find(|(name, _)| name == &key) {
            *slot = value;
        } else {
            table.push((key, value));
        }
    }

    pub(super) fn infer(
        &mut self,
        tokens: &[Token<'a>],
        lookup: &impl Fn(&str) -> ValueType,
    ) -> ValueType {
        self.expression(tokens, lookup, 0)
    }

    fn expression(
        &mut self,
        tokens: &[Token<'a>],
        lookup: &impl Fn(&str) -> ValueType,
        depth: usize,
    ) -> ValueType {
        let Some(first) = tokens.first().filter(|_| depth < 32) else {
            return ValueType::Unknown;
        };
        let (mut value, mut rest) = match first.text {
            "{" | "(" => {
                let Some(close) = group_end(tokens) else {
                    return ValueType::Unknown;
                };
                let contents = &tokens[1..close];
                let value = if first.text == "{" {
                    self.table(contents, lookup, depth + 1)
                } else {
                    self.expression(contents, lookup, depth + 1)
                };
                (value, &tokens[close + 1..])
            }
            _ if identifier(first) => (lookup(first.text), &tokens[1..]),
            _ => return ValueType::Unknown,
        };
        while let Some(next) = rest.first() {
            match next.text {
                "." | "[" => {
                    let Some((key, tail)) = field(rest) else {
                        return ValueType::Unknown;
                    };
                    value = match (value, &key) {
                        (ValueType::Table(table), key) => self.get(table, key),
                        (ValueType::Gui(gui), Key::Text(name)) => {
                            gui.field(name).map_or(ValueType::Unknown, ValueType::Gui)
                        }
                        (ValueType::Globals, Key::Text(name)) => lookup(name),
                        _ => ValueType::Unknown,
                    };
                    rest = tail;
                }
                ":" => {
                    let (ValueType::Gui(gui), [_, method, arguments @ ..]) = (value, rest) else {
                        return ValueType::Unknown;
                    };
                    let Some((_, tail)) = arguments_of(arguments) else {
                        return ValueType::Unknown;
                    };
                    value = gui
                        .result(method.text)
                        .map_or(ValueType::Unknown, ValueType::Gui);
                    rest = tail;
                }
                _ if matches!(value, ValueType::DefineGui) => {
                    let Some((arguments, tail)) = arguments_of(rest) else {
                        return ValueType::Unknown;
                    };
                    let engine = arguments.first().and_then(|token| string(*token)).map_or(
                        Engine::Unspecified,
                        |name| {
                            if name.eq_ignore_ascii_case("basic") {
                                Engine::Basic
                            } else if name.eq_ignore_ascii_case("imgui") {
                                Engine::Imgui
                            } else {
                                Engine::Unspecified
                            }
                        },
                    );
                    value = ValueType::Gui(GuiType::engine(engine));
                    rest = tail;
                }
                _ => return ValueType::Unknown,
            }
        }
        value
    }

    fn table(
        &mut self,
        tokens: &[Token<'a>],
        lookup: &impl Fn(&str) -> ValueType,
        depth: usize,
    ) -> ValueType {
        let index = self.tables.len();
        self.tables.push(Vec::new());
        let mut next_index = 1;
        for entry in split(tokens) {
            let (key, rhs) = match entry {
                [name, equals, rhs @ ..] if identifier(name) && equals.text == "=" => {
                    (Some(Key::Text(Cow::Borrowed(name.text))), rhs)
                }
                [open, ..] if open.text == "[" => {
                    let Some(close) = group_end(entry) else {
                        continue;
                    };
                    let Some([equals, rhs @ ..]) = entry.get(close + 1..) else {
                        continue;
                    };
                    if equals.text != "=" {
                        continue;
                    }
                    (key(&entry[1..close]), rhs)
                }
                _ => {
                    let key = Key::Number(next_index);
                    next_index += 1;
                    (Some(key), entry)
                }
            };
            if let Some(key) = key {
                let value = self.expression(rhs, lookup, depth);
                self.insert(index, key, value);
            }
        }
        ValueType::Table(index)
    }

    pub(super) fn assignment(
        &self,
        tokens: &'_ [Token<'a>],
        mut value: ValueType,
    ) -> Option<(usize, Key<'a>, usize)> {
        let mut rest = tokens;
        while let ValueType::Table(index) = value {
            let (key, tail) = field(rest)?;
            if tail.first().is_some_and(|token| token.text == "=") {
                return Some((index, key, tokens.len() - tail.len()));
            }
            value = self.get(index, &key);
            rest = tail;
        }
        None
    }
}

fn field<'a, 'b>(tokens: &'b [Token<'a>]) -> Option<(Key<'a>, &'b [Token<'a>])> {
    match tokens {
        [dot, name, rest @ ..] if dot.text == "." && identifier(name) => {
            Some((Key::Text(Cow::Borrowed(name.text)), rest))
        }
        [open, ..] if open.text == "[" => {
            let close = group_end(tokens)?;
            Some((key(&tokens[1..close])?, &tokens[close + 1..]))
        }
        _ => None,
    }
}

fn key<'a>(tokens: &[Token<'a>]) -> Option<Key<'a>> {
    match tokens {
        [token] if token.kind == Kind::String => string(*token).map(Key::Text),
        [token] if token.kind == Kind::Number => integer(token.text).map(Key::Number),
        [minus, token] if minus.text == "-" && token.kind == Kind::Number => {
            integer(token.text)?.checked_neg().map(Key::Number)
        }
        _ => None,
    }
}

fn integer(text: &str) -> Option<i64> {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

fn arguments_of<'a, 'b>(tokens: &'b [Token<'a>]) -> Option<(&'b [Token<'a>], &'b [Token<'a>])> {
    let first = tokens.first()?;
    if first.kind == Kind::String {
        Some(tokens.split_at(1))
    } else if first.text == "(" {
        let end = group_end(tokens)?;
        Some((&tokens[1..end], &tokens[end + 1..]))
    } else {
        None
    }
}

pub(super) fn group_end(tokens: &[Token<'_>]) -> Option<usize> {
    let mut delimiters = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == Kind::Symbol {
            track_delimiter(&mut delimiters, token.text, false);
        }
        if delimiters.is_empty() {
            return Some(index);
        }
    }
    None
}

pub(super) fn track_delimiter(stack: &mut Vec<&'static str>, symbol: &str, types: bool) {
    match symbol {
        "(" => stack.push(")"),
        "[" => stack.push("]"),
        "{" => stack.push("}"),
        "<" if types => stack.push(">"),
        value if stack.last().is_some_and(|&closing| closing == value) => {
            stack.pop();
        }
        _ => {}
    }
}

pub(super) fn split<'a, 'b>(mut tokens: &'b [Token<'a>]) -> impl Iterator<Item = &'b [Token<'a>]> {
    std::iter::from_fn(move || {
        if tokens.is_empty() {
            return None;
        }
        let mut delimiters = Vec::new();
        let mut blocks = Vec::new();
        let end = tokens
            .iter()
            .position(|token| {
                if token.kind == Kind::Symbol {
                    if delimiters.is_empty() && blocks.is_empty() && matches!(token.text, "," | ";")
                    {
                        return true;
                    }
                    track_delimiter(&mut delimiters, token.text, false);
                } else if token.kind == Kind::Word {
                    observe_block(&mut blocks, token.text);
                }
                false
            })
            .unwrap_or(tokens.len());
        let part = &tokens[..end];
        tokens = &tokens[(end + 1).min(tokens.len())..];
        Some(part)
    })
}

pub(super) fn string(token: Token<'_>) -> Option<Cow<'_, str>> {
    if token.kind != Kind::String || !token.is_closed() {
        return None;
    }
    if token.text.starts_with('[') {
        let delimiter = token.text[1..].find('[')? + 2;
        let content = token
            .text
            .get(delimiter..token.text.len().checked_sub(delimiter)?)?;
        return Some(Cow::Borrowed(content.strip_prefix('\n').unwrap_or(content)));
    }
    let content = token.text.get(1..token.text.len().checked_sub(1)?)?;
    if !content.contains('\\') {
        return Some(Cow::Borrowed(content));
    }
    let mut bytes = content.bytes().peekable();
    let mut result = Vec::new();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            result.push(byte);
            continue;
        }
        let escaped = match bytes.next()? {
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 11,
            digit if digit.is_ascii_digit() => {
                let mut value = u16::from(digit - b'0');
                for _ in 0..2 {
                    if bytes.peek().is_some_and(u8::is_ascii_digit) {
                        value = value * 10 + u16::from(bytes.next()? - b'0');
                    }
                }
                u8::try_from(value).ok()?
            }
            byte => byte,
        };
        result.push(escaped);
    }
    String::from_utf8(result).ok().map(Cow::Owned)
}
