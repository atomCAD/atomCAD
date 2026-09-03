use super::text_value::TextValue;
use crate::data_type::{DataType, FunctionType, RecordType};
use glam::{DVec2, DVec3, IVec2, IVec3};
use std::fmt;

// ============================================================================
// Error Types
// ============================================================================

/// Error that occurred during parsing
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl ParseError {
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Parse error at line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

// ============================================================================
// Token Types
// ============================================================================

/// Token types for the text format lexer
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Identifier(String),
    Int(i32),
    Float(f64),
    String(String),
    True,
    False,
    Equals,       // =
    Colon,        // :
    Comma,        // ,
    LeftBrace,    // {
    RightBrace,   // }
    LeftBracket,  // [
    RightBracket, // ]
    LeftParen,    // (
    RightParen,   // )
    At,           // @
    Dot,          // .
    Hash,         // #
    Dollar,       // $ (zone-input sigil)
    Caret,        // ^ (scope operator, one per level)
    Slash,        // / (path separator: `m1/x`)
    Arrow,        // -> (destination side of a wire reference)
    Output,       // output keyword
    Delete,       // delete keyword
    Description,  // description keyword
    Summary,      // summary keyword
    Newline,
    Eof,
}

/// A token with its position information
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token: Token,
    pub line: usize,
    pub column: usize,
}

// ============================================================================
// Parsed Statement Types
// ============================================================================

/// The reserved property-block item that opens a zone body. Not a lexer
/// keyword: it is an ordinary identifier, distinguished from a property only
/// by the token that follows it (`{` versus `:`), so a node type is still free
/// to have a parameter called `body` — none does today (D2).
pub const BODY_KEYWORD: &str = "body";

/// Parsed statements from the text format
#[derive(Debug, Clone)]
pub enum Statement {
    /// Node assignment: `name = type { prop: value, ..., body { ... } }`
    Assignment {
        /// The scope prefix of a path-addressed statement (`m1/x = …` gives
        /// `["m1"]`), empty for the ordinary same-scope form. Each segment
        /// names a zone-owning node, walked outward-in from the scope the
        /// statement is written in (D7).
        scope_path: Vec<String>,
        name: String,
        node_type: String,
        properties: Vec<(String, PropertyValue)>,
        /// The statements of this node's `body { … }` block, when it has one.
        /// `None` means the statement did not mention a body, which leaves an
        /// existing body untouched; `Some(vec![])` is `body { }`, which empties
        /// it (`doc/design_hof_body_text_format.md` D6).
        ///
        /// Kept out of `properties` on purpose: a body is a block of
        /// *statements*, not a property value, and holding it separately is
        /// what makes "properties are applied before the body block"
        /// structural rather than a rule the editor has to remember (D13).
        body: Option<Vec<Statement>>,
    },
    /// Output statement: `output node_name`, or `output m1/x` to re-point the
    /// zone-output wire of one body (D7).
    Output {
        /// Scope prefix, as on [`Statement::Assignment`].
        scope_path: Vec<String>,
        node_name: String,
    },
    /// Delete statement: `delete node_name`, or `delete m1/x` for one body
    /// node.
    Delete {
        /// Scope prefix, as on [`Statement::Assignment`].
        scope_path: Vec<String>,
        node_name: String,
    },
    /// Description statement: `description "text"` or `description """multi-line"""`
    Description { text: String },
    /// Summary statement: `summary "text"` - short description for CLI listings
    Summary { text: String },
    /// Comment: `# comment text`
    Comment(String),
}

/// A property value can be a literal or a reference
#[derive(Debug, Clone)]
pub enum PropertyValue {
    /// A literal value (number, string, vector, etc.)
    Literal(TextValue),
    /// A node reference: `other_node` or `other_node.pin_name`
    /// The optional second field is the output pin name for multi-output nodes.
    NodeRef(String, Option<String>),
    /// A function pin reference: `@node_name`
    FunctionRef(String),
    /// A wire reference: `source -> dest.param`, with the source side
    /// optionally qualified by an output pin name (`source.pin -> dest.param`).
    ///
    /// Wires are not stored objects — they are assembled from the incoming
    /// wires on the destination — so a wire is named from its destination
    /// slot, with the source identifying which of that slot's wires is meant.
    WireRef {
        source: String,
        source_pin: Option<String>,
        /// Number of leading `^` on the source side — the scope the source
        /// node lives in, counted outward from the scope the *wire* is in.
        /// `0` for the ordinary same-scope case.
        source_depth: usize,
        dest: String,
        dest_param: String,
    },
    /// Array of references or values: `[sphere1, box1]`
    Array(Vec<PropertyValue>),
    /// A reference into an **enclosing** scope: `^name`, `^^name.pin`,
    /// `^@name`. `depth` is the caret count and is always `>= 1` — depth 0 is
    /// spelled by [`PropertyValue::NodeRef`] / [`PropertyValue::FunctionRef`],
    /// which additionally get the lexical outward fallback (D4).
    ///
    /// Encodes a `NodeOutput` wire at `source_scope_depth = depth`.
    ScopedRef {
        depth: usize,
        name: String,
        pin_name: Option<String>,
        is_function_ref: bool,
    },
    /// An enclosing HOF's per-iteration value: `$element`, `^$acc`. `depth` is
    /// the caret count, so `$name` is `depth = 0` (this body's own owner).
    ///
    /// Encodes a `ZoneInput` wire at `source_scope_depth = depth + 1`. The
    /// `+ 1` is the base asymmetry between the two source kinds — see the
    /// reference table in `network_serializer.rs`'s module docs.
    ZoneInputRef { depth: usize, name: String },
}

// ============================================================================
// Lexer
// ============================================================================

/// Lexer for the node network text format
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    /// Tokenize the entire input
    pub fn tokenize(input: &str) -> Result<Vec<TokenInfo>, ParseError> {
        let mut lexer = Self::new(input);
        let mut tokens = Vec::new();

        loop {
            let token_info = lexer.next_token()?;
            let is_eof = token_info.token == Token::Eof;
            tokens.push(token_info);
            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn peek_ahead(&self, n: usize) -> Option<char> {
        self.input.get(self.pos + n).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
            if ch == Some('\n') {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        ch
    }

    fn skip_whitespace_except_newline(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() && ch != '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<TokenInfo, ParseError> {
        self.skip_whitespace_except_newline();

        let line = self.line;
        let column = self.column;

        match self.peek() {
            None => Ok(TokenInfo {
                token: Token::Eof,
                line,
                column,
            }),

            Some('\n') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::Newline,
                    line,
                    column,
                })
            }

            Some('#') => {
                // Comment - read to end of line
                self.advance(); // consume #
                let _comment = self.read_comment();
                Ok(TokenInfo {
                    token: Token::Hash,
                    line,
                    column,
                })
            }

            Some('=') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::Equals,
                    line,
                    column,
                })
            }

            Some(':') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::Colon,
                    line,
                    column,
                })
            }

            Some(',') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::Comma,
                    line,
                    column,
                })
            }

            Some('{') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::LeftBrace,
                    line,
                    column,
                })
            }

            Some('}') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::RightBrace,
                    line,
                    column,
                })
            }

            Some('[') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::LeftBracket,
                    line,
                    column,
                })
            }

            Some(']') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::RightBracket,
                    line,
                    column,
                })
            }

            Some('(') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::LeftParen,
                    line,
                    column,
                })
            }

            Some(')') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::RightParen,
                    line,
                    column,
                })
            }

            Some('@') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::At,
                    line,
                    column,
                })
            }

            Some('.') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::Dot,
                    line,
                    column,
                })
            }

            Some('$') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::Dollar,
                    line,
                    column,
                })
            }

            // One `^` per token, never a `^^` digraph: the parser counts them,
            // so scope depth is not capped by the lexer.
            Some('^') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::Caret,
                    line,
                    column,
                })
            }

            // Path separator (D7). `/` never occurs inside a bare identifier —
            // `read_identifier` claims only alphanumerics and `_` — so a
            // backtick-quoted name containing a slash stays one token and is
            // still addressable as `` m1/`a/b` ``.
            Some('/') => {
                self.advance();
                Ok(TokenInfo {
                    token: Token::Slash,
                    line,
                    column,
                })
            }

            Some('"') => {
                let s = self.read_string()?;
                Ok(TokenInfo {
                    token: Token::String(s),
                    line,
                    column,
                })
            }

            Some('`') => {
                // Backtick-quoted identifier: `<one or more non-backtick chars>`
                self.advance(); // consume opening backtick
                let mut content = String::new();
                loop {
                    match self.peek() {
                        None => {
                            return Err(ParseError::new(
                                "Unterminated quoted identifier",
                                line,
                                column,
                            ));
                        }
                        Some('`') => {
                            self.advance(); // consume closing backtick
                            break;
                        }
                        Some(ch) => {
                            content.push(ch);
                            self.advance();
                        }
                    }
                }
                if content.is_empty() {
                    return Err(ParseError::new("Empty quoted identifier", line, column));
                }
                Ok(TokenInfo {
                    token: Token::Identifier(content),
                    line,
                    column,
                })
            }

            // `->` must be tested before the number rule; the number rule
            // only claims a `-` that is followed by a digit, so the two
            // cannot collide, but keeping the order explicit is cheaper than
            // re-deriving that each time.
            Some('-') if self.peek_ahead(1) == Some('>') => {
                self.advance();
                self.advance();
                Ok(TokenInfo {
                    token: Token::Arrow,
                    line,
                    column,
                })
            }

            Some(ch)
                if ch.is_ascii_digit()
                    || (ch == '-' && self.peek_ahead(1).is_some_and(|c| c.is_ascii_digit())) =>
            {
                let num = self.read_number()?;
                Ok(TokenInfo {
                    token: num,
                    line,
                    column,
                })
            }

            Some(ch) if ch.is_alphabetic() || ch == '_' => {
                let ident = self.read_identifier();
                let token = match ident.as_str() {
                    "true" => Token::True,
                    "false" => Token::False,
                    "output" => Token::Output,
                    "delete" => Token::Delete,
                    "description" => Token::Description,
                    "summary" => Token::Summary,
                    _ => Token::Identifier(ident),
                };
                Ok(TokenInfo {
                    token,
                    line,
                    column,
                })
            }

            Some(ch) => Err(ParseError::new(
                format!("Unexpected character: '{}'", ch),
                line,
                column,
            )),
        }
    }

    fn read_comment(&mut self) -> String {
        let mut comment = String::new();
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            comment.push(ch);
            self.advance();
        }
        comment.trim().to_string()
    }

    fn read_identifier(&mut self) -> String {
        let mut result = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                result.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        result
    }

    fn read_number(&mut self) -> Result<Token, ParseError> {
        let line = self.line;
        let column = self.column;
        let mut s = String::new();
        let mut is_float = false;

        // Handle negative sign
        if self.peek() == Some('-') {
            s.push('-');
            self.advance();
        }

        // Read integer part
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check for decimal point
        if self.peek() == Some('.') {
            // Make sure it's followed by a digit (not a method call)
            if self.peek_ahead(1).is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                s.push('.');
                self.advance();

                // Read fractional part
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_digit() {
                        s.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
        }

        // Check for exponent
        if let Some(ch) = self.peek()
            && (ch == 'e' || ch == 'E')
        {
            is_float = true;
            s.push(ch);
            self.advance();

            // Handle exponent sign
            if let Some(sign) = self.peek()
                && (sign == '+' || sign == '-')
            {
                s.push(sign);
                self.advance();
            }

            // Read exponent digits
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    s.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
        }

        if is_float {
            s.parse::<f64>()
                .map(Token::Float)
                .map_err(|_| ParseError::new(format!("Invalid float: {}", s), line, column))
        } else {
            s.parse::<i32>()
                .map(Token::Int)
                .map_err(|_| ParseError::new(format!("Invalid integer: {}", s), line, column))
        }
    }

    fn read_string(&mut self) -> Result<String, ParseError> {
        let line = self.line;
        let column = self.column;

        self.advance(); // consume opening quote

        // Check for triple-quoted string
        if self.peek() == Some('"') && self.peek_ahead(1) == Some('"') {
            self.advance(); // consume second quote
            self.advance(); // consume third quote
            return self.read_triple_quoted_string();
        }

        // Regular single-line string
        let mut result = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => {
                    return Err(ParseError::new("Unterminated string literal", line, column));
                }
                Some('"') => {
                    self.advance(); // consume closing quote
                    break;
                }
                Some('\\') => {
                    self.advance(); // consume backslash
                    match self.peek() {
                        Some('n') => {
                            result.push('\n');
                            self.advance();
                        }
                        Some('r') => {
                            result.push('\r');
                            self.advance();
                        }
                        Some('t') => {
                            result.push('\t');
                            self.advance();
                        }
                        Some('\\') => {
                            result.push('\\');
                            self.advance();
                        }
                        Some('"') => {
                            result.push('"');
                            self.advance();
                        }
                        Some(ch) => {
                            return Err(ParseError::new(
                                format!("Invalid escape sequence: \\{}", ch),
                                self.line,
                                self.column,
                            ));
                        }
                        None => {
                            return Err(ParseError::new(
                                "Unexpected end of input in escape sequence",
                                self.line,
                                self.column,
                            ));
                        }
                    }
                }
                Some(ch) => {
                    result.push(ch);
                    self.advance();
                }
            }
        }

        Ok(result)
    }

    fn read_triple_quoted_string(&mut self) -> Result<String, ParseError> {
        let line = self.line;
        let column = self.column;
        let mut result = String::new();

        loop {
            match self.peek() {
                None => {
                    return Err(ParseError::new(
                        "Unterminated triple-quoted string",
                        line,
                        column,
                    ));
                }
                Some('"') if self.peek_ahead(1) == Some('"') && self.peek_ahead(2) == Some('"') => {
                    self.advance(); // consume first quote
                    self.advance(); // consume second quote
                    self.advance(); // consume third quote
                    break;
                }
                Some(ch) => {
                    result.push(ch);
                    self.advance();
                }
            }
        }

        Ok(result)
    }
}

// ============================================================================
// Parser
// ============================================================================

/// Parser for the node network text format
pub struct Parser {
    tokens: Vec<TokenInfo>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<TokenInfo>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parse the entire input and return a list of statements
    pub fn parse(input: &str) -> Result<Vec<Statement>, ParseError> {
        let tokens = Lexer::tokenize(input)?;
        let mut parser = Self::new(tokens);
        parser.parse_statements()
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|ti| &ti.token)
            .unwrap_or(&Token::Eof)
    }

    /// The token `n` positions ahead of the cursor. Only `n == 1` is used: the
    /// body block is decided on one token of lookahead (D2).
    fn peek_ahead(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .map(|ti| &ti.token)
            .unwrap_or(&Token::Eof)
    }

    fn current_position(&self) -> (usize, usize) {
        self.tokens
            .get(self.pos)
            .map(|ti| (ti.line, ti.column))
            .unwrap_or((0, 0))
    }

    fn bump(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<(), ParseError> {
        let (line, col) = self.current_position();
        if self.peek() == expected {
            self.bump();
            Ok(())
        } else {
            Err(ParseError::new(
                format!("Expected {:?}, found {:?}", expected, self.peek()),
                line,
                col,
            ))
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        let (line, col) = self.current_position();
        match self.peek().clone() {
            Token::Identifier(name) => {
                self.bump();
                Ok(name)
            }
            other => Err(ParseError::new(
                format!("Expected identifier, found {:?}", other),
                line,
                col,
            )),
        }
    }

    /// Parse a node path: `x`, `m1/x`, or `outer/inner/x` (D7).
    ///
    /// Returns the scope prefix and the final segment, which is the node's own
    /// name — **the prefix selects the scope, the last segment names the node
    /// in it**. The separator is `/` rather than `.` because `.` already means
    /// pin access in value position, which would make `output m1.d` genuinely
    /// ambiguous.
    fn expect_path(&mut self) -> Result<(Vec<String>, String), ParseError> {
        let mut scope_path = Vec::new();
        let mut name = self.expect_identifier()?;
        while self.peek() == &Token::Slash {
            self.bump();
            scope_path.push(name);
            name = self.expect_identifier()?;
        }
        Ok((scope_path, name))
    }

    fn skip_newlines(&mut self) {
        while self.peek() == &Token::Newline || self.peek() == &Token::Hash {
            if self.peek() == &Token::Hash {
                // Skip the entire comment line by skipping until newline or EOF
                while self.peek() != &Token::Newline && self.peek() != &Token::Eof {
                    self.bump();
                }
            }
            self.bump();
        }
    }

    /// Parse all statements of the input.
    fn parse_statements(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.parse_statement_list(false)
    }

    /// Parse a run of statements.
    ///
    /// `in_body` switches the terminator: the top level runs to `Eof`, while a
    /// `body { … }` block ends at its closing brace — and an `Eof` reached
    /// inside one is an unterminated block, not a successful parse.
    fn parse_statement_list(&mut self, in_body: bool) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();

        loop {
            self.skip_newlines();

            match self.peek() {
                Token::RightBrace if in_body => break,
                Token::Eof if in_body => {
                    let (line, col) = self.current_position();
                    return Err(ParseError::new("Unterminated `body` block", line, col));
                }
                Token::Eof => break,
                Token::Output => {
                    statements.push(self.parse_output_statement()?);
                }
                Token::Delete => {
                    statements.push(self.parse_delete_statement()?);
                }
                Token::Description => {
                    statements.push(self.parse_description_statement()?);
                }
                Token::Summary => {
                    statements.push(self.parse_summary_statement()?);
                }
                Token::Identifier(_) => {
                    statements.push(self.parse_assignment()?);
                }
                other => {
                    let (line, col) = self.current_position();
                    return Err(ParseError::new(
                        format!("Unexpected token: {:?}", other),
                        line,
                        col,
                    ));
                }
            }
        }

        Ok(statements)
    }

    /// Parse an assignment: `name = type { props, body { … } }`
    fn parse_assignment(&mut self) -> Result<Statement, ParseError> {
        let (scope_path, name) = self.expect_path()?;
        self.expect(&Token::Equals)?;
        let node_type = self.expect_identifier()?;

        let (properties, body) = if self.peek() == &Token::LeftBrace {
            self.parse_property_block()?
        } else {
            (vec![], None)
        };

        Ok(Statement::Assignment {
            scope_path,
            name,
            node_type,
            properties,
            body,
        })
    }

    /// Parse an output statement: `output node_name` or `output m1/x`
    fn parse_output_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Output)?;
        let (scope_path, node_name) = self.expect_path()?;
        Ok(Statement::Output {
            scope_path,
            node_name,
        })
    }

    /// Parse a delete statement: `delete node_name` or `delete m1/x`
    fn parse_delete_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Delete)?;
        let (scope_path, node_name) = self.expect_path()?;
        Ok(Statement::Delete {
            scope_path,
            node_name,
        })
    }

    /// Parse a description statement: `description "text"` or `description """multi-line"""`
    fn parse_description_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Description)?;
        let (line, col) = self.current_position();
        match self.peek().clone() {
            Token::String(text) => {
                self.bump();
                Ok(Statement::Description { text })
            }
            other => Err(ParseError::new(
                format!("Expected string after 'description', found {:?}", other),
                line,
                col,
            )),
        }
    }

    /// Parse a summary statement: `summary "text"`
    fn parse_summary_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Summary)?;
        let (line, col) = self.current_position();
        match self.peek().clone() {
            Token::String(text) => {
                self.bump();
                Ok(Statement::Summary { text })
            }
            other => Err(ParseError::new(
                format!("Expected string after 'summary', found {:?}", other),
                line,
                col,
            )),
        }
    }

    /// Parse a property block: `{ prop: value, ..., body { … } }`.
    ///
    /// Returns the properties and, separately, the statements of the node's
    /// `body { … }` block if it has one. The two are told apart on **one**
    /// token of lookahead: inside a property block, an identifier followed by
    /// `:` opens a property and one followed by `{` opens the body (D2). No
    /// backtracking, and the error lands on the offending token rather than
    /// after the whole block has been consumed.
    #[allow(clippy::type_complexity)]
    fn parse_property_block(
        &mut self,
    ) -> Result<(Vec<(String, PropertyValue)>, Option<Vec<Statement>>), ParseError> {
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();

        let mut properties = Vec::new();
        let mut body: Option<Vec<Statement>> = None;

        while self.peek() != &Token::RightBrace && self.peek() != &Token::Eof {
            let opens_body = matches!(self.peek(), Token::Identifier(n) if n == BODY_KEYWORD)
                && self.peek_ahead(1) == &Token::LeftBrace;

            if opens_body {
                let (line, col) = self.current_position();
                if body.is_some() {
                    return Err(ParseError::new(
                        "Duplicate `body` block: a node has exactly one body",
                        line,
                        col,
                    ));
                }
                self.bump(); // `body`
                self.expect(&Token::LeftBrace)?;
                let statements = self.parse_statement_list(true)?;
                self.expect(&Token::RightBrace)?;
                body = Some(statements);
            } else {
                let prop_name = self.expect_identifier()?;
                self.expect(&Token::Colon)?;
                let value = self.parse_property_value()?;
                properties.push((prop_name, value));
            }

            self.skip_newlines();

            // Optional comma
            if self.peek() == &Token::Comma {
                self.bump();
                self.skip_newlines();
            }
        }

        self.expect(&Token::RightBrace)?;
        Ok((properties, body))
    }

    /// Parse a property value (literal, reference, or array)
    fn parse_property_value(&mut self) -> Result<PropertyValue, ParseError> {
        match self.peek() {
            // `^…` walks up the scope chain and `$…` names an iteration
            // value; the two compose (`^$element`). One production handles
            // all of them — see `parse_scoped_reference`.
            Token::Caret | Token::Dollar => self.parse_scoped_reference(),
            Token::At => {
                // Function reference: @node_name
                self.bump();
                let name = self.expect_identifier()?;
                if self.peek() == &Token::Arrow {
                    // `@f -> apply1.f`: a wire whose source is a function
                    // pin. A wire is identified by its source *node*, so the
                    // `@` carries no extra information here and is dropped.
                    return self.parse_wire_ref_tail(name, None, 0);
                }
                Ok(PropertyValue::FunctionRef(name))
            }
            Token::LeftBracket => {
                // Array
                self.bump();
                self.skip_newlines();
                let mut elements = Vec::new();

                while self.peek() != &Token::RightBracket && self.peek() != &Token::Eof {
                    elements.push(self.parse_property_value()?);
                    self.skip_newlines();
                    if self.peek() == &Token::Comma {
                        self.bump();
                        self.skip_newlines();
                    }
                }

                self.expect(&Token::RightBracket)?;
                Ok(PropertyValue::Array(elements))
            }
            Token::LeftParen => {
                // A function type's parameter list — `() -> T`, `(A, B) -> C`
                // — opens like a vector literal, but its first element is a
                // type (or nothing) rather than a number.
                if self.starts_function_type() {
                    return self.parse_function_type();
                }
                // Vector literal: (x, y) or (x, y, z)
                let vec_value = self.parse_vector_literal()?;
                Ok(PropertyValue::Literal(vec_value))
            }
            Token::Identifier(name) => {
                // Could be a node reference or a DataType identifier
                let name = name.clone();
                self.bump();

                // Check if this looks like a DataType
                if let Ok(dt) = DataType::from_string(&name) {
                    // It's a valid DataType like "Int", "Float", "Vec3", etc.
                    // — possibly the parameter half of `A -> B`.
                    self.finish_type(dt)
                } else if (name == "Iter" || name == "Optional")
                    && self.peek() == &Token::LeftBracket
                {
                    // `Iter[T]` / `Optional[T]`: the two constructors
                    // `DataType::from_string` cannot take as a bare word. The
                    // bracketed part parses as a one-element type list, which
                    // `to_data_type` folds into an array type; unwrap it.
                    let (line, col) = self.current_position();
                    let inner = match self.parse_type_operand()? {
                        DataType::Array(inner) => *inner,
                        _ => {
                            return Err(ParseError::new(
                                format!("Expected `{name}[T]`"),
                                line,
                                col,
                            ));
                        }
                    };
                    let dt = if name == "Iter" {
                        DataType::Iterator(Box::new(inner))
                    } else {
                        DataType::Optional(Box::new(inner))
                    };
                    self.finish_type(dt)
                } else if name == "Record" && self.peek() == &Token::LeftParen {
                    // `Record(Name)`, the spelling `DataType`'s `Display` gives a
                    // named record so it cannot collide with a node reference.
                    self.bump();
                    let record_name = self.expect_identifier()?;
                    self.expect(&Token::RightParen)?;
                    self.finish_type(DataType::Record(RecordType::Named(record_name)))
                } else {
                    // Check for `.pin_name` suffix (multi-output pin reference)
                    let pin_name = if self.peek() == &Token::Dot {
                        self.bump(); // consume dot
                        Some(self.expect_identifier()?)
                    } else {
                        None
                    };
                    if self.peek() == &Token::Arrow {
                        // `source -> dest.param`: a wire reference, where the
                        // part parsed so far is its source side.
                        return self.parse_wire_ref_tail(name, pin_name, 0);
                    }
                    // It's a node reference, optionally qualified with a pin name
                    Ok(PropertyValue::NodeRef(name, pin_name))
                }
            }
            Token::Int(i) => {
                let i = *i;
                self.bump();
                Ok(PropertyValue::Literal(TextValue::Int(i)))
            }
            Token::Float(f) => {
                let f = *f;
                self.bump();
                Ok(PropertyValue::Literal(TextValue::Float(f)))
            }
            Token::True => {
                self.bump();
                Ok(PropertyValue::Literal(TextValue::Bool(true)))
            }
            Token::False => {
                self.bump();
                Ok(PropertyValue::Literal(TextValue::Bool(false)))
            }
            Token::String(s) => {
                let s = s.clone();
                self.bump();
                Ok(PropertyValue::Literal(TextValue::String(s)))
            }
            Token::LeftBrace => {
                // Nested object
                let obj = self.parse_object_literal()?;
                Ok(PropertyValue::Literal(obj))
            }
            other => {
                let (line, col) = self.current_position();
                Err(ParseError::new(
                    format!("Expected property value, found {:?}", other),
                    line,
                    col,
                ))
            }
        }
    }

    // ---- Type syntax in property position -------------------------------
    //
    // `DataType`'s `Display` is the serializer's spelling of a type-valued
    // property (`data_type`, `input_type`, `element_type`, …), and the parser
    // must read everything it can write: `A -> B`, `(A, B) -> C`, `() -> T`,
    // `[T]`, `Iter[T]`, `Optional[T]`, `Record(Name)`, and any nesting of
    // those. Until this landed only a bare builtin name parsed, so a network
    // with a function- or array-typed `parameter` could not round-trip
    // through `query` → `edit --replace`.
    //
    // `[T]` is *not* decided here: the same tokens are a one-element list of
    // types for `closure { type_args: [Crystal] }`, so the parser yields the
    // list and `TextValue::to_data_type` folds it where a type is wanted.

    /// A type has been parsed; if `->` follows, it was the single parameter
    /// of a function type whose return type comes next (right-associative,
    /// and `FunctionType::new` flattens a curried spelling).
    fn finish_type(&mut self, dt: DataType) -> Result<PropertyValue, ParseError> {
        if self.peek() == &Token::Arrow {
            self.bump();
            let output = self.parse_type_operand()?;
            let function = DataType::Function(FunctionType::new(vec![dt], output));
            return Ok(PropertyValue::Literal(TextValue::DataType(function)));
        }
        Ok(PropertyValue::Literal(TextValue::DataType(dt)))
    }

    /// Parse a property value that must denote a type.
    fn parse_type_operand(&mut self) -> Result<DataType, ParseError> {
        let (line, col) = self.current_position();
        let value = self.parse_property_value()?;
        Self::property_value_as_type(&value)
            .ok_or_else(|| ParseError::new("Expected a type", line, col))
    }

    /// The type a parsed property value denotes, if it denotes one. A
    /// bracketed type is still a parser-level [`PropertyValue::Array`] here,
    /// so the one-element fold `TextValue::to_data_type` applies to text
    /// values is repeated at this level.
    fn property_value_as_type(value: &PropertyValue) -> Option<DataType> {
        match value {
            PropertyValue::Literal(text_value) => text_value.to_data_type(),
            PropertyValue::Array(items) if items.len() == 1 => {
                Self::property_value_as_type(&items[0]).map(|t| DataType::Array(Box::new(t)))
            }
            _ => None,
        }
    }

    /// At `(`: is this a function type's parameter list rather than a vector
    /// literal? A vector literal's first element is a number; a parameter
    /// list is empty or opens with a type.
    fn starts_function_type(&self) -> bool {
        match self.peek_ahead(1) {
            Token::RightParen | Token::LeftBracket | Token::LeftBrace => true,
            Token::Identifier(name) => {
                DataType::from_string(name).is_ok()
                    || matches!(name.as_str(), "Iter" | "Optional" | "Record")
            }
            _ => false,
        }
    }

    /// `(A, B, …) -> R`, including the nullary `() -> R`.
    fn parse_function_type(&mut self) -> Result<PropertyValue, ParseError> {
        self.expect(&Token::LeftParen)?;
        let mut parameter_types = Vec::new();
        while self.peek() != &Token::RightParen {
            parameter_types.push(self.parse_type_operand()?);
            if self.peek() == &Token::Comma {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(&Token::RightParen)?;
        self.expect(&Token::Arrow)?;
        let output = self.parse_type_operand()?;
        let function = DataType::Function(FunctionType::new(parameter_types, output));
        Ok(PropertyValue::Literal(TextValue::DataType(function)))
    }

    /// Parse the destination half of a wire reference, having already
    /// consumed its source half: `-> dest.param`.
    fn parse_wire_ref_tail(
        &mut self,
        source: String,
        source_pin: Option<String>,
        source_depth: usize,
    ) -> Result<PropertyValue, ParseError> {
        self.expect(&Token::Arrow)?;
        let dest = self.expect_identifier()?;
        self.expect(&Token::Dot)?;
        let dest_param = self.expect_identifier()?;
        Ok(PropertyValue::WireRef {
            source,
            source_pin,
            source_depth,
            dest,
            dest_param,
        })
    }

    /// Parse the one sigil-led reference form: `^* [$] ident [. pin]`.
    ///
    /// The carets are counted first, then a single token decides what the
    /// scope they landed on is being asked for — `$` an iteration value, `@` a
    /// function pin, anything else a node output. That is the whole of D4's
    /// rule: **`k` carets → `NodeOutput` at depth `k`, a `$` prefix →
    /// `ZoneInput` at depth `k + 1`**; the `+ 1` is applied by the editor, not
    /// here, so the AST stays a faithful record of what was written.
    ///
    /// Only reached with a leading `^` or `$`, so a bare name never comes
    /// through here — bare names keep their lexical outward fallback.
    fn parse_scoped_reference(&mut self) -> Result<PropertyValue, ParseError> {
        let mut depth = 0usize;
        while self.peek() == &Token::Caret {
            self.bump();
            depth += 1;
        }

        if self.peek() == &Token::Dollar {
            self.bump();
            let name = self.expect_identifier()?;
            // `$name` never searches outward: promoting a per-iteration read
            // to a capture would change evaluation semantics, not just the
            // referent (D4). An outer HOF's element must be written `^$name`.
            return Ok(PropertyValue::ZoneInputRef { depth, name });
        }

        if self.peek() == &Token::At {
            self.bump();
            let name = self.expect_identifier()?;
            return Ok(PropertyValue::ScopedRef {
                depth,
                name,
                pin_name: None,
                is_function_ref: true,
            });
        }

        let name = self.expect_identifier()?;
        let pin_name = if self.peek() == &Token::Dot {
            self.bump();
            Some(self.expect_identifier()?)
        } else {
            None
        };
        if self.peek() == &Token::Arrow {
            // A comment anchor naming a wire whose source is a capture.
            return self.parse_wire_ref_tail(name, pin_name, depth);
        }
        Ok(PropertyValue::ScopedRef {
            depth,
            name,
            pin_name,
            is_function_ref: false,
        })
    }

    /// Parse a vector literal: `(x, y)` or `(x, y, z)`, or a 3x3 matrix
    /// literal `((a,b,c), (d,e,f), (g,h,i))`. Matrix literals are detected by
    /// peeking the first token after the opening paren — if it is another `(`,
    /// the literal is a matrix; otherwise it is a vector.
    fn parse_vector_literal(&mut self) -> Result<TextValue, ParseError> {
        self.expect(&Token::LeftParen)?;

        // Matrix literal: nested tuples.
        if self.peek() == &Token::LeftParen {
            return self.parse_matrix_literal_body();
        }

        let mut components: Vec<f64> = Vec::new();
        let mut all_ints = true;

        // Parse first component
        let first = self.parse_number_component()?;
        if first.1 {
            all_ints = false;
        }
        components.push(first.0);

        // Parse remaining components
        while self.peek() == &Token::Comma {
            self.bump();
            let comp = self.parse_number_component()?;
            if comp.1 {
                all_ints = false;
            }
            components.push(comp.0);
        }

        self.expect(&Token::RightParen)?;

        // Determine vector type based on component count and whether floats were used
        match components.len() {
            2 if all_ints => Ok(TextValue::IVec2(IVec2::new(
                components[0] as i32,
                components[1] as i32,
            ))),
            2 => Ok(TextValue::Vec2(DVec2::new(components[0], components[1]))),
            3 if all_ints => Ok(TextValue::IVec3(IVec3::new(
                components[0] as i32,
                components[1] as i32,
                components[2] as i32,
            ))),
            3 => Ok(TextValue::Vec3(DVec3::new(
                components[0],
                components[1],
                components[2],
            ))),
            n => {
                let (line, col) = self.current_position();
                Err(ParseError::new(
                    format!("Vector must have 2 or 3 components, found {}", n),
                    line,
                    col,
                ))
            }
        }
    }

    /// Parse the body of a matrix literal after the outer `(` has been consumed.
    /// Accepts either a 2x2 literal (`((a,b),(c,d))`) → `TextValue::IMat2`, or a
    /// 3x3 literal (`((a,b,c),...)`) → `TextValue::IMat3` if all 9 components are
    /// integers, otherwise `TextValue::Mat3`. (`IMat2` has no float partner, so a
    /// 2x2 literal always resolves to `IMat2`, truncating any float components.)
    fn parse_matrix_literal_body(&mut self) -> Result<TextValue, ParseError> {
        let (start_line, start_col) = self.current_position();

        let mut rows: Vec<Vec<f64>> = Vec::new();
        let mut all_ints = true;

        loop {
            let (row, row_all_ints) = self.parse_matrix_row()?;
            if !row_all_ints {
                all_ints = false;
            }
            rows.push(row);

            match self.peek() {
                Token::Comma => {
                    self.bump();
                }
                Token::RightParen => {
                    break;
                }
                other => {
                    let (line, col) = self.current_position();
                    return Err(ParseError::new(
                        format!("Expected ',' or ')' in matrix literal, found {:?}", other),
                        line,
                        col,
                    ));
                }
            }
        }

        self.expect(&Token::RightParen)?;

        let n = rows.len();
        // Every row must have exactly `n` components (square matrix).
        if rows.iter().any(|r| r.len() != n) {
            return Err(ParseError::new(
                "Matrix literal rows must all have the same length as the row count".to_string(),
                start_line,
                start_col,
            ));
        }

        match n {
            2 => Ok(TextValue::IMat2([
                [rows[0][0] as i32, rows[0][1] as i32],
                [rows[1][0] as i32, rows[1][1] as i32],
            ])),
            3 if all_ints => Ok(TextValue::IMat3([
                [rows[0][0] as i32, rows[0][1] as i32, rows[0][2] as i32],
                [rows[1][0] as i32, rows[1][1] as i32, rows[1][2] as i32],
                [rows[2][0] as i32, rows[2][1] as i32, rows[2][2] as i32],
            ])),
            3 => Ok(TextValue::Mat3([
                [rows[0][0], rows[0][1], rows[0][2]],
                [rows[1][0], rows[1][1], rows[1][2]],
                [rows[2][0], rows[2][1], rows[2][2]],
            ])),
            _ => Err(ParseError::new(
                format!("Matrix literal must be 2x2 or 3x3, found {} rows", n),
                start_line,
                start_col,
            )),
        }
    }

    /// Parse one `(a, b, ...)` tuple as part of a matrix literal. Returns the
    /// components and whether all were integer-typed (no fractional part).
    fn parse_matrix_row(&mut self) -> Result<(Vec<f64>, bool), ParseError> {
        self.expect(&Token::LeftParen)?;

        let mut comps: Vec<f64> = Vec::new();
        let mut all_ints = true;

        let first = self.parse_number_component()?;
        if first.1 {
            all_ints = false;
        }
        comps.push(first.0);

        while self.peek() == &Token::Comma {
            self.bump();
            let c = self.parse_number_component()?;
            if c.1 {
                all_ints = false;
            }
            comps.push(c.0);
        }

        self.expect(&Token::RightParen)?;

        Ok((comps, all_ints))
    }

    /// Parse a numeric component, returning (value, is_float)
    fn parse_number_component(&mut self) -> Result<(f64, bool), ParseError> {
        let (line, col) = self.current_position();

        // Note: Negative numbers are already handled by the lexer (e.g., -10 becomes Token::Int(-10))
        match self.peek() {
            Token::Int(i) => {
                let i = *i;
                self.bump();
                Ok((i as f64, false))
            }
            Token::Float(f) => {
                let f = *f;
                self.bump();
                Ok((f, true))
            }
            other => Err(ParseError::new(
                format!("Expected number in vector, found {:?}", other),
                line,
                col,
            )),
        }
    }

    /// Parse an object literal: `{ key: value, ... }`
    fn parse_object_literal(&mut self) -> Result<TextValue, ParseError> {
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();

        let mut entries = Vec::new();

        while self.peek() != &Token::RightBrace && self.peek() != &Token::Eof {
            let key = self.expect_identifier()?;
            self.expect(&Token::Colon)?;
            let value = self.parse_literal_value()?;
            entries.push((key, value));

            self.skip_newlines();
            if self.peek() == &Token::Comma {
                self.bump();
                self.skip_newlines();
            }
        }

        self.expect(&Token::RightBrace)?;
        Ok(TextValue::Object(entries))
    }

    /// Returns true iff `s` lexes as exactly one bare-identifier token followed by
    /// EOF, with the token text equal to `s`. Anything else — multi-token splits,
    /// keyword collisions, leading digit, embedded reserved character, leading
    /// backtick, etc. — returns false.
    pub fn lexes_as_single_bare_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        // Reject backtick directly: a `-prefixed string would lex as a quoted
        // identifier, which is not the bare form even when its content matches.
        if s.starts_with('`') {
            return false;
        }
        let tokens = match Lexer::tokenize(s) {
            Ok(t) => t,
            Err(_) => return false,
        };
        // Expect exactly two tokens: Identifier(t) followed by Eof.
        if tokens.len() != 2 {
            return false;
        }
        if !matches!(tokens[1].token, Token::Eof) {
            return false;
        }
        match &tokens[0].token {
            Token::Identifier(t) => t == s,
            _ => false,
        }
    }

    /// Returns true iff `s` must be emitted in backtick-quoted form to round-trip
    /// through the text format. This includes: empty strings (defensively),
    /// names that the lexer would split into multiple tokens, names that collide
    /// with a keyword (`true`, `false`, `output`, `delete`, `description`,
    /// `summary`), names beginning with a digit, and names containing reserved
    /// characters.
    pub fn needs_quoting(s: &str) -> bool {
        !Self::lexes_as_single_bare_identifier(s)
    }

    /// Parse a literal value (for use in object literals)
    fn parse_literal_value(&mut self) -> Result<TextValue, ParseError> {
        match self.peek() {
            Token::Int(i) => {
                let i = *i;
                self.bump();
                Ok(TextValue::Int(i))
            }
            Token::Float(f) => {
                let f = *f;
                self.bump();
                Ok(TextValue::Float(f))
            }
            Token::True => {
                self.bump();
                Ok(TextValue::Bool(true))
            }
            Token::False => {
                self.bump();
                Ok(TextValue::Bool(false))
            }
            Token::String(s) => {
                let s = s.clone();
                self.bump();
                Ok(TextValue::String(s))
            }
            // A parenthesis opens a vector literal or a function type's
            // parameter list; an identifier is a type (possibly the start of
            // `A -> B`, `Iter[T]`, `Record(Name)`) or a bare word. Both are
            // exactly what the property-value grammar already decides, so
            // defer to it: an `expr` parameter's `data_type` is written inside
            // an object literal and needs the full type syntax too.
            Token::LeftParen | Token::Identifier(_) => match self.parse_property_value()? {
                PropertyValue::Literal(value) => Ok(value),
                // A word that is not a type: kept as a string, as before.
                PropertyValue::NodeRef(name, None) => Ok(TextValue::String(name)),
                other => {
                    let (line, col) = self.current_position();
                    Err(ParseError::new(
                        format!("Expected literal value, found {:?}", other),
                        line,
                        col,
                    ))
                }
            },
            Token::LeftBracket => {
                self.bump();
                let mut elements = Vec::new();
                self.skip_newlines();

                while self.peek() != &Token::RightBracket && self.peek() != &Token::Eof {
                    elements.push(self.parse_literal_value()?);
                    self.skip_newlines();
                    if self.peek() == &Token::Comma {
                        self.bump();
                        self.skip_newlines();
                    }
                }

                self.expect(&Token::RightBracket)?;
                Ok(TextValue::Array(elements))
            }
            Token::LeftBrace => self.parse_object_literal(),
            other => {
                let (line, col) = self.current_position();
                Err(ParseError::new(
                    format!("Expected literal value, found {:?}", other),
                    line,
                    col,
                ))
            }
        }
    }
}
