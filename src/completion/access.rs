use std::{borrow::Cow, collections::VecDeque, ops::Range};

use crate::lua_syntax::{is_keyword, is_word_byte, Kind, Token};

use super::{
    members as library_members, prefix_matches,
    values::{Key, ValueType},
    variables, Access, Item, Style, Suggestions, API, GLOBALS,
};

pub(super) fn members(
    source: &str,
    previous: &VecDeque<Token<'_>>,
    mut range: Range<usize>,
    prefix: &str,
) -> Option<Suggestions> {
    let operator = previous.back()?;
    let mut symbols = variables::collect(source, operator.start);
    let is_dot = operator.text == ".";
    let (items, access) = match symbols.resolve(source, operator.start) {
        ValueType::Gui(gui) => {
            let mode = if is_dot {
                Access::GuiDot
            } else {
                Access::Plain
            };
            (gui.suggestions(is_dot), mode)
        }
        ValueType::Table(table) if is_dot => (
            symbols
                .tables
                .keys(table)
                .filter_map(|key| match key {
                    Key::Text(name) if is_identifier(name) => Some(Item {
                        name: Cow::Owned(name.to_string()),
                        detail: "Table key",
                        style: Style::Value,
                    }),
                    _ => None,
                })
                .collect(),
            Access::Plain,
        ),
        ValueType::Globals if is_dot => {
            (API.iter().chain(GLOBALS).cloned().collect(), Access::Plain)
        }
        ValueType::Unknown if is_dot => {
            let mut recent = previous.iter().rev().skip(1);
            let name = recent.next().filter(|token| token.kind == Kind::Word)?.text;
            if recent
                .next()
                .is_some_and(|token| matches!(token.text, "." | ":"))
                || symbols.names.iter().any(|binding| binding.text == name)
            {
                return None;
            }
            (library_members(name).to_vec(), Access::Plain)
        }
        _ => return None,
    };
    let existing = &source[range.clone()];
    if matches!(access, Access::GuiDot) {
        range.start = operator.start;
    }
    finish(items, range, prefix, Some(existing), access)
}

pub(super) fn index(
    source: &str,
    open: Token<'_>,
    range: Range<usize>,
    prefix: &str,
    numbers_only: bool,
) -> Option<Suggestions> {
    let mut symbols = variables::collect(source, open.start);
    index_with_symbols(source, &mut symbols, open, range, prefix, numbers_only)
}

pub(super) fn index_with_symbols<'a>(
    source: &'a str,
    symbols: &mut variables::Symbols<'a>,
    open: Token<'_>,
    range: Range<usize>,
    prefix: &str,
    numbers_only: bool,
) -> Option<Suggestions> {
    let ValueType::Table(table) = symbols.resolve(source, open.start) else {
        return None;
    };
    let items = symbols
        .tables
        .keys(table)
        .filter_map(|key| match key {
            Key::Text(name) if !numbers_only => Some(Item {
                name: Cow::Owned(name.to_string()),
                detail: "Table key",
                style: Style::TableStringKey,
            }),
            Key::Number(index) => Some(Item {
                name: Cow::Owned(index.to_string()),
                detail: "Table index",
                style: Style::Value,
            }),
            _ => None,
        })
        .collect();
    finish(items, range, prefix, None, Access::Index)
}

pub(super) fn quoted_index(
    source: &str,
    caret: usize,
    token: Token<'_>,
    open: Option<Token<'_>>,
) -> Option<Suggestions> {
    let open = open.filter(|token| token.text == "[")?;
    let quote = token
        .text
        .chars()
        .next()
        .filter(|&quote| matches!(quote, '"' | '\''))?;
    if token.kind != Kind::String {
        return None;
    }
    let start = token.start + 1;
    let prefix = source.get(start..caret)?;
    let mut symbols = variables::collect(source, open.start);
    let ValueType::Table(table) = symbols.resolve(source, open.start) else {
        return None;
    };
    let items = symbols
        .tables
        .keys(table)
        .filter_map(|key| match key {
            Key::Text(name) => Some(Item {
                name: Cow::Owned(name.to_string()),
                detail: "Table key",
                style: Style::TableStringKey,
            }),
            Key::Number(_) => None,
        })
        .collect();
    finish(
        items,
        start..token.end,
        prefix,
        None,
        Access::QuotedIndex(quote),
    )
}

fn finish(
    mut items: Vec<Item>,
    range: Range<usize>,
    prefix: &str,
    existing: Option<&str>,
    access: Access,
) -> Option<Suggestions> {
    items.retain(|item| {
        prefix_matches(&item.name, prefix)
            && (existing != Some(item.name.as_ref()) || item.style == Style::Call)
    });
    items.sort_by_key(|item| item.name != prefix);
    (!items.is_empty()).then_some(Suggestions {
        range,
        items,
        expression: false,
        access,
    })
}

fn is_identifier(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        && name.bytes().all(is_word_byte)
        && !is_keyword(name)
}

pub(super) fn escape_key(name: &str, quote: char) -> String {
    let mut escaped = String::new();
    for character in name.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character == quote || character == '\\' => {
                escaped.push('\\');
                escaped.push(character);
            }
            character if character.is_ascii_control() => {
                use std::fmt::Write;
                let _ = write!(escaped, "\\{:03}", character as u32);
            }
            character => escaped.push(character),
        }
    }
    escaped
}
