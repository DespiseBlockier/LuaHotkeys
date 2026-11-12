use std::ops::Range;

use crate::lua_syntax::{Kind, Tokens};

const WIDTH: usize = 4;

pub(super) struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub selected: Range<usize>,
}

fn line_start(source: &str, position: usize) -> usize {
    source[..position].rfind('\n').map_or(0, |start| start + 1)
}

fn column(text: &str) -> usize {
    text.chars().fold(0, |column, character| {
        column
            + if character == '\t' {
                WIDTH - column % WIDTH
            } else {
                1
            }
    })
}

fn leading(text: &str) -> &str {
    &text[..text.len() - text.trim_start_matches([' ', '\t']).len()]
}

fn dedent(line: &str) -> usize {
    if line.starts_with('\t') {
        return 1;
    }
    let spaces = line.bytes().take_while(|&byte| byte == b' ').count();
    spaces.checked_sub(1).map_or(0, |index| index % WIDTH + 1)
}

pub(super) fn tab(source: &str, selection: Range<usize>, reverse: bool) -> Edit {
    let start = line_start(source, selection.start);
    if selection.is_empty() && !reverse {
        let prefix = &source[start..selection.start];
        let column = column(prefix);
        let stop = column + WIDTH - column % WIDTH;
        let scope = if prefix.trim().is_empty() {
            depth(source, selection.start).0 * WIDTH
        } else {
            0
        };
        let count = stop.max(scope) - column;
        return Edit {
            range: selection,
            text: " ".repeat(count),
            selected: count..count,
        };
    }
    let end = if !selection.is_empty() && selection.end == line_start(source, selection.end) {
        selection.end - 1
    } else {
        source[selection.end..]
            .find('\n')
            .map_or(source.len(), |end| end + selection.end)
    };
    let lines = &source[start..end];
    let text = lines
        .split('\n')
        .map(|line| {
            if reverse {
                line[dedent(line)..].to_owned()
            } else {
                format!("{}{line}", " ".repeat(WIDTH))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let selected = if selection.is_empty() {
        let caret = (selection.start - start).saturating_sub(dedent(lines));
        caret..caret
    } else {
        0..text.len()
    };
    Edit {
        range: start..end,
        text,
        selected,
    }
}

fn depth(source: &str, position: usize) -> (usize, bool) {
    let mut scopes = Vec::new();
    let level = |scopes: &[(&str, usize)]| {
        scopes
            .iter()
            .enumerate()
            .filter(|&(index, &(scope, line))| {
                scope != ")"
                    || !scopes[index + 1..]
                        .iter()
                        .any(|&(nested, nested_line)| nested == "end" && nested_line == line)
            })
            .count()
    };
    for token in Tokens::new(source).take_while(|token| token.start < position) {
        if token.blocks_completion(position)
            && (token.text.starts_with('[') || token.text.starts_with("--["))
        {
            return (level(&scopes), true);
        }
        if token.end > position || matches!(token.kind, Kind::String | Kind::Comment) {
            continue;
        }
        let closing = match token.text {
            "function" | "then" | "do" => {
                scopes.push(("end", line_start(source, token.start)));
                continue;
            }
            "repeat" => {
                scopes.push(("until", line_start(source, token.start)));
                continue;
            }
            "(" => {
                scopes.push((")", line_start(source, token.start)));
                continue;
            }
            "[" => {
                scopes.push(("]", line_start(source, token.start)));
                continue;
            }
            "{" => {
                scopes.push(("}", line_start(source, token.start)));
                continue;
            }
            "else" | "elseif" => "end",
            word => word,
        };
        if scopes.last().is_some_and(|&(scope, _)| scope == closing) {
            scopes.pop();
        }
        if token.text == "else" {
            scopes.push(("end", line_start(source, token.start)));
        }
    }
    (level(&scopes), false)
}

pub(super) fn enter(source: &str, selection: Range<usize>) -> Edit {
    let start = line_start(source, selection.start);
    if selection.is_empty() {
        if let Some(edit) = finish_function(source, start, selection.start) {
            return edit;
        }
    }
    let (level, literal) = depth(source, selection.start);
    let indent = if literal {
        leading(&source[start..selection.start]).to_owned()
    } else {
        " ".repeat(level * WIDTH)
    };
    let mut text = format!("\n{indent}");
    let caret = text.len();
    let mut range = selection;
    let tail = &source[range.end..];
    let spaces = leading(tail);
    let closing = Tokens::new(&tail[spaces.len()..])
        .next()
        .is_some_and(|token| {
            token.start == 0
                && matches!(
                    token.text,
                    "end" | "else" | "elseif" | "until" | "}" | ")" | "]"
                )
        });
    if !literal && level > 0 && closing && !source[start..range.start].trim().is_empty() {
        range.end += spaces.len();
        text.push('\n');
        text.push_str(&" ".repeat((level - 1) * WIDTH));
    }
    Edit {
        range,
        text,
        selected: caret..caret,
    }
}

fn finish_function(source: &str, line: usize, caret: usize) -> Option<Edit> {
    let mut tokens = Tokens::new(source)
        .take_while(|token| token.start < caret)
        .filter(|token| token.start >= line && token.kind != Kind::Comment)
        .collect::<Vec<_>>();
    let function = tokens.iter().rposition(|token| token.text == "function")?;
    let mut signature = tokens.drain(function + 1..);
    let opener =
        signature.find(|token| token.kind != Kind::Word && !matches!(token.text, "." | ":"))?;
    if opener.text != "(" {
        return None;
    }
    let mut parentheses = 1;
    let mut header = None;
    for token in signature {
        match token.text {
            "(" => parentheses += 1,
            ")" => {
                parentheses -= 1;
                if parentheses == 0 {
                    header = Some(token.end);
                    break;
                }
            }
            _ => {}
        }
    }
    let header = header?;
    let before = &source[header..caret];
    let closing = |ch: char| matches!(ch, ')' | ']' | '}');
    if !before
        .chars()
        .all(|ch| ch.is_ascii_whitespace() || closing(ch))
    {
        return None;
    }
    let mut end = caret;
    let mut closers: String = before.chars().filter(|&ch| closing(ch)).collect();
    for (offset, ch) in source[caret..].char_indices() {
        if closing(ch) {
            closers.push(ch);
            end = caret + offset + ch.len_utf8();
        } else if !matches!(ch, ' ' | '\t') {
            break;
        }
    }
    let (level, literal) = depth(source, header);
    let closing_indent = level.checked_sub(1)? * WIDTH;
    if literal || existing_end(source, end, closing_indent) {
        return None;
    }
    let indent = " ".repeat(level * WIDTH);
    let body = 1 + indent.len();
    Some(Edit {
        range: header..end,
        text: format!("\n{indent}\n{}end{closers}", " ".repeat(closing_indent)),
        selected: body..body,
    })
}

fn existing_end(source: &str, start: usize, indent: usize) -> bool {
    let mut level = 0usize;
    let mut branch = false;
    for token in Tokens::new(&source[start..]) {
        if matches!(token.kind, Kind::String | Kind::Comment) {
            continue;
        }
        match token.text {
            "function" | "do" | "repeat" => level += 1,
            "elseif" => branch = true,
            "then" if std::mem::take(&mut branch) => {}
            "then" => level += 1,
            "end" if level == 0 => {
                let position = start + token.start;
                let line = line_start(source, position);
                return line == line_start(source, start)
                    || column(leading(&source[line..position])) >= indent;
            }
            "end" | "until" => level = level.saturating_sub(1),
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_lua_scopes_and_tab_stops() {
        let source = "if ready then\n    local callback = function()end";
        let caret = source.rfind("end").unwrap();
        let edit = enter(source, caret..caret);
        assert_eq!(edit.text, "\n        \n    ");
        assert_eq!(edit.selected, 9..9);
        let source = "if ready then\n    log('end')\nelse";
        let edit = enter(source, source.len()..source.len());
        assert_eq!(edit.text, "\n    ");
        let source = "if ready then\n    function run()\n";
        assert_eq!(
            tab(source, source.len()..source.len(), false).text,
            "        "
        );
        assert_eq!(tab("      x", 6..6, true).text, "    x");
        assert_eq!(tab("a\nb", 0..3, false).text, "    a\n    b");
        let source = "hotkey('J', function()";
        assert_eq!(
            enter(source, source.len()..source.len()).text,
            "\n    \nend"
        );
    }

    #[test]
    fn finishes_functions_and_moves_enclosing_delimiters() {
        for (source, caret) in [
            ("hotkey(combo, function())", 24),
            ("hotkey(combo, function())", 25),
        ] {
            let edit = enter(source, caret..caret);
            let result = format!(
                "{}{}{}",
                &source[..edit.range.start],
                edit.text,
                &source[edit.range.end..]
            );
            assert_eq!(result, "hotkey(combo, function()\n    \nend)");
            assert_eq!(edit.selected, 5..5);
        }
        let source = "if ready then\n    local function work(arg)\nend";
        let caret = source.find("\nend").unwrap();
        assert_eq!(enter(source, caret..caret).text, "\n        \n    end");
        let source = "function work()\n    return 1\nend";
        let caret = source.find('\n').unwrap();
        assert_eq!(enter(source, caret..caret).text, "\n    ");
    }
}
