#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Word,
    Number,
    String,
    Comment,
    Symbol,
}

#[derive(Clone, Copy)]
pub struct Token<'a> {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub text: &'a str,
    closed: bool,
}

impl Token<'_> {
    pub fn blocks_completion(self, caret: usize) -> bool {
        matches!(self.kind, Kind::String | Kind::Comment)
            && self.start < caret
            && (caret < self.end || (caret == self.end && !self.closed))
    }
}

pub fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

pub fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub struct Tokens<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> Tokens<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }

    fn long_bracket(&self, start: usize) -> Option<(usize, bool)> {
        let bytes = self.text.as_bytes();
        if bytes.get(start) != Some(&b'[') {
            return None;
        }
        let mut open = start + 1;
        while bytes.get(open) == Some(&b'=') {
            open += 1;
        }
        if bytes.get(open) != Some(&b'[') {
            return None;
        }
        let close = format!("]{}]", "=".repeat(open - start - 1));
        Some(
            self.text[open + 1..]
                .find(&close)
                .map_or((bytes.len(), false), |index| {
                    (open + 1 + index + close.len(), true)
                }),
        )
    }

    fn run(&mut self, predicate: impl Fn(u8) -> bool) {
        while self
            .text
            .as_bytes()
            .get(self.position)
            .copied()
            .is_some_and(&predicate)
        {
            self.position += 1;
        }
    }

    fn number(&mut self, first: u8) {
        let bytes = self.text.as_bytes();
        self.run(|byte| byte.is_ascii_digit());
        if first != b'.'
            && bytes.get(self.position) == Some(&b'.')
            && bytes.get(self.position + 1) != Some(&b'.')
        {
            self.position += 1;
            self.run(|byte| byte.is_ascii_digit());
        }
        if matches!(bytes.get(self.position), Some(b'e' | b'E')) {
            let before = self.position;
            self.position += 1;
            if matches!(bytes.get(self.position), Some(b'+' | b'-')) {
                self.position += 1;
            }
            let exponent = self.position;
            self.run(|byte| byte.is_ascii_digit());
            if self.position == exponent {
                self.position = before;
            }
        }
    }
}

impl<'a> Iterator for Tokens<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        self.run(|byte| byte.is_ascii_whitespace());
        let start = self.position;
        let byte = *bytes.get(start)?;
        self.position += 1;
        let next = bytes.get(self.position).copied();
        let mut closed = true;
        let kind = match byte {
            b'-' if next == Some(b'-') => {
                self.position += 1;
                if let Some((end, complete)) = self.long_bracket(self.position) {
                    self.position = end;
                    closed = complete;
                } else {
                    self.run(|byte| !matches!(byte, b'\r' | b'\n'));
                    closed = false;
                }
                Kind::Comment
            }
            b'[' => match self.long_bracket(start) {
                Some((end, complete)) => {
                    self.position = end;
                    closed = complete;
                    Kind::String
                }
                None => Kind::Symbol,
            },
            b'\'' | b'"' => {
                closed = false;
                while let Some(&current) = bytes.get(self.position) {
                    if matches!(current, b'\r' | b'\n') {
                        break;
                    }
                    self.position += 1;
                    if current == byte {
                        closed = true;
                        break;
                    }
                    if current == b'\\' && self.position < bytes.len() {
                        self.position += 1;
                        if bytes[self.position - 1] == b'\r'
                            && bytes.get(self.position) == Some(&b'\n')
                        {
                            self.position += 1;
                        }
                    }
                }
                Kind::String
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                self.run(is_word_byte);
                Kind::Word
            }
            b'.' if next == Some(b'.') => {
                self.run(|byte| byte == b'.');
                Kind::Symbol
            }
            b'0' if matches!(next, Some(b'x' | b'X'))
                && bytes
                    .get(self.position + 1)
                    .is_some_and(u8::is_ascii_hexdigit) =>
            {
                self.position += 1;
                self.run(|byte| byte.is_ascii_hexdigit());
                Kind::Number
            }
            byte if byte.is_ascii_digit()
                || (byte == b'.' && next.is_some_and(|byte| byte.is_ascii_digit())) =>
            {
                self.number(byte);
                Kind::Number
            }
            b'=' | b'<' | b'>' | b'~' if next == Some(b'=') => {
                self.position += 1;
                Kind::Symbol
            }
            _ => {
                while !self.text.is_char_boundary(self.position) {
                    self.position += 1;
                }
                Kind::Symbol
            }
        };
        Some(Token {
            kind,
            start,
            end: self.position,
            text: &self.text[start..self.position],
            closed,
        })
    }
}
