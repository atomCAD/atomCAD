use crate::expr::expr::Expr;
use crate::expr::lexer::Token;
use crate::expr::expr::UnOp;
use crate::expr::expr::BinOp;

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

  // binding powers: (left_bp, right_bp)
  fn infix_binding_power(op: &Token) -> Option<(u8, u8)> {
      match op {
          Token::Dot => Some((110, 111)), // highest precedence, higher than unary (100)
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
          },
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
                              Token::Comma => { self.bump(); continue; }
                              Token::RParen => { self.bump(); break; }
                              other => return Err(format!("Expected ',' or ')' in call, got {:?}", other)),
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
                              Expr::Conditional(Box::new(condition), Box::new(then_expr), Box::new(else_expr))
                          }
                          other => return Err(format!("Expected 'else' after then expression, got {:?}", other)),
                      }
                  }
                  other => return Err(format!("Expected 'then' after if condition, got {:?}", other)),
              }
          }
          other => return Err(format!("Unexpected token in prefix: {:?}", other)),
      };

      // parse infix while precedence allows
      loop {
          let op = self.peek().clone();
          if let Some((lbp, rbp)) = Self::infix_binding_power(&op) {
              if lbp < min_bp { break; }
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
                      other => return Err(format!("Expected identifier after '.', got {:?}", other)),
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
