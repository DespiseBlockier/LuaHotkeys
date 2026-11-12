use std::collections::{HashSet, VecDeque};

use super::{value, Access, Suggestions};
use crate::{
    lua_syntax::{Kind, Token, Tokens},
    shortcut::{modifier_flag, Key, MODIFIERS, NAMED_KEYS},
};

const ALPHANUMERIC: &[&str] = &[
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
];
const FUNCTION_KEYS: &[&str] = &[
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15",
    "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
];
const NUMBER_PAD: &[&str] = &[
    "NumPad0", "NumPad1", "NumPad2", "NumPad3", "NumPad4", "NumPad5", "NumPad6", "NumPad7",
    "NumPad8", "NumPad9",
];

pub(super) fn suggest(
    source: &str,
    caret: usize,
    token: Token<'_>,
    previous: &VecDeque<Token<'_>>,
    manual: bool,
) -> Option<Suggestions> {
    if token.kind != Kind::String
        || !token.text.starts_with(['"', '\''])
        || token.text.contains('\\')
    {
        return None;
    }
    let registering = context(source, token.start, previous)?;
    let content_end = if token.text.len() > 1 && token.text.ends_with(&token.text[..1]) {
        token.end - 1
    } else {
        token.end
    };
    let content_start = token.start + 1;
    let before = source.get(content_start..caret)?;
    let after = source.get(caret..content_end)?;
    let segment_start = before
        .rfind('+')
        .map_or(content_start, |plus| content_start + plus + 1);
    let segment_end = after.find('+').map_or(content_end, |plus| caret + plus);
    let segment = &source[segment_start..segment_end];
    let start = segment_start + segment.len() - segment.trim_start().len();
    let end = (segment_start + segment.trim_end().len()).max(start);
    let prefix = source.get(start..caret)?.trim_end();
    if !prefix.bytes().all(|byte| byte.is_ascii_alphanumeric())
        || (prefix.is_empty() && !manual && segment_start == content_start)
    {
        return None;
    }
    let mut used = 0;
    if segment_start > content_start {
        for part in source[content_start..segment_start - 1].split('+') {
            let flag = modifier_flag(part.trim())?;
            if used & flag != 0 {
                return None;
            }
            used |= flag;
        }
    }
    let needs_modifier = segment_end < content_end;
    if needs_modifier {
        // A later modifier already occupies its slot, even when editing an earlier segment.
        let tail = &source[segment_end + 1..content_end];
        if let Some((modifiers, _)) = tail.rsplit_once('+') {
            for part in modifiers.split('+') {
                used |= modifier_flag(part.trim()).unwrap_or(0);
            }
        }
    }
    let matches = |name: &str| {
        name.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
            && name != &source[start..end]
    };
    let mut items: Vec<_> = MODIFIERS
        .iter()
        .filter(|&&(name, _, flag)| used & flag == 0 && matches(name))
        .map(|&(name, _, _)| value(name, "Modifier"))
        .collect();
    if !needs_modifier {
        items.extend(
            ALPHANUMERIC
                .iter()
                .chain(FUNCTION_KEYS)
                .chain(NUMBER_PAD)
                .copied()
                .filter(|&name| matches(name) && !(registering && name == "F12"))
                .map(|name| value(name, "Key")),
        );
        items.extend(
            NAMED_KEYS
                .iter()
                .flat_map(|(_, keys)| *keys)
                .filter_map(|&(name, key)| {
                    let detail = match key {
                        Key::Mouse(_) if registering => return None,
                        Key::Mouse(_) => "Mouse button",
                        _ => "Key",
                    };
                    matches(name).then_some(value(name, detail))
                }),
        );
    }
    let mut names = HashSet::new();
    items.retain(|item| names.insert(item.name.clone()));
    (!items.is_empty()).then_some(Suggestions {
        range: start..end,
        items,
        expression: false,
        access: Access::Plain,
    })
}

fn context(source: &str, string_start: usize, previous: &VecDeque<Token<'_>>) -> Option<bool> {
    let mut recent = previous.iter().rev();
    let last = recent.next()?.text;
    let mut name = recent.next()?.text;
    if last == "=" && name == "?" {
        name = recent.next()?.text;
    }
    let before_name = recent.next().map(|token| token.text);
    if last == ","
        && before_name == Some("(")
        && recent.next().is_some_and(|token| token.text == "keybind")
        && recent.next().is_some_and(|token| token.text == ":")
    {
        return Some(false);
    }
    if last == "(" && matches!(name, "hotkey" | "press") {
        let global = match before_name {
            Some(".") => recent.next().is_some_and(|token| token.text == "_G"),
            Some(":" | "function") => false,
            _ => true,
        };
        return global.then_some(name == "hotkey");
    }
    if last == "="
        && before_name == Some(":")
        && recent.next().is_some_and(|token| token.kind == Kind::Word)
        && recent.next().is_some_and(|token| token.text == "local")
        && (name == "Combo" || is_combo_alias(&source[..string_start], name))
    {
        return Some(false);
    }
    None
}

fn is_combo_alias(source: &str, name: &str) -> bool {
    let tokens: Vec<_> = Tokens::new(source)
        .filter(|token| token.kind != Kind::Comment)
        .collect();
    let mut aliases = HashSet::from(["Combo"]);
    for (index, words) in tokens.windows(4).enumerate() {
        let [keyword, alias, equals, target] = words else {
            continue;
        };
        if keyword.text != "type" || alias.kind != Kind::Word || equals.text != "=" {
            continue;
        }
        let prefix = source[..keyword.start]
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .trim();
        if !matches!(prefix, "" | "export") {
            continue;
        }
        let simple = tokens.get(index + 4).is_none_or(|next| {
            next.text == ";"
                || (source[target.end..next.start].contains('\n')
                    && !matches!(next.text, "|" | "&" | "?" | "." | "<"))
        });
        if simple && aliases.contains(target.text) {
            aliases.insert(alias.text);
        } else {
            aliases.remove(alias.text);
        }
    }
    aliases.contains(name)
}
