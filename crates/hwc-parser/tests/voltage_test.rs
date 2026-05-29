use hwc_parser::lexer::{Lexer, Token};

#[test]
fn test_voltage_tokenization() {
    let source = "5V";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    println!("Tokens for '{}': {:?}", source, tokens);

    assert_eq!(tokens.len(), 2); // Measurement + EOF
    assert!(matches!(tokens[0].token, Token::Measurement(_)));
}

#[test]
fn test_battery_in_parens() {
    let source = "(5V)";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    println!("Tokens for '{}': {:?}", source, tokens);

    assert_eq!(tokens.len(), 4); // OpenParen + Measurement + CloseParen + EOF
    assert!(matches!(tokens[0].token, Token::OpenParen));
    assert!(matches!(tokens[1].token, Token::Measurement(_)));
    assert!(matches!(tokens[2].token, Token::CloseParen));
}
