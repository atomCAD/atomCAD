use crate::expr::expr::BinOp;
use crate::expr::expr::Expr;
use crate::expr::expr::TemplatePart;
use crate::expr::expr::UnOp;
use crate::expr::lexer::{TemplateLexError, Token, TokenTemplatePart};
use crate::structure_designer::data_type::{DataType, RecordType};
use std::collections::HashSet;

// Pratt parser
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn bump(&mut self) -> Token {
        let t = self.peek().clone();
        self.pos += 1;
        t
    }

    fn parse(&mut self) -> Result<Expr, String> {
        let expr = self.parse_bp(0)?;
        match self.peek() {
            Token::Eof => Ok(expr),
            other => Err(format!("Unexpected token after expression: {:?}", other)),
        }
    }

    /// Parse the body of an array literal. The leading `[` has already been consumed.
    /// Disambiguation: if the next token is `]`, this is an empty-typed-array
    /// literal (`[]TypeExpr`); otherwise it is a non-empty element list.
    fn parse_array_literal(&mut self) -> Result<Expr, String> {
        if matches!(self.peek(), Token::RBracket) {
            self.bump(); // consume `]`
            let t = self.parse_type_expr()?;
            return Ok(Expr::EmptyArray(t));
        }

        let mut elements = Vec::new();
        loop {
            let e = self.parse_bp(0)?;
            elements.push(e);
            match self.peek() {
                Token::Comma => {
                    self.bump();
                    continue;
                }
                Token::RBracket => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(format!(
                        "Expected ',' or ']' in array literal, got {:?}",
                        other
                    ));
                }
            }
        }
        Ok(Expr::Array(elements))
    }

    /// Parse a concrete `TypeExpr` used after the empty-array marker `[]`:
    ///   TypeExpr := TypeName
    ///             | "[" TypeExpr "]"
    ///             | "{" Field ("," Field)* "}"           // anonymous record
    ///             | "{" "}"                              // empty record (top of lattice)
    /// where `TypeName` is a built-in type identifier (handled by
    /// `parse_concrete_type_name`) or — phase 7 — any other identifier, which
    /// is treated as a reference to a named record type (`Record(Named(_))`).
    /// Resolution of the named reference (and the dangling-name check) happens
    /// at the network layer via `RecordType::resolve_fields(registry)`.
    fn parse_type_expr(&mut self) -> Result<DataType, String> {
        match self.bump() {
            Token::LBracket => {
                let inner = self.parse_type_expr()?;
                match self.bump() {
                    Token::RBracket => Ok(DataType::Array(Box::new(inner))),
                    other => Err(format!("Expected ']' to close array type, got {:?}", other)),
                }
            }
            Token::LBrace => self.parse_anonymous_record_type(),
            Token::Ident(name) => {
                // Built-in concrete types take priority. `parse_concrete_type_name`
                // also enforces the documented rejection set (None, abstract
                // supertypes, function types).
                if let Some(dt) = parse_concrete_type_name(&name) {
                    return Ok(dt);
                }
                // The name failed `parse_concrete_type_name` either because it
                // is unknown or because it parsed to a rejected type (None,
                // HasAtoms, HasStructure, HasFreeLinOps, Function). Reject the
                // explicit-rejection cases here with a clear message; treat
                // everything else as a named record reference (resolved /
                // checked at the network layer).
                if matches!(
                    name.as_str(),
                    "None" | "HasAtoms" | "HasStructure" | "HasFreeLinOps"
                ) {
                    return Err(format!("'{}' is not allowed as element type", name));
                }
                Ok(DataType::Record(RecordType::Named(name)))
            }
            other => Err(format!(
                "Expected type name or '[' or '{{', got {:?}",
                other
            )),
        }
    }

    /// Parse the body of an anonymous record literal. The leading `{` has
    /// already been consumed. Empty `{}` is a valid (empty) record literal.
    /// Fields are emitted in source (authored) order; canonicalization to
    /// sorted-by-name happens later when validation builds the
    /// `RecordType::Anonymous` and when evaluation builds the
    /// `NetworkResult::Record`.
    fn parse_record_literal(&mut self) -> Result<Expr, String> {
        if matches!(self.peek(), Token::RBrace) {
            self.bump();
            return Ok(Expr::RecordLiteral(Vec::new()));
        }
        let mut fields = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        loop {
            let name = match self.bump() {
                Token::Ident(n) => n,
                other => {
                    return Err(format!(
                        "Expected field name in record literal, got {:?}",
                        other
                    ));
                }
            };
            match self.bump() {
                Token::Colon => {}
                other => {
                    return Err(format!(
                        "Expected ':' after field name '{}' in record literal, got {:?}",
                        name, other
                    ));
                }
            }
            let value_expr = self.parse_bp(0)?;
            if !seen.insert(name.clone()) {
                return Err(format!("Record literal has duplicate field '{}'", name));
            }
            fields.push((name, value_expr));
            match self.peek() {
                Token::Comma => {
                    self.bump();
                    // Reject trailing comma — same convention as array
                    // literals (`[1, 2,]` is an error).
                    if matches!(self.peek(), Token::RBrace) {
                        return Err("Trailing comma not allowed in record literal".to_string());
                    }
                    continue;
                }
                Token::RBrace => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(format!(
                        "Expected ',' or '}}' in record literal, got {:?}",
                        other
                    ));
                }
            }
        }
        Ok(Expr::RecordLiteral(fields))
    }

    /// Parse the body of an anonymous record-type expression. The leading `{`
    /// has already been consumed. Empty `{}` is the top of the record-subtype
    /// lattice — every record is assignable to it.
    fn parse_anonymous_record_type(&mut self) -> Result<DataType, String> {
        if matches!(self.peek(), Token::RBrace) {
            self.bump();
            return Ok(DataType::Record(RecordType::anonymous(Vec::new())));
        }
        let mut fields: Vec<(String, DataType)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        loop {
            let name = match self.bump() {
                Token::Ident(n) => n,
                other => {
                    return Err(format!(
                        "Expected field name in record type, got {:?}",
                        other
                    ));
                }
            };
            match self.bump() {
                Token::Colon => {}
                other => {
                    return Err(format!(
                        "Expected ':' after field name '{}' in record type, got {:?}",
                        name, other
                    ));
                }
            }
            let field_type = self.parse_type_expr()?;
            if !seen.insert(name.clone()) {
                return Err(format!("Record type has duplicate field '{}'", name));
            }
            fields.push((name, field_type));
            match self.peek() {
                Token::Comma => {
                    self.bump();
                    if matches!(self.peek(), Token::RBrace) {
                        return Err("Trailing comma not allowed in record type".to_string());
                    }
                    continue;
                }
                Token::RBrace => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(format!(
                        "Expected ',' or '}}' in record type, got {:?}",
                        other
                    ));
                }
            }
        }
        // Canonicalize on construction (sort by name) so derived `PartialEq` /
        // `Hash` work as expected; authored order is not preserved on
        // anonymous record types (named defs are the only place authored
        // order matters in the type system).
        Ok(DataType::Record(RecordType::anonymous(fields)))
    }

    // binding powers: (left_bp, right_bp)
    fn infix_binding_power(op: &Token) -> Option<(u8, u8)> {
        match op {
            Token::Dot => Some((110, 111)), // highest precedence, higher than unary (100)
            // Postfix `[` (index) at the same precedence level as member access.
            // Right-bp is unused (index has its own bracketed body) but we keep
            // the slot non-empty to participate in the Pratt loop.
            Token::LBracket => Some((110, 111)),
            Token::Caret => Some((70, 69)), // right-assoc: use lower rbp
            Token::Star | Token::Slash | Token::Percent => Some((60, 61)),
            Token::Plus | Token::Minus => Some((50, 51)),
            Token::Lt | Token::Le | Token::Gt | Token::Ge => Some((40, 41)),
            Token::EqEq | Token::Ne => Some((30, 31)),
            Token::And => Some((20, 21)),
            Token::Or => Some((10, 11)),
            _ => None,
        }
    }

    fn parse_bp(&mut self, min_bp: u8) -> Result<Expr, String> {
        // parse prefix / primary
        let mut lhs = match self.bump() {
            Token::Number(n) => {
                // Determine if this should be Int or Float based on whether it has a decimal point
                if n.fract() == 0.0 && n >= i32::MIN as f64 && n <= i32::MAX as f64 {
                    Expr::Int(n as i32)
                } else {
                    Expr::Float(n)
                }
            }
            Token::Bool(b) => Expr::Bool(b),
            Token::Ident(name) => {
                // var or call
                if let Token::LParen = self.peek() {
                    self.bump(); // consume '('
                    let mut args = vec![];
                    if let Token::RParen = self.peek() {
                        self.bump();
                    } else {
                        loop {
                            let e = self.parse_bp(0)?;
                            args.push(e);
                            match self.peek() {
                                Token::Comma => {
                                    self.bump();
                                    continue;
                                }
                                Token::RParen => {
                                    self.bump();
                                    break;
                                }
                                other => {
                                    return Err(format!(
                                        "Expected ',' or ')' in call, got {:?}",
                                        other
                                    ));
                                }
                            }
                        }
                    }
                    Expr::Call(name, args)
                } else {
                    Expr::Var(name)
                }
            }
            Token::LParen => {
                let e = self.parse_bp(0)?;
                match self.bump() {
                    Token::RParen => e,
                    other => return Err(format!("Expected ')', got {:?}", other)),
                }
            }
            Token::LBracket => self.parse_array_literal()?,
            Token::LBrace => self.parse_record_literal()?,
            Token::Template(Err(e)) => return Err(format_template_lex_error(&e)),
            Token::Template(Ok(parts)) => {
                let mut ast_parts: Vec<TemplatePart> = Vec::with_capacity(parts.len());
                for p in parts {
                    match p {
                        TokenTemplatePart::Text(s) => ast_parts.push(TemplatePart::Text(s)),
                        TokenTemplatePart::Expr(src) => {
                            let inner = parse(&src).map_err(|e| {
                                format!("in template interpolation `${{{}}}`: {}", src, e)
                            })?;
                            ast_parts.push(TemplatePart::Expr(Box::new(inner)));
                        }
                    }
                }
                Expr::Template(ast_parts)
            }
            Token::Plus => {
                // unary plus
                let rhs = self.parse_bp(100)?;
                Expr::Unary(UnOp::Pos, Box::new(rhs))
            }
            Token::Minus => {
                // unary minus
                let rhs = self.parse_bp(100)?;
                Expr::Unary(UnOp::Neg, Box::new(rhs))
            }
            Token::Not => {
                // unary not
                let rhs = self.parse_bp(100)?;
                Expr::Unary(UnOp::Not, Box::new(rhs))
            }
            Token::If => {
                // if-then-else conditional
                let condition = self.parse_bp(0)?;
                match self.bump() {
                    Token::Then => {
                        let then_expr = self.parse_bp(0)?;
                        match self.bump() {
                            Token::Else => {
                                let else_expr = self.parse_bp(0)?;
                                Expr::Conditional(
                                    Box::new(condition),
                                    Box::new(then_expr),
                                    Box::new(else_expr),
                                )
                            }
                            other => {
                                return Err(format!(
                                    "Expected 'else' after then expression, got {:?}",
                                    other
                                ));
                            }
                        }
                    }
                    other => {
                        return Err(format!(
                            "Expected 'then' after if condition, got {:?}",
                            other
                        ));
                    }
                }
            }
            other => return Err(format!("Unexpected token in prefix: {:?}", other)),
        };

        // parse infix while precedence allows
        loop {
            let op = self.peek().clone();
            if let Some((lbp, rbp)) = Self::infix_binding_power(&op) {
                if lbp < min_bp {
                    break;
                }
                // consume op
                self.bump();

                // Handle dot operator specially for member access
                if let Token::Dot = op {
                    // For member access, the RHS must be an identifier
                    match self.bump() {
                        Token::Ident(member_name) => {
                            lhs = Expr::MemberAccess(Box::new(lhs), member_name);
                            continue;
                        }
                        other => {
                            return Err(format!("Expected identifier after '.', got {:?}", other));
                        }
                    }
                } else if let Token::LBracket = op {
                    // Postfix indexing: `[` already consumed. Parse index expression
                    // then require a closing `]`. Empty `[]` and comma-separated lists
                    // are rejected to keep indexing single-argument.
                    if matches!(self.peek(), Token::RBracket) {
                        return Err("Empty index '[]' is not allowed".to_string());
                    }
                    let index = self.parse_bp(0)?;
                    match self.bump() {
                        Token::RBracket => {
                            lhs = Expr::Index(Box::new(lhs), Box::new(index));
                            continue;
                        }
                        Token::Comma => {
                            return Err("Index expression takes a single Int argument".to_string());
                        }
                        other => {
                            return Err(format!("Expected ']' to close index, got {:?}", other));
                        }
                    }
                } else {
                    // Handle normal binary operators
                    let rhs = self.parse_bp(rbp)?;
                    let binop = match op {
                        Token::Plus => BinOp::Add,
                        Token::Minus => BinOp::Sub,
                        Token::Star => BinOp::Mul,
                        Token::Slash => BinOp::Div,
                        Token::Percent => BinOp::Mod,
                        Token::Caret => BinOp::Pow,
                        Token::EqEq => BinOp::Eq,
                        Token::Ne => BinOp::Ne,
                        Token::Lt => BinOp::Lt,
                        Token::Le => BinOp::Le,
                        Token::Gt => BinOp::Gt,
                        Token::Ge => BinOp::Ge,
                        Token::And => BinOp::And,
                        Token::Or => BinOp::Or,
                        _ => unreachable!(),
                    };
                    lhs = Expr::Binary(Box::new(lhs), binop, Box::new(rhs));
                    continue;
                }
            } else {
                break;
            }
        }
        Ok(lhs)
    }
}

/// Public function to parse a string input into an expression
pub fn parse(input: &str) -> Result<Expr, String> {
    let tokens = crate::expr::lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    parser.parse()
}

fn format_template_lex_error(e: &TemplateLexError) -> String {
    match e {
        TemplateLexError::Unterminated => "unterminated template literal".to_string(),
        TemplateLexError::UnterminatedInterpolation => {
            "unterminated `${...}` in template literal".to_string()
        }
        TemplateLexError::EmptyInterpolation => "empty `${}` in template literal".to_string(),
        TemplateLexError::UnknownEscape(c) => {
            format!("unknown escape sequence `\\{}` in template literal", c)
        }
        TemplateLexError::NestedTemplateNotSupported => {
            "nested template literals are not supported inside `${...}`".to_string()
        }
    }
}

/// Maps a type-name identifier to a concrete `DataType` for use as an
/// array-literal element type after `[]`.
///
/// Reuses `DataType::from_string` so the variant table stays in one place,
/// then rejects the documented non-element types: `None`, the abstract
/// supertypes (`HasAtoms`, `HasStructure`, `HasFreeLinOps`), and `Function(_)`.
/// Returns `None` for unknown or rejected names.
pub fn parse_concrete_type_name(name: &str) -> Option<DataType> {
    let dt = DataType::from_string(name).ok()?;
    match dt {
        DataType::None | DataType::HasAtoms | DataType::HasStructure | DataType::HasFreeLinOps => {
            None
        }
        DataType::Function(_) => None,
        // `Iter[T]` is not a valid expression-language array element type:
        // expressions are eager, and no `expr` operator advances or
        // materializes a walker. See `doc/design_iterators.md` ("Out of
        // scope: Iterator support inside the `expr` expression language").
        DataType::Iterator(_) => None,
        _ => Some(dt),
    }
}
