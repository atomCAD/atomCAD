use rust_lib_flutter_cad::structure_designer::expr::lexer::{tokenize, Token};
use rust_lib_flutter_cad::structure_designer::expr::parser::parse;

mod lexer_tests {
    use super::*;

    #[test]
    fn test_tokenize_numbers() {
        let tokens = tokenize("42");
        assert_eq!(tokens, vec![Token::Number(42.0), Token::Eof]);

        let tokens = tokenize("3.14");
        assert_eq!(tokens, vec![Token::Number(3.14), Token::Eof]);

        let tokens = tokenize("1.5e10");
        assert_eq!(tokens, vec![Token::Number(1.5e10), Token::Eof]);

        let tokens = tokenize("2.5E-3");
        assert_eq!(tokens, vec![Token::Number(2.5e-3), Token::Eof]);

        let tokens = tokenize(".5");
        assert_eq!(tokens, vec![Token::Number(0.5), Token::Eof]);
    }

    #[test]
    fn test_tokenize_identifiers() {
        let tokens = tokenize("x");
        assert_eq!(tokens, vec![Token::Ident("x".to_string()), Token::Eof]);

        let tokens = tokenize("variable_name");
        assert_eq!(tokens, vec![Token::Ident("variable_name".to_string()), Token::Eof]);

        let tokens = tokenize("func123");
        assert_eq!(tokens, vec![Token::Ident("func123".to_string()), Token::Eof]);

        let tokens = tokenize("_private");
        assert_eq!(tokens, vec![Token::Ident("_private".to_string()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_operators() {
        let tokens = tokenize("+ - * / ^");
        assert_eq!(tokens, vec![
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::Caret,
            Token::Eof
        ]);
    }

    #[test]
    fn test_tokenize_parentheses_and_comma() {
        let tokens = tokenize("( ) ,");
        assert_eq!(tokens, vec![
            Token::LParen,
            Token::RParen,
            Token::Comma,
            Token::Eof
        ]);
    }

    #[test]
    fn test_tokenize_whitespace_handling() {
        let tokens = tokenize("  x  +  y  ");
        assert_eq!(tokens, vec![
            Token::Ident("x".to_string()),
            Token::Plus,
            Token::Ident("y".to_string()),
            Token::Eof
        ]);
    }

    #[test]
    fn test_tokenize_complex_expression() {
        let tokens = tokenize("2 * x + sin(3.14)");
        assert_eq!(tokens, vec![
            Token::Number(2.0),
            Token::Star,
            Token::Ident("x".to_string()),
            Token::Plus,
            Token::Ident("sin".to_string()),
            Token::LParen,
            Token::Number(3.14),
            Token::RParen,
            Token::Eof
        ]);
    }

    #[test]
    fn test_tokenize_unknown_characters() {
        // Unknown characters should be skipped and result in EOF
        let tokens = tokenize("@#$");
        assert_eq!(tokens, vec![Token::Eof]);
    }

    #[test]
    fn test_tokenize_empty_string() {
        let tokens = tokenize("");
        assert_eq!(tokens, vec![Token::Eof]);
    }

    #[test]
    fn test_tokenize_boolean_literals() {
        let tokens = tokenize("true");
        assert_eq!(tokens, vec![Token::Bool(true), Token::Eof]);

        let tokens = tokenize("false");
        assert_eq!(tokens, vec![Token::Bool(false), Token::Eof]);
    }

    #[test]
    fn test_tokenize_comparison_operators() {
        let tokens = tokenize("== != < <= > >=");
        assert_eq!(tokens, vec![
            Token::EqEq,
            Token::Ne,
            Token::Lt,
            Token::Le,
            Token::Gt,
            Token::Ge,
            Token::Eof
        ]);
    }

    #[test]
    fn test_tokenize_logical_operators() {
        let tokens = tokenize("&& || !");
        assert_eq!(tokens, vec![
            Token::And,
            Token::Or,
            Token::Not,
            Token::Eof
        ]);
    }

    #[test]
    fn test_tokenize_boolean_expressions() {
        let tokens = tokenize("x == 5 && !flag");
        assert_eq!(tokens, vec![
            Token::Ident("x".to_string()),
            Token::EqEq,
            Token::Number(5.0),
            Token::And,
            Token::Not,
            Token::Ident("flag".to_string()),
            Token::Eof
        ]);

        let tokens = tokenize("a <= b || c != d");
        assert_eq!(tokens, vec![
            Token::Ident("a".to_string()),
            Token::Le,
            Token::Ident("b".to_string()),
            Token::Or,
            Token::Ident("c".to_string()),
            Token::Ne,
            Token::Ident("d".to_string()),
            Token::Eof
        ]);
    }

    #[test]
    fn test_tokenize_conditional_keywords() {
        let tokens = tokenize("if then else");
        assert_eq!(tokens, vec![
            Token::If,
            Token::Then,
            Token::Else,
            Token::Eof
        ]);
    }

    #[test]
    fn test_tokenize_conditional_expression() {
        let tokens = tokenize("if x > 0 then 1 else -1");
        assert_eq!(tokens, vec![
            Token::If,
            Token::Ident("x".to_string()),
            Token::Gt,
            Token::Number(0.0),
            Token::Then,
            Token::Number(1.0),
            Token::Else,
            Token::Minus,
            Token::Number(1.0),
            Token::Eof
        ]);
    }
}

mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_number() {
        let expr = parse("42").unwrap();
        assert_eq!(expr.to_prefix_string(), "42");

        let expr = parse("3.14").unwrap();
        assert_eq!(expr.to_prefix_string(), "3.14");
    }

    #[test]
    fn test_parse_variable() {
        let expr = parse("x").unwrap();
        assert_eq!(expr.to_prefix_string(), "x");

        let expr = parse("variable_name").unwrap();
        assert_eq!(expr.to_prefix_string(), "variable_name");
    }

    #[test]
    fn test_parse_unary_operators() {
        let expr = parse("-x").unwrap();
        assert_eq!(expr.to_prefix_string(), "(neg x)");

        let expr = parse("+42").unwrap();
        assert_eq!(expr.to_prefix_string(), "(pos 42)");

        let expr = parse("--x").unwrap();
        assert_eq!(expr.to_prefix_string(), "(neg (neg x))");
    }

    #[test]
    fn test_parse_binary_operators() {
        let expr = parse("x + y").unwrap();
        assert_eq!(expr.to_prefix_string(), "(+ x y)");

        let expr = parse("a - b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(- a b)");

        let expr = parse("x * y").unwrap();
        assert_eq!(expr.to_prefix_string(), "(* x y)");

        let expr = parse("a / b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(/ a b)");

        let expr = parse("x ^ y").unwrap();
        assert_eq!(expr.to_prefix_string(), "(^ x y)");
    }

    #[test]
    fn test_parse_operator_precedence() {
        // Multiplication has higher precedence than addition
        let expr = parse("x + y * z").unwrap();
        assert_eq!(expr.to_prefix_string(), "(+ x (* y z))");

        // Division has higher precedence than subtraction
        let expr = parse("a - b / c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(- a (/ b c))");

        // Power has higher precedence than multiplication
        let expr = parse("x * y ^ z").unwrap();
        assert_eq!(expr.to_prefix_string(), "(* x (^ y z))");

        // Complex precedence
        let expr = parse("a + b * c ^ d").unwrap();
        assert_eq!(expr.to_prefix_string(), "(+ a (* b (^ c d)))");
    }

    #[test]
    fn test_parse_associativity() {
        // Left associative operators
        let expr = parse("a + b + c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(+ (+ a b) c)");

        let expr = parse("a - b - c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(- (- a b) c)");

        let expr = parse("a * b * c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(* (* a b) c)");

        let expr = parse("a / b / c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(/ (/ a b) c)");

        // Right associative power operator
        let expr = parse("a ^ b ^ c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(^ a (^ b c))");
    }

    #[test]
    fn test_parse_parentheses() {
        let expr = parse("(x + y) * z").unwrap();
        assert_eq!(expr.to_prefix_string(), "(* (+ x y) z)");

        let expr = parse("x * (y + z)").unwrap();
        assert_eq!(expr.to_prefix_string(), "(* x (+ y z))");

        let expr = parse("((x))").unwrap();
        assert_eq!(expr.to_prefix_string(), "x");
    }

    #[test]
    fn test_parse_function_calls() {
        let expr = parse("sin(x)").unwrap();
        assert_eq!(expr.to_prefix_string(), "(call sin x)");

        let expr = parse("max(a, b)").unwrap();
        assert_eq!(expr.to_prefix_string(), "(call max a b)");

        let expr = parse("func(x, y, z)").unwrap();
        assert_eq!(expr.to_prefix_string(), "(call func x y z)");

        let expr = parse("empty()").unwrap();
        assert_eq!(expr.to_prefix_string(), "(call empty)");
    }

    #[test]
    fn test_parse_complex_expressions() {
        let expr = parse("2 * x + sin(3.14 * y)").unwrap();
        assert_eq!(expr.to_prefix_string(), "(+ (* 2 x) (call sin (* 3.14 y)))");

        let expr = parse("-sin(x) + cos(y) ^ 2").unwrap();
        assert_eq!(expr.to_prefix_string(), "(+ (neg (call sin x)) (^ (call cos y) 2))");

        let expr = parse("(a + b) * (c - d) / (e ^ f)").unwrap();
        assert_eq!(expr.to_prefix_string(), "(/ (* (+ a b) (- c d)) (^ e f))");
    }

    #[test]
    fn test_parse_nested_function_calls() {
        let expr = parse("sin(cos(x))").unwrap();
        assert_eq!(expr.to_prefix_string(), "(call sin (call cos x))");

        let expr = parse("max(min(a, b), c)").unwrap();
        assert_eq!(expr.to_prefix_string(), "(call max (call min a b) c)");
    }

    #[test]
    fn test_parse_errors() {
        // Mismatched parentheses
        assert!(parse("(x + y").is_err());
        assert!(parse("x + y)").is_err());

        // Invalid function call syntax
        assert!(parse("sin(x,)").is_err());
        assert!(parse("sin(,x)").is_err());

        // Unexpected tokens - binary operators at start should fail
        assert!(parse("* x").is_err());
        assert!(parse("/ x").is_err());
        assert!(parse("^ x").is_err());
        
        // Note: "x + + y" is actually valid as "x + (+y)", so we test other invalid cases
        assert!(parse("x + * y").is_err());
        assert!(parse("x * / y").is_err());

        // Empty expression
        assert!(parse("").is_err());

        // Trailing tokens
        assert!(parse("x + y z").is_err());
    }

    #[test]
    fn test_parse_whitespace_handling() {
        let expr1 = parse("x+y").unwrap();
        let expr2 = parse("  x  +  y  ").unwrap();
        assert_eq!(expr1.to_prefix_string(), expr2.to_prefix_string());
    }

    #[test]
    fn test_parse_boolean_literals() {
        let expr = parse("true").unwrap();
        assert_eq!(expr.to_prefix_string(), "true");

        let expr = parse("false").unwrap();
        assert_eq!(expr.to_prefix_string(), "false");
    }

    #[test]
    fn test_parse_comparison_operators() {
        let expr = parse("x == y").unwrap();
        assert_eq!(expr.to_prefix_string(), "(== x y)");

        let expr = parse("a != b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(!= a b)");

        let expr = parse("x < y").unwrap();
        assert_eq!(expr.to_prefix_string(), "(< x y)");

        let expr = parse("a <= b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(<= a b)");

        let expr = parse("x > y").unwrap();
        assert_eq!(expr.to_prefix_string(), "(> x y)");

        let expr = parse("a >= b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(>= a b)");
    }

    #[test]
    fn test_parse_logical_operators() {
        let expr = parse("x && y").unwrap();
        assert_eq!(expr.to_prefix_string(), "(&& x y)");

        let expr = parse("a || b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(|| a b)");

        let expr = parse("!x").unwrap();
        assert_eq!(expr.to_prefix_string(), "(not x)");

        let expr = parse("!!x").unwrap();
        assert_eq!(expr.to_prefix_string(), "(not (not x))");
    }

    #[test]
    fn test_parse_boolean_precedence() {
        // Comparison has higher precedence than logical AND
        let expr = parse("x < 5 && y > 3").unwrap();
        assert_eq!(expr.to_prefix_string(), "(&& (< x 5) (> y 3))");

        // Logical AND has higher precedence than logical OR
        let expr = parse("a || b && c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(|| a (&& b c))");

        // Arithmetic has higher precedence than comparison
        let expr = parse("x + 1 > y * 2").unwrap();
        assert_eq!(expr.to_prefix_string(), "(> (+ x 1) (* y 2))");

        // Equality has lower precedence than comparison
        let expr = parse("x < y == a > b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(== (< x y) (> a b))");
    }

    #[test]
    fn test_parse_boolean_associativity() {
        // Logical operators are left associative
        let expr = parse("a && b && c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(&& (&& a b) c)");

        let expr = parse("a || b || c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(|| (|| a b) c)");

        // Comparison operators are left associative
        let expr = parse("a < b < c").unwrap();
        assert_eq!(expr.to_prefix_string(), "(< (< a b) c)");
    }

    #[test]
    fn test_parse_complex_boolean_expressions() {
        let expr = parse("!flag && (x > 0 || y < 10)").unwrap();
        assert_eq!(expr.to_prefix_string(), "(&& (not flag) (|| (> x 0) (< y 10)))");

        let expr = parse("a == b && c != d || e >= f").unwrap();
        assert_eq!(expr.to_prefix_string(), "(|| (&& (== a b) (!= c d)) (>= e f))");

        let expr = parse("x + y > z && !done").unwrap();
        assert_eq!(expr.to_prefix_string(), "(&& (> (+ x y) z) (not done))");
    }

    #[test]
    fn test_parse_mixed_arithmetic_boolean() {
        let expr = parse("2 * x + 1 == y ^ 2").unwrap();
        assert_eq!(expr.to_prefix_string(), "(== (+ (* 2 x) 1) (^ y 2))");

        let expr = parse("sin(x) > 0.5 && cos(y) < 0.8").unwrap();
        assert_eq!(expr.to_prefix_string(), "(&& (> (call sin x) 0.5) (< (call cos y) 0.8))");
    }

    #[test]
    fn test_parse_conditional_basic() {
        let expr = parse("if true then 1 else 0").unwrap();
        assert_eq!(expr.to_prefix_string(), "(if true then 1 else 0)");

        let expr = parse("if x > 0 then 1 else -1").unwrap();
        assert_eq!(expr.to_prefix_string(), "(if (> x 0) then 1 else (neg 1))");

        let expr = parse("if flag then a else b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(if flag then a else b)");
    }

    #[test]
    fn test_parse_conditional_complex() {
        let expr = parse("if x > 0 && y < 10 then x + y else 0").unwrap();
        assert_eq!(expr.to_prefix_string(), "(if (&& (> x 0) (< y 10)) then (+ x y) else 0)");

        let expr = parse("if !done then compute(x) else result").unwrap();
        assert_eq!(expr.to_prefix_string(), "(if (not done) then (call compute x) else result)");
    }

    #[test]
    fn test_parse_conditional_nested() {
        let expr = parse("if x > 0 then if y > 0 then 1 else 2 else 3").unwrap();
        assert_eq!(expr.to_prefix_string(), "(if (> x 0) then (if (> y 0) then 1 else 2) else 3)");
    }

    #[test]
    fn test_parse_conditional_with_arithmetic() {
        let expr = parse("if x == 0 then sin(y) else cos(z) + 1").unwrap();
        assert_eq!(expr.to_prefix_string(), "(if (== x 0) then (call sin y) else (+ (call cos z) 1))");

        let expr = parse("2 * if flag then a else b").unwrap();
        assert_eq!(expr.to_prefix_string(), "(* 2 (if flag then a else b))");
    }

    #[test]
    fn test_parse_conditional_errors() {
        // Missing then
        assert!(parse("if x > 0 else 1").is_err());
        
        // Missing else
        assert!(parse("if x > 0 then 1").is_err());
        
        // Missing condition
        assert!(parse("if then 1 else 0").is_err());
        
        // Missing then expression
        assert!(parse("if true then else 0").is_err());
        
        // Missing else expression
        assert!(parse("if true then 1 else").is_err());
    }
}
