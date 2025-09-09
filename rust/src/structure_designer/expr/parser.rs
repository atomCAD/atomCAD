use crate::structure_designer::expr::expr::Expr;
use crate::structure_designer::expr::lexer::Token;
use crate::structure_designer::expr::expr::UnOp;
use crate::structure_designer::expr::expr::BinOp;

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
          Token::Caret => Some((70, 69)), // right-assoc: use lower rbp
          Token::Star | Token::Slash => Some((60, 61)),
          Token::Plus | Token::Minus => Some((50, 51)),
          _ => None,
      }
  }

  fn parse_bp(&mut self, min_bp: u8) -> Result<Expr, String> {
      // parse prefix / primary
      let mut lhs = match self.bump() {
          Token::Number(n) => Expr::Number(n),
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
          other => return Err(format!("Unexpected token in prefix: {:?}", other)),
      };

      // parse infix while precedence allows
      loop {
          let op = self.peek().clone();
          if let Some((lbp, rbp)) = Self::infix_binding_power(&op) {
              if lbp < min_bp { break; }
              // consume op
              self.bump();
              let rhs = self.parse_bp(rbp)?;
              let binop = match op {
                  Token::Plus => BinOp::Add,
                  Token::Minus => BinOp::Sub,
                  Token::Star => BinOp::Mul,
                  Token::Slash => BinOp::Div,
                  Token::Caret => BinOp::Pow,
                  _ => unreachable!(),
              };
              lhs = Expr::Binary(Box::new(lhs), binop, Box::new(rhs));
              continue;
          } else {
              break;
          }
      }
      Ok(lhs)
  }
}
