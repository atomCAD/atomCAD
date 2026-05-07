#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Bool(bool),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Dot,
    Colon,
    // Comparison operators
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical operators
    And,
    Or,
    Not,
    // Conditional operators
    If,
    Then,
    Else,
    /// Whole template literal, scanned in one go from `` ` `` to matching `` ` ``.
    /// On success, carries the parsed parts; each interpolation's *raw inner
    /// source* is preserved as a string and the parser re-tokenizes and
    /// parses it on demand. On failure, carries a structured error that the
    /// parser converts to a user-facing message.
    Template(Result<Vec<TokenTemplatePart>, TemplateLexError>),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenTemplatePart {
    /// Already-decoded literal text (escapes resolved).
    Text(String),
    /// Raw inner source of one `${...}` (without the `${` `}` delimiters).
    Expr(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateLexError {
    /// Reached end of input before the closing backtick.
    Unterminated,
    /// Inside `${...}`, reached end of input before the matching `}`.
    UnterminatedInterpolation,
    /// `${}` — the interpolation has no source.
    EmptyInterpolation,
    /// `\X` where X is not one of the supported escapes.
    UnknownEscape(char),
    /// Backtick encountered inside `${...}`. Nested template literals are
    /// intentionally unsupported.
    NestedTemplateNotSupported,
}

pub struct Lexer<'a> {
    #[allow(dead_code)]
    input: &'a str,
    i: usize,
    chars: Vec<char>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        let chars: Vec<char> = input.chars().collect();
        Self { input, i: 0, chars }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).cloned()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.i += 1;
        }
        c
    }

    fn eat_while<F: Fn(char) -> bool>(&mut self, f: F) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if f(c) {
                s.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        s
    }

    /// Scan a template literal. The opening backtick has already been consumed.
    /// Walks character-by-character; on entering `${...}` delegates to
    /// `scan_interpolation_inner` which tracks brace depth so inner record
    /// literals like `${ {x: 1} }` work without prematurely terminating.
    fn scan_template_literal(&mut self) -> Token {
        let mut parts: Vec<TokenTemplatePart> = Vec::new();
        let mut text = String::new();

        loop {
            match self.bump() {
                None => return Token::Template(Err(TemplateLexError::Unterminated)),
                Some('`') => break,
                Some('\\') => {
                    let escaped = match self.bump() {
                        Some('`') => '`',
                        Some('\\') => '\\',
                        Some('$') => '$',
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some(c) => return Token::Template(Err(TemplateLexError::UnknownEscape(c))),
                        None => return Token::Template(Err(TemplateLexError::Unterminated)),
                    };
                    text.push(escaped);
                }
                Some('$') if self.peek() == Some('{') => {
                    self.bump(); // consume '{'
                    if !text.is_empty() {
                        parts.push(TokenTemplatePart::Text(std::mem::take(&mut text)));
                    }
                    match self.scan_interpolation_inner() {
                        Err(e) => return Token::Template(Err(e)),
                        Ok(s) => {
                            if s.trim().is_empty() {
                                return Token::Template(Err(TemplateLexError::EmptyInterpolation));
                            }
                            parts.push(TokenTemplatePart::Expr(s));
                        }
                    }
                }
                Some(c) => text.push(c),
            }
        }

        if !text.is_empty() {
            parts.push(TokenTemplatePart::Text(text));
        }
        Token::Template(Ok(parts))
    }

    /// Scan the inner source of a `${...}` interpolation. The opening `${`
    /// has already been consumed; this routine consumes characters up to and
    /// including the matching closing `}`. Backticks inside the body reject
    /// nested template literals at lex time.
    fn scan_interpolation_inner(&mut self) -> Result<String, TemplateLexError> {
        let mut buf = String::new();
        let mut depth: u32 = 0;
        loop {
            match self.bump() {
                None => return Err(TemplateLexError::UnterminatedInterpolation),
                Some('}') if depth == 0 => return Ok(buf),
                Some('}') => {
                    depth -= 1;
                    buf.push('}');
                }
                Some('{') => {
                    depth += 1;
                    buf.push('{');
                }
                Some('`') => return Err(TemplateLexError::NestedTemplateNotSupported),
                Some(c) => buf.push(c),
            }
        }
    }

    fn next_token(&mut self) -> Token {
        // skip whitespace
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.i += 1;
            } else {
                break;
            }
        }

        match self.peek() {
            None => Token::Eof,
            Some(c)
                if c.is_ascii_digit()
                    || (c == '.'
                        && self
                            .chars
                            .get(self.i + 1)
                            .map(|ch| ch.is_ascii_digit())
                            .unwrap_or(false)) =>
            {
                // number literal (simple)
                let mut s = String::new();
                // integer part and fraction
                s += &self.eat_while(|ch| ch.is_ascii_digit() || ch == '.');
                // optional exponent
                if let Some('e') | Some('E') = self.peek() {
                    s.push(self.bump().unwrap());
                    if let Some('+') | Some('-') = self.peek() {
                        s.push(self.bump().unwrap());
                    }
                    s += &self.eat_while(|ch| ch.is_ascii_digit());
                }
                match s.parse::<f64>() {
                    Ok(n) => Token::Number(n),
                    Err(_) => Token::Number(0.0), // fallback; could return error
                }
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let id = self.eat_while(|ch| ch.is_ascii_alphanumeric() || ch == '_');
                match id.as_str() {
                    "true" => Token::Bool(true),
                    "false" => Token::Bool(false),
                    "if" => Token::If,
                    "then" => Token::Then,
                    "else" => Token::Else,
                    _ => Token::Ident(id),
                }
            }
            Some('+') => {
                self.i += 1;
                Token::Plus
            }
            Some('-') => {
                self.i += 1;
                Token::Minus
            }
            Some('*') => {
                self.i += 1;
                Token::Star
            }
            Some('/') => {
                self.i += 1;
                Token::Slash
            }
            Some('%') => {
                self.i += 1;
                Token::Percent
            }
            Some('^') => {
                self.i += 1;
                Token::Caret
            }
            Some('(') => {
                self.i += 1;
                Token::LParen
            }
            Some(')') => {
                self.i += 1;
                Token::RParen
            }
            Some('[') => {
                self.i += 1;
                Token::LBracket
            }
            Some(']') => {
                self.i += 1;
                Token::RBracket
            }
            Some('{') => {
                self.i += 1;
                Token::LBrace
            }
            Some('}') => {
                self.i += 1;
                Token::RBrace
            }
            Some(':') => {
                self.i += 1;
                Token::Colon
            }
            Some(',') => {
                self.i += 1;
                Token::Comma
            }
            Some('.') => {
                // Check if this is part of a number (should have been handled above)
                // If we get here, it's a standalone dot for member access
                self.i += 1;
                Token::Dot
            }
            Some('=') => {
                self.i += 1;
                if let Some('=') = self.peek() {
                    self.i += 1;
                    Token::EqEq
                } else {
                    // Single '=' is not a valid token in our language
                    Token::Eof
                }
            }
            Some('!') => {
                self.i += 1;
                if let Some('=') = self.peek() {
                    self.i += 1;
                    Token::Ne
                } else {
                    Token::Not
                }
            }
            Some('<') => {
                self.i += 1;
                if let Some('=') = self.peek() {
                    self.i += 1;
                    Token::Le
                } else {
                    Token::Lt
                }
            }
            Some('>') => {
                self.i += 1;
                if let Some('=') = self.peek() {
                    self.i += 1;
                    Token::Ge
                } else {
                    Token::Gt
                }
            }
            Some('&') => {
                self.i += 1;
                if let Some('&') = self.peek() {
                    self.i += 1;
                    Token::And
                } else {
                    // Single '&' is not valid in our language
                    Token::Eof
                }
            }
            Some('|') => {
                self.i += 1;
                if let Some('|') = self.peek() {
                    self.i += 1;
                    Token::Or
                } else {
                    // Single '|' is not valid in our language
                    Token::Eof
                }
            }
            Some('`') => {
                self.i += 1; // consume opening backtick
                self.scan_template_literal()
            }
            Some(_other) => {
                self.i += 1;
                // unknown char -> skip
                Token::Eof
            }
        }
    }

    fn tokenize(mut self) -> Vec<Token> {
        let mut out = Vec::new();
        loop {
            let tok = self.next_token();
            if tok == Token::Eof {
                out.push(Token::Eof);
                break;
            } else {
                out.push(tok);
            }
        }
        out
    }
}

/// Public function to tokenize a string input
pub fn tokenize(input: &str) -> Vec<Token> {
    let lexer = Lexer::new(input);
    lexer.tokenize()
}
