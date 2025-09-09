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
}
