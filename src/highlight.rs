use crate::{
    completion::{self, Builtin},
    editor::color,
    lua_syntax::{is_keyword, Kind, Tokens},
};

const COMMENT: u32 = color(200, 213, 236);
const STRING: u32 = color(151, 224, 180);
const NUMBER: u32 = color(247, 196, 122);
const KEYWORD: u32 = color(186, 166, 255);
const FUNCTION: u32 = color(121, 207, 255);
const LIBRARY: u32 = color(145, 176, 250);
const VALUE: u32 = color(255, 210, 144);
const OPERATOR: u32 = color(157, 182, 215);

pub const COLORS: [u32; 8] = [
    COMMENT, STRING, NUMBER, KEYWORD, FUNCTION, LIBRARY, VALUE, OPERATOR,
];

pub fn tokens(source: &str) -> impl Iterator<Item = (usize, usize, u32)> + '_ {
    let mut library = None;
    let mut member_of = None;
    Tokens::new(source).filter_map(move |token| {
        let color = match token.kind {
            Kind::Comment => COMMENT,
            Kind::Word => {
                let builtin = match member_of.take() {
                    Some(Some(namespace)) => completion::builtin(Some(namespace), token.text),
                    Some(None) => None,
                    None => completion::builtin(None, token.text),
                };
                library = (builtin == Some(Builtin::Library)).then_some(token.text);
                if is_keyword(token.text) {
                    KEYWORD
                } else {
                    match builtin? {
                        Builtin::Function => FUNCTION,
                        Builtin::Library => LIBRARY,
                        Builtin::Value => VALUE,
                    }
                }
            }
            Kind::Symbol if matches!(token.text, "." | ":") => {
                member_of = Some(if token.text == "." || library == Some("gui") {
                    library
                } else {
                    None
                });
                library = None;
                return None;
            }
            kind => {
                library = None;
                member_of = None;
                match kind {
                    Kind::String => STRING,
                    Kind::Number => NUMBER,
                    Kind::Symbol
                        if matches!(
                            token.text,
                            "=" | "+"
                                | "-"
                                | "*"
                                | "/"
                                | "%"
                                | "^"
                                | "#"
                                | "=="
                                | "~="
                                | "<"
                                | ">"
                                | "<="
                                | ">="
                                | ".."
                                | "..."
                        ) =>
                    {
                        OPERATOR
                    }
                    _ => return None,
                }
            }
        };
        Some((token.start, token.end, color))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_keywords_libraries_and_members_in_context() {
        let words = "and break do else elseif end false for function if in local nil not or repeat return then true until while";
        assert_eq!(
            tokens(words)
                .filter(|&(_, _, color)| color == KEYWORD)
                .count(),
            words.split_whitespace().count()
        );

        let source = "math.sqrt(math.pi) + _G.string.len('math') + user.math.sqrt";
        let colors: Vec<_> = tokens(source)
            .map(|(a, b, color)| (&source[a..b], color))
            .collect();
        assert_eq!(
            colors,
            [
                ("math", LIBRARY),
                ("sqrt", FUNCTION),
                ("math", LIBRARY),
                ("pi", VALUE),
                ("+", OPERATOR),
                ("_G", LIBRARY),
                ("string", LIBRARY),
                ("len", FUNCTION),
                ("'math'", STRING),
                ("+", OPERATOR),
            ]
        );
    }
}
