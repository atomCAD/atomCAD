#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Neg,
    Pos,
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add, Sub, Mul, Div, Pow,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(Box<Expr>, BinOp, Box<Expr>),
    Call(String, Vec<Expr>),
}

