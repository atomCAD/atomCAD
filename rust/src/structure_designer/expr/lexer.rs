#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    Comma,
    Eof,
}

pub struct Lexer<'a> {
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
        if c.is_some() { self.i += 1; }
        c
    }

    fn eat_while<F: Fn(char) -> bool>(&mut self, f: F) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if f(c) {
                s.push(c);
                self.i += 1;
            } else { break; }
        }
        s
    }

    fn next_token(&mut self) -> Token {
        // skip whitespace
        while let Some(c) = self.peek() {
            if c.is_whitespace() { self.i += 1; }
            else { break; }
        }

        match self.peek() {
            None => Token::Eof,
            Some(c) if c.is_ascii_digit() || (c == '.' && self.chars.get(self.i+1).map(|ch| ch.is_ascii_digit()).unwrap_or(false)) => {
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
                    Err(_) => Token::Number(0.0) // fallback; could return error
                }
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let id = self.eat_while(|ch| ch.is_ascii_alphanumeric() || ch == '_');
                Token::Ident(id)
            }
            Some('+') => { self.i += 1; Token::Plus }
            Some('-') => { self.i += 1; Token::Minus }
            Some('*') => { self.i += 1; Token::Star }
            Some('/') => { self.i += 1; Token::Slash }
            Some('^') => { self.i += 1; Token::Caret }
            Some('(') => { self.i += 1; Token::LParen }
            Some(')') => { self.i += 1; Token::RParen }
            Some(',') => { self.i += 1; Token::Comma }
            Some(other) => {
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
