use std::ops::Range;

use crate::lua_syntax::{Kind, Tokens};

use super::indent::Edit;

pub(super) fn typed(source: &str, selection: Range<usize>, key: char) -> Option<Edit> {
    let closing = match key {
        '"' | '\'' => key,
        '(' => ')',
        '[' => ']',
        '{' => '}',
        ')' | ']' | '}' => key,
        _ => return None,
    };
    let start = selection.start;
    let literal = Tokens::new(source)
        .take_while(|token| token.start < start)
        .find(|token| token.blocks_completion(start));
    let after = &source[selection.end..];
    if selection.is_empty() && after.starts_with(key) {
        let can_skip = match literal {
            None => matches!(key, ')' | ']' | '}'),
            Some(token) => {
                token.kind == Kind::String
                    && token.text.starts_with(key)
                    && token.end == start + 1
                    && source[..start]
                        .bytes()
                        .rev()
                        .take_while(|&byte| byte == b'\\')
                        .count()
                        % 2
                        == 0
            }
        };
        if can_skip {
            return Some(Edit {
                range: start..start + 1,
                text: key.to_string(),
                selected: 1..1,
            });
        }
    }
    if literal.is_some() || matches!(key, ')' | ']' | '}') {
        return None;
    }
    if selection.is_empty()
        && after
            .chars()
            .next()
            .is_some_and(|next| next.is_alphanumeric() || next == '_')
    {
        return None;
    }
    let selected = &source[selection.clone()];
    Some(Edit {
        range: selection,
        text: format!("{key}{selected}{closing}"),
        selected: 1..1 + selected.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_code_wraps_selection_and_skips_closers() {
        let edit = typed("call", 4..4, '(').unwrap();
        assert_eq!(edit.text, "()");
        assert_eq!(edit.selected, 1..1);
        let edit = typed("é🚀", 0.."é🚀".len(), '"').unwrap();
        assert_eq!(edit.text, "\"é🚀\"");
        assert_eq!(&edit.text[edit.selected], "é🚀");
        assert_eq!(typed("\"\"", 1..1, '"').unwrap().selected, 1..1);
        assert_eq!(typed("{}", 1..1, '}').unwrap().range, 1..2);
        assert!(typed("-- comment", 10..10, '(').is_none());
        assert!(typed("\"string\"", 4..4, '[').is_none());
    }
}
