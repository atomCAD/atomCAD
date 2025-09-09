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

impl Expr {
    /// Convert the expression to prefix notation string representation
    pub fn to_prefix_string(&self) -> String {
        match self {
            Expr::Number(n) => n.to_string(),
            Expr::Var(name) => name.clone(),
            Expr::Unary(op, expr) => {
                let op_str = match op {
                    UnOp::Neg => "neg",
                    UnOp::Pos => "pos",
                };
                format!("({} {})", op_str, expr.to_prefix_string())
            }
            Expr::Binary(left, op, right) => {
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Pow => "^",
                };
                format!("({} {} {})", op_str, left.to_prefix_string(), right.to_prefix_string())
            }
            Expr::Call(name, args) => {
                let args_str = args.iter()
                    .map(|arg| arg.to_prefix_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                if args.is_empty() {
                    format!("(call {})", name)
                } else {
                    format!("(call {} {})", name, args_str)
                }
            }
        }
    }
}

