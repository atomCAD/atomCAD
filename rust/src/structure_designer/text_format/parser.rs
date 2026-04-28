use super::text_value::TextValue;
use crate::structure_designer::data_type::DataType;
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

/// Parsed statements from the text format
#[derive(Debug, Clone)]
pub enum Statement {
    /// Node assignment: `name = type { prop: value, ... }`
    Assignment {
        name: String,
        node_type: String,
        properties: Vec<(String, PropertyValue)>,
    },
    /// Output statement: `output node_name`
    Output { node_name: String },
    /// Delete statement: `delete node_name`
    Delete { node_name: String },
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
    /// Array of references or values: `[sphere1, box1]`
    Array(Vec<PropertyValue>),
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
                    return Err(ParseError::new(
                        "Empty quoted identifier",
                        line,
                        column,
                    ));
                }
                Ok(TokenInfo {
                    token: Token::Identifier(content),
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
        if let Some(ch) = self.peek() {
            if ch == 'e' || ch == 'E' {
                is_float = true;
                s.push(ch);
                self.advance();

                // Handle exponent sign
                if let Some(sign) = self.peek() {
                    if sign == '+' || sign == '-' {
                        s.push(sign);
                        self.advance();
                    }
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

    /// Parse all statements
    fn parse_statements(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();

        loop {
            self.skip_newlines();

            match self.peek() {
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

    /// Parse an assignment: `name = type { props }`
    fn parse_assignment(&mut self) -> Result<Statement, ParseError> {
        let name = self.expect_identifier()?;
        self.expect(&Token::Equals)?;
        let node_type = self.expect_identifier()?;

        let properties = if self.peek() == &Token::LeftBrace {
            self.parse_property_block()?
        } else {
            vec![]
        };

        Ok(Statement::Assignment {
            name,
            node_type,
            properties,
        })
    }

    /// Parse an output statement: `output node_name`
    fn parse_output_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Output)?;
        let node_name = self.expect_identifier()?;
        Ok(Statement::Output { node_name })
    }

    /// Parse a delete statement: `delete node_name`
    fn parse_delete_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Delete)?;
        let node_name = self.expect_identifier()?;
        Ok(Statement::Delete { node_name })
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

    /// Parse a property block: `{ prop: value, ... }`
    fn parse_property_block(&mut self) -> Result<Vec<(String, PropertyValue)>, ParseError> {
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();

        let mut properties = Vec::new();

        while self.peek() != &Token::RightBrace && self.peek() != &Token::Eof {
            let prop_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;
            let value = self.parse_property_value()?;
            properties.push((prop_name, value));

            self.skip_newlines();

            // Optional comma
            if self.peek() == &Token::Comma {
                self.bump();
                self.skip_newlines();
            }
        }

        self.expect(&Token::RightBrace)?;
        Ok(properties)
    }

    /// Parse a property value (literal, reference, or array)
    fn parse_property_value(&mut self) -> Result<PropertyValue, ParseError> {
        match self.peek() {
            Token::At => {
                // Function reference: @node_name
                self.bump();
                let name = self.expect_identifier()?;
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
                    Ok(PropertyValue::Literal(TextValue::DataType(dt)))
                } else {
                    // Check for `.pin_name` suffix (multi-output pin reference)
                    let pin_name = if self.peek() == &Token::Dot {
                        self.bump(); // consume dot
                        Some(self.expect_identifier()?)
                    } else {
                        None
                    };
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
    /// Expects three 3-component sub-tuples separated by commas, then a closing `)`.
    /// Returns `TextValue::IMat3` if all 9 components are integers, otherwise `TextValue::Mat3`.
    fn parse_matrix_literal_body(&mut self) -> Result<TextValue, ParseError> {
        let (start_line, start_col) = self.current_position();

        let mut rows: Vec<[f64; 3]> = Vec::new();
        let mut all_ints = true;

        loop {
            let (row, row_all_ints) = self.parse_3_tuple_row()?;
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

        if rows.len() != 3 {
            return Err(ParseError::new(
                format!("Matrix literal must have 3 rows, found {}", rows.len()),
                start_line,
                start_col,
            ));
        }

        if all_ints {
            Ok(TextValue::IMat3([
                [rows[0][0] as i32, rows[0][1] as i32, rows[0][2] as i32],
                [rows[1][0] as i32, rows[1][1] as i32, rows[1][2] as i32],
                [rows[2][0] as i32, rows[2][1] as i32, rows[2][2] as i32],
            ]))
        } else {
            Ok(TextValue::Mat3([rows[0], rows[1], rows[2]]))
        }
    }

    /// Parse one `(a, b, c)` triple as part of a matrix literal. Returns the
    /// three components and whether all were integer-typed (no fractional part).
    fn parse_3_tuple_row(&mut self) -> Result<([f64; 3], bool), ParseError> {
        let (line, col) = self.current_position();
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

        if comps.len() != 3 {
            return Err(ParseError::new(
                format!("Matrix row must have 3 components, found {}", comps.len()),
                line,
                col,
            ));
        }

        Ok(([comps[0], comps[1], comps[2]], all_ints))
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
            Token::LeftParen => self.parse_vector_literal(),
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
            Token::Identifier(name) => {
                let name = name.clone();
                self.bump();
                // Try to parse as DataType
                if let Ok(dt) = DataType::from_string(&name) {
                    Ok(TextValue::DataType(dt))
                } else {
                    // Treat as string identifier
                    Ok(TextValue::String(name))
                }
            }
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
