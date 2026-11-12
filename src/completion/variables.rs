use std::collections::HashSet;

use crate::lua_syntax::{is_keyword, Kind, Token, Tokens};

use super::{
    expects_expression,
    gui_api::{Engine, GuiType},
    observe_block,
    values::{split, track_delimiter, Tables, ValueType},
    Block,
};

#[derive(Clone, Copy)]
pub(super) struct Name<'a> {
    pub text: &'a str,
    pub detail: &'static str,
    pub value: ValueType,
}

#[derive(Default)]
struct Scope<'a> {
    names: Vec<Name<'a>>,
    loop_names: Vec<Name<'a>>,
    delimiter_depth: usize,
}

pub(super) struct Symbols<'a> {
    pub names: Vec<Name<'a>>,
    pub tables: Tables<'a>,
}

impl<'a> Symbols<'a> {
    pub(super) fn resolve(&mut self, source: &'a str, end: usize) -> ValueType {
        let tokens: Vec<_> = Tokens::new(&source[..end])
            .filter(|token| token.kind != Kind::Comment)
            .collect();
        let Some(start) = expression_start(&tokens) else {
            return ValueType::Unknown;
        };
        self.tables.infer(&tokens[start..], &|name| {
            self.names
                .iter()
                .find(|item| item.text == name)
                .map_or_else(|| builtin(name), |item| item.value)
        })
    }
}

pub(super) fn collect(source: &str, caret: usize) -> Symbols<'_> {
    let tokens: Vec<_> = Tokens::new(&source[..caret])
        .filter(|token| token.kind != Kind::Comment)
        .collect();
    let mut blocks = Vec::new();
    let mut scopes = vec![Scope::default()];
    let mut globals = Vec::new();
    let mut delimiters = Vec::new();
    let mut tables = Tables::default();

    for (index, token) in tokens.iter().enumerate() {
        if token.kind == Kind::Symbol {
            track_delimiter(&mut delimiters, token.text, false);
        }
        if token.kind != Kind::Word {
            continue;
        }
        let following = &tokens[index + 1..];
        let previous = index.checked_sub(1).map(|before| tokens[before]);
        let old = blocks.last().copied();
        let old_depth = blocks.len();
        match token.text {
            "local"
                if following
                    .first()
                    .is_some_and(|next| next.text == "function") =>
            {
                if let Some(name) = following.get(1).filter(|next| identifier(next)) {
                    scopes.last_mut().expect("root scope").names.push(Name {
                        text: name.text,
                        detail: "Local function",
                        value: ValueType::Unknown,
                    });
                }
            }
            "local" => {
                let (mut names, rest) = binding_names(source, following, "Local variable");
                if let Some(values) = declaration_values(source, token.end, rest, caret) {
                    for (name, value) in names.iter_mut().zip(split(values)) {
                        name.value = tables.infer(value, &|name| lookup(&scopes, &globals, name));
                    }
                    scopes.last_mut().expect("root scope").names.extend(names);
                }
            }
            "function" if previous.is_none_or(|before| before.text != "local") => {
                if let [name, open, ..] = following {
                    if identifier(name) && open.text == "(" {
                        globals.push(Name {
                            text: name.text,
                            detail: "Function",
                            value: ValueType::Unknown,
                        });
                    }
                }
            }
            word if !is_keyword(word)
                && delimiters.len() == scopes.last().expect("root scope").delimiter_depth
                && at_statement_start(source, previous, token.start) =>
            {
                if let Some((table, key, consumed)) =
                    tables.assignment(following, lookup(&scopes, &globals, word))
                {
                    if let Some(values) =
                        declaration_values(source, token.end, &following[consumed..], caret)
                    {
                        let value = split(values).next().map_or(ValueType::Unknown, |value| {
                            tables.infer(value, &|name| lookup(&scopes, &globals, name))
                        });
                        tables.insert(table, key, value);
                    }
                } else {
                    let (mut names, rest) = binding_names(source, &tokens[index..], "Variable");
                    if rest.first().is_some_and(|next| next.text == "=") {
                        if let Some(values) = declaration_values(source, token.end, rest, caret) {
                            for (name, value) in names.iter_mut().zip(split(values)) {
                                name.value =
                                    tables.infer(value, &|name| lookup(&scopes, &globals, name));
                            }
                            for name in names {
                                if let Some(previous) = scopes
                                    .iter_mut()
                                    .rev()
                                    .flat_map(|scope| scope.names.iter_mut().rev())
                                    .find(|previous| previous.text == name.text)
                                {
                                    previous.value = name.value;
                                } else {
                                    globals.push(name);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        if token.text != "until" || source[token.end..caret].contains(['\n', ';']) {
            observe_block(&mut blocks, token.text);
        }
        if blocks.len() > old_depth {
            let mut scope = Scope {
                delimiter_depth: delimiters.len(),
                ..Scope::default()
            };
            match token.text {
                "function" => {
                    if let Some(open) = following.iter().position(|next| next.text == "(") {
                        scope.names = binding_names(source, &following[open + 1..], "Parameter").0;
                        if following[..open].iter().any(|next| next.text == ":") {
                            scope.names.push(Name {
                                text: "self",
                                detail: "Parameter",
                                value: ValueType::Unknown,
                            });
                        }
                    }
                }
                "for" => scope.loop_names = binding_names(source, following, "Loop variable").0,
                _ => {}
            }
            scopes.push(scope);
        } else if blocks.len() < old_depth {
            scopes.pop();
        } else if matches!(token.text, "else" | "elseif") && old == Some(Block::IfBody) {
            scopes.last_mut().expect("branch scope").names.clear();
        } else if token.text == "do" && old == Some(Block::ForHeader) {
            let scope = scopes.last_mut().expect("loop scope");
            scope.names.append(&mut scope.loop_names);
        }
    }

    let mut seen = HashSet::new();
    let names = scopes
        .into_iter()
        .rev()
        .flat_map(|scope| scope.names.into_iter().rev())
        .chain(globals.into_iter().rev())
        .filter(|name| seen.insert(name.text))
        .collect();
    Symbols { names, tables }
}

fn lookup(scopes: &[Scope<'_>], globals: &[Name<'_>], name: &str) -> ValueType {
    scopes
        .iter()
        .rev()
        .flat_map(|scope| scope.names.iter().rev())
        .chain(globals.iter().rev())
        .find(|item| item.text == name)
        .map_or_else(|| builtin(name), |item| item.value)
}

fn builtin(name: &str) -> ValueType {
    match name {
        "gui" => ValueType::Gui(GuiType::engine(Engine::Basic)),
        "defineGui" => ValueType::DefineGui,
        "_G" => ValueType::Globals,
        _ => ValueType::Unknown,
    }
}

pub(super) fn identifier(token: &Token<'_>) -> bool {
    token.kind == Kind::Word
        && !is_keyword(token.text)
        && token
            .text
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
}

fn at_statement_start(source: &str, previous: Option<Token<'_>>, start: usize) -> bool {
    previous.is_none_or(|token| {
        matches!(token.text, ";" | "then" | "do" | "else" | "repeat")
            || (!matches!(token.text, "." | ":" | "local" | "function" | "for")
                && !expects_expression(token.text)
                && source[token.end..start].contains('\n'))
    })
}

fn binding_names<'a, 'b>(
    source: &str,
    mut tokens: &'b [Token<'a>],
    detail: &'static str,
) -> (Vec<Name<'a>>, &'b [Token<'a>]) {
    let mut names = Vec::new();
    while let Some((token, rest)) = tokens.split_first().filter(|(token, _)| identifier(token)) {
        names.push(Name {
            text: token.text,
            detail,
            value: ValueType::Unknown,
        });
        tokens = rest;
        if tokens.first().is_some_and(|token| token.text == ":") {
            tokens = skip_type(source, &tokens[1..]);
        }
        if tokens.first().is_none_or(|token| token.text != ",") {
            break;
        }
        tokens = &tokens[1..];
    }
    (names, tokens)
}

fn skip_type<'a, 'b>(source: &str, tokens: &'b [Token<'a>]) -> &'b [Token<'a>] {
    let mut delimiters = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if delimiters.is_empty()
            && (matches!(token.text, "," | ")" | "=" | ";")
                || (index > 0 && source[tokens[index - 1].end..token.start].contains('\n')))
        {
            return &tokens[index..];
        }
        if token.kind == Kind::Symbol {
            track_delimiter(&mut delimiters, token.text, true);
        }
    }
    &tokens[tokens.len()..]
}

fn declaration_values<'a, 'b>(
    source: &str,
    after_local: usize,
    tokens: &'b [Token<'a>],
    caret: usize,
) -> Option<&'b [Token<'a>]> {
    let Some(first) = tokens.first() else {
        return source[after_local..caret].contains('\n').then_some(&[]);
    };
    if first.text != "=" {
        return Some(&[]);
    }
    let mut delimiters = Vec::new();
    let mut blocks = Vec::new();
    let mut previous = first;
    for (index, token) in tokens.iter().enumerate().skip(1) {
        if delimiters.is_empty()
            && blocks.is_empty()
            && (token.text == ";"
                || (!expects_expression(previous.text)
                    && !matches!(token.text, "." | ":" | "[" | "(" | ",")
                    && source[previous.end..token.start].contains('\n')))
        {
            return Some(&tokens[1..index]);
        }
        match token.kind {
            Kind::Word => observe_block(&mut blocks, token.text),
            Kind::Symbol => track_delimiter(&mut delimiters, token.text, false),
            _ => {}
        }
        previous = token;
    }
    (delimiters.is_empty()
        && blocks.is_empty()
        && !expects_expression(previous.text)
        && source[previous.end..caret].contains('\n'))
    .then_some(&tokens[1..])
}

fn expression_start(tokens: &[Token<'_>]) -> Option<usize> {
    let mut end = tokens.len();
    loop {
        let last = tokens.get(end.checked_sub(1)?)?;
        if identifier(last) {
            end -= 1;
            if end > 0 && matches!(tokens[end - 1].text, "." | ":") {
                end -= 1;
                continue;
            }
            return Some(end);
        }
        if last.kind == Kind::String && end > 1 && identifier(&tokens[end - 2]) {
            end -= 1;
            continue;
        }
        let mut groups = Vec::new();
        let mut open = None;
        for index in (0..end).rev() {
            let token = &tokens[index];
            if token.kind != Kind::Symbol {
                continue;
            }
            match token.text {
                ")" => groups.push("("),
                "]" => groups.push("["),
                "}" => groups.push("{"),
                value if groups.last().is_some_and(|&opener| opener == value) => {
                    groups.pop();
                }
                _ => {}
            }
            if groups.is_empty() {
                open = Some(index);
                break;
            }
        }
        let open = open?;
        end = open;
        if end == 0 || !(identifier(&tokens[end - 1]) || matches!(tokens[end - 1].text, ")" | "]"))
        {
            return Some(open);
        }
    }
}
