use super::{Lexer, Token};

#[test]
fn test_v030_keywords() {
    let source = "fn let mut if else for in return assert match struct enum and or not space module device material profile route test nets pins";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    let expected = vec![
        Token::Fn, Token::Let, Token::Mut, Token::If, Token::Else,
        Token::For, Token::In, Token::Return, Token::Assert, Token::Match,
        Token::Struct, Token::Enum, Token::And, Token::Or, Token::Not,
        Token::Space, Token::Module, Token::Device, Token::Material,
        Token::Profile, Token::Route, Token::Test, Token::Nets, Token::Pins,
        Token::Eof,
    ];

    let actual: Vec<_> = tokens.into_iter().map(|t| t.token).collect();
    assert_eq!(actual, expected);
}

#[test]
fn test_delimiters_and_operators() {
    let source = "{ } ( ) [ ] : ; , . -> => = += -= *= /= + - * / % == != < > <= >= .. ..= @";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    let expected = vec![
        Token::OpenBrace, Token::CloseBrace,
        Token::OpenParen, Token::CloseParen,
        Token::OpenBracket, Token::CloseBracket,
        Token::Colon, Token::Semicolon, Token::Comma, Token::Dot,
        Token::Arrow, Token::FatArrow,
        Token::Equals, Token::PlusEquals, Token::MinusEquals, Token::StarEquals, Token::SlashEquals,
        Token::Plus, Token::Hyphen, Token::Asterisk, Token::Slash, Token::Percent,
        Token::DoubleEquals, Token::NotEquals,
        Token::LessThan, Token::GreaterThan, Token::LessThanOrEqual, Token::GreaterThanOrEqual,
        Token::Range, Token::RangeInclusive,
        Token::AtSymbol,
        Token::Eof,
    ];

    let actual: Vec<_> = tokens.into_iter().map(|t| t.token).collect();
    assert_eq!(actual, expected);
}

#[test]
fn test_measurements_and_literals() {
    let source = "10um 1.8V 20mA 150nm 0x1F 42 3.14 \"hello\"";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert!(matches!(tokens[0].token, Token::Measurement(_)));
    assert!(matches!(tokens[1].token, Token::Measurement(_)));
    assert!(matches!(tokens[2].token, Token::Measurement(_)));
    assert!(matches!(tokens[3].token, Token::Measurement(_)));
    assert_eq!(tokens[4].token, Token::Integer(0x1F));
    assert_eq!(tokens[5].token, Token::Integer(42));
    assert_eq!(tokens[6].token, Token::Float(3.14));
    assert_eq!(tokens[7].token, Token::String("hello".into()));
}

#[test]
fn test_zero_indentation_sensitivity() {
    // HardwareScript v0.3.0 parses nested braces regardless of indentation
    let source = "
    fn my_func() {
let x = 10
        let y = 20
return x + y
    }
    ";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Fn);
    assert_eq!(tokens[1].token, Token::Identifier("my_func".into()));
    assert_eq!(tokens[2].token, Token::OpenParen);
    assert_eq!(tokens[3].token, Token::CloseParen);
    assert_eq!(tokens[4].token, Token::OpenBrace);
    assert_eq!(tokens[5].token, Token::Let);
    assert_eq!(tokens[6].token, Token::Identifier("x".into()));
    assert_eq!(tokens[7].token, Token::Equals);
    assert_eq!(tokens[8].token, Token::Integer(10));
    assert_eq!(tokens[9].token, Token::Let);
    assert_eq!(tokens[10].token, Token::Identifier("y".into()));
    assert_eq!(tokens[11].token, Token::Equals);
    assert_eq!(tokens[12].token, Token::Integer(20));
    assert_eq!(tokens[13].token, Token::Return);
    assert_eq!(tokens[14].token, Token::Identifier("x".into()));
    assert_eq!(tokens[15].token, Token::Plus);
    assert_eq!(tokens[16].token, Token::Identifier("y".into()));
    assert_eq!(tokens[17].token, Token::CloseBrace);
    assert_eq!(tokens[18].token, Token::Eof);
}
