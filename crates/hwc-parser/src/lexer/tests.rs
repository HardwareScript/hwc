use super::{Lexer, Token};

#[test]
fn test_keywords() {
    let source = "import add route expose";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Import);
    assert_eq!(tokens[1].token, Token::Add);
    assert_eq!(tokens[2].token, Token::Route);
    assert_eq!(tokens[3].token, Token::Expose);
}

#[test]
fn test_define_is_now_identifier() {
    // v0.1.6: 'define' is no longer a keyword, it should lex as an identifier
    let source = "define";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("define".into()));
}

#[test]
fn test_connectors() {
    let source = "from named at rotated to by spanning as";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::From);
    assert_eq!(tokens[1].token, Token::Named);
    assert_eq!(tokens[2].token, Token::At);
    assert_eq!(tokens[3].token, Token::Rotated);
    assert_eq!(tokens[4].token, Token::To);
    assert_eq!(tokens[5].token, Token::By);
    assert_eq!(tokens[6].token, Token::Spanning);
    assert_eq!(tokens[7].token, Token::As);
}

#[test]
fn test_numbers() {
    let source = "42 -10 7.89 -2.5";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Sprint 3.9 Fix: Signs are now separate tokens (Hyphen/Plus)
    // This prevents "i+1" from being lexed as "i" + "+1" (greedy sign consumption)
    // Negative numbers are: Hyphen + Integer/Float
    assert_eq!(tokens[0].token, Token::Integer(42));
    assert_eq!(tokens[1].token, Token::Hyphen);
    assert_eq!(tokens[2].token, Token::Integer(10));
    assert_eq!(tokens[3].token, Token::Float(7.89));
    assert_eq!(tokens[4].token, Token::Hyphen);
    assert_eq!(tokens[5].token, Token::Float(2.5));
}

#[test]
fn test_strings() {
    let source = r#""Hello World" "Test""#;
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::String("Hello World".into()));
    assert_eq!(tokens[1].token, Token::String("Test".into()));
}

#[test]
fn test_coordinates() {
    let source = "[1,10,10]";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::OpenBracket);
    assert_eq!(tokens[1].token, Token::Integer(1));
    assert_eq!(tokens[2].token, Token::Comma);
    assert_eq!(tokens[3].token, Token::Integer(10));
    assert_eq!(tokens[4].token, Token::Comma);
    assert_eq!(tokens[5].token, Token::Integer(10));
    assert_eq!(tokens[6].token, Token::CloseBracket);
}

#[test]
fn test_units() {
    let source = "50mm 4.7kΩ 100nF 12V";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // v0.1.4: Units are parsed as atomic measurements
    assert!(matches!(tokens[0].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[0].token {
        assert_eq!(m.value, 50.0);
        assert!(matches!(m.unit, super::units::Unit::Distance(_)));
    }

    assert!(matches!(tokens[1].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[1].token {
        assert_eq!(m.value, 4.7);
        // Resistance is now Custom
        assert!(matches!(m.unit, super::units::Unit::Custom(_)));
    }

    assert!(matches!(tokens[2].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[2].token {
        assert_eq!(m.value, 100.0);
        // Capacitance is now Custom
        assert!(matches!(m.unit, super::units::Unit::Custom(_)));
    }

    assert!(matches!(tokens[3].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[3].token {
        assert_eq!(m.value, 12.0);
        assert!(matches!(m.unit, super::units::Unit::Voltage(_)));
    }
}

#[test]
fn test_unit_aliases() {
    let source = "4.7kOhm 100uF";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // v0.1.4: Keyboard aliases work
    assert!(matches!(tokens[0].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[0].token {
        assert_eq!(m.value, 4.7);
        // Resistance is now Custom
        assert!(matches!(m.unit, super::units::Unit::Custom(_)));
    }

    assert!(matches!(tokens[1].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[1].token {
        assert_eq!(m.value, 100.0);
        // Capacitance is now Custom
        assert!(matches!(m.unit, super::units::Unit::Custom(_)));
    }
}

#[test]
fn test_rotation() {
    let source = "rotated 45 rotated -30.5 rotated 90°";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Sprint 3.9 Fix: Signs are now separate tokens
    assert_eq!(tokens[0].token, Token::Rotated);
    assert_eq!(tokens[1].token, Token::Integer(45));
    assert_eq!(tokens[2].token, Token::Rotated);
    assert_eq!(tokens[3].token, Token::Hyphen);
    assert_eq!(tokens[4].token, Token::Float(30.5));
    assert_eq!(tokens[5].token, Token::Rotated);
    // v0.1.4: 90° is parsed as atomic measurement
    assert!(matches!(tokens[6].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[6].token {
        assert_eq!(m.value, 90.0);
        // Angle is now Custom("°")
        assert!(matches!(m.unit, super::units::Unit::Custom(_)));
    }
}

#[test]
fn test_comments() {
    let source = "## This is a doc comment\n## Another doc comment";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // Single # comments are ignored (not tokenized)
    // Only ## doc comments produce tokens
    assert_eq!(
        tokens[0].token,
        Token::DocComment("This is a doc comment".into())
    );
    assert_eq!(tokens[1].token, Token::Newline);
    assert_eq!(
        tokens[2].token,
        Token::DocComment("Another doc comment".into())
    );
}

#[test]
fn test_minimal_space_definition() {
    // v0.1.6: No 'define' keyword, type names are bare identifiers
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
"#;
    let lexer = Lexer::new(source);
    let result = lexer.tokenize();

    assert!(
        result.is_ok(),
        "Lexer should successfully tokenize minimal space definition"
    );
    let tokens = result.unwrap();

    // Verify key tokens are present
    assert_eq!(tokens[0].token, Token::Space);
    assert_eq!(tokens[1].token, Token::Identifier("Test".into()));
    assert_eq!(tokens[2].token, Token::Colon);
    assert_eq!(tokens[3].token, Token::Newline);
    assert_eq!(tokens[4].token, Token::Indent);
    assert_eq!(tokens[5].token, Token::Dimensions);
}

#[test]
fn test_dot_notation() {
    let source = "Power.Plus Driver.VIN";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("Power".into()));
    assert_eq!(tokens[1].token, Token::Dot);
    assert_eq!(tokens[2].token, Token::Identifier("Plus".into()));
    assert_eq!(tokens[3].token, Token::Identifier("Driver".into()));
    assert_eq!(tokens[4].token, Token::Dot);
    assert_eq!(tokens[5].token, Token::Identifier("VIN".into()));
}

#[test]
fn test_identifiers() {
    let source = "Battery_LiPo MainPower PullUp ErrorSignal";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("Battery_LiPo".into()));
    assert_eq!(tokens[1].token, Token::Identifier("MainPower".into()));
    assert_eq!(tokens[2].token, Token::Identifier("PullUp".into()));
    assert_eq!(tokens[3].token, Token::Identifier("ErrorSignal".into()));
}

#[test]
fn test_block_comment() {
    let source = r#"#[ This is a block comment ]#"#;
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens.len(), 2); // BlockComment + EOF
    match &tokens[0].token {
        Token::BlockComment(content) => {
            assert_eq!(content, "This is a block comment");
        }
        _ => panic!("Expected BlockComment token"),
    }
}

#[test]
fn test_multiline_block_comment() {
    let source = r#"#[
This is a multi-line
block comment
with multiple lines
]#"#;
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens.len(), 2); // BlockComment + EOF
    match &tokens[0].token {
        Token::BlockComment(content) => {
            assert!(content.contains("multi-line"));
            assert!(content.contains("multiple lines"));
        }
        _ => panic!("Expected BlockComment token"),
    }
}

#[test]
fn test_doc_block() {
    let source = r#"##[ This is a documentation block ]##"#;
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens.len(), 2); // DocBlock + EOF
    match &tokens[0].token {
        Token::DocBlock(content) => {
            assert_eq!(content, "This is a documentation block");
        }
        _ => panic!("Expected DocBlock token"),
    }
}

#[test]
fn test_multiline_doc_block() {
    let source = r#"##[
Advanced Motor Driver Module

This module provides complete motor control
with fault detection and overcurrent protection.
]##"#;
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens.len(), 2); // DocBlock + EOF
    match &tokens[0].token {
        Token::DocBlock(content) => {
            assert!(content.contains("Advanced Motor Driver"));
            assert!(content.contains("fault detection"));
        }
        _ => panic!("Expected DocBlock token"),
    }
}

// ========================================================================
// PHASE 9: TRI-FOLD CASE SENSITIVITY TESTS
// ========================================================================

#[test]
fn test_uppercase_keyword_rejected() {
    // Software domain keywords must be lowercase
    let source = "Define space \"Test\":";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // "Define" should be tokenized as Identifier, not Define keyword
    assert!(matches!(tokens[0].token, Token::Identifier(_)));
    if let Token::Identifier(name) = &tokens[0].token {
        assert_eq!(name, "Define");
    }
}

#[test]
fn test_mixed_case_keyword_rejected() {
    // Software domain keywords must be lowercase
    let source = "Dimensions: 50mm by 50mm by 4mm";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // "Dimensions" should be tokenized as Identifier, not Dimensions keyword
    assert!(matches!(tokens[0].token, Token::Identifier(_)));
    if let Token::Identifier(name) = &tokens[0].token {
        assert_eq!(name, "Dimensions");
    }
}

#[test]
fn test_uppercase_origin_rejected() {
    // Origin points must be lowercase
    let source = "origin: TL";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // "TL" should be tokenized as Identifier, not TopLeft
    assert_eq!(tokens[0].token, Token::Origin);
    assert_eq!(tokens[1].token, Token::Colon);
    assert!(matches!(tokens[2].token, Token::Identifier(_)));
    if let Token::Identifier(name) = &tokens[2].token {
        assert_eq!(name, "TL");
    }
}

#[test]
fn test_lowercase_keywords_accepted() {
    // v0.1.6: All lowercase keywords should be accepted (no 'define' keyword)
    let source = "space component substrate dimensions grid path origin";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Space);
    assert_eq!(tokens[1].token, Token::Component);
    assert_eq!(tokens[2].token, Token::Substrate);
    assert_eq!(tokens[3].token, Token::Dimensions);
    assert_eq!(tokens[4].token, Token::Grid);
    assert_eq!(tokens[5].token, Token::Path);
    assert_eq!(tokens[6].token, Token::Origin);
}

#[test]
fn test_lowercase_origins_accepted() {
    // All lowercase origin points should be accepted
    let source = "tl bl tr br";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // v0.2.1: tl/bl/tr br are now identifiers (contextually parsed)
    assert_eq!(tokens[0].token, Token::Identifier("tl".into()));
    assert_eq!(tokens[1].token, Token::Identifier("bl".into()));
    assert_eq!(tokens[2].token, Token::Identifier("tr".into()));
    assert_eq!(tokens[3].token, Token::Identifier("br".into()));
}

#[test]
fn test_si_unit_case_sensitive() {
    // SI units must maintain strict case
    let source = "12V 500mV"; // Correct case
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // v0.1.4: Units are atomic measurements
    assert!(matches!(tokens[0].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[0].token {
        assert_eq!(m.value, 12.0);
        assert!(matches!(m.unit, super::units::Unit::Voltage(_)));
    }

    assert!(matches!(tokens[1].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[1].token {
        assert_eq!(m.value, 500.0);
        assert!(matches!(m.unit, super::units::Unit::Voltage(_)));
    }
}

#[test]
fn test_si_unit_wrong_case_rejected() {
    // With generic measurement parser, wrong case units become Custom units
    // This is the desired behavior - we don't crash on unknown units
    let source = "12v 500MV"; // Wrong case
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // "12v" is parsed as a measurement with Custom("v") unit
    assert!(matches!(tokens[0].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[0].token {
        assert_eq!(m.value, 12.0);
        assert!(matches!(m.unit, crate::lexer::units::Unit::Custom(_)));
    }

    // "500MV" is parsed as a measurement with Custom("MV") unit
    assert!(matches!(tokens[1].token, Token::Measurement(_)));
    if let Token::Measurement(m) = &tokens[1].token {
        assert_eq!(m.value, 500.0);
        assert!(matches!(m.unit, crate::lexer::units::Unit::Custom(_)));
    }
}

#[test]
fn test_property_keywords_are_identifiers() {
    // v0.1.6: Property keywords like 'tolerance', 'trace', etc. are now identifiers
    let source =
        "tolerance trace via clearance category properties metadata pins layout electrical render";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("tolerance".into()));
    assert_eq!(tokens[1].token, Token::Identifier("trace".into()));
    assert_eq!(tokens[2].token, Token::Identifier("via".into()));
    assert_eq!(tokens[3].token, Token::Identifier("clearance".into()));
    assert_eq!(tokens[4].token, Token::Identifier("category".into()));
    assert_eq!(tokens[5].token, Token::Identifier("properties".into()));
    assert_eq!(tokens[6].token, Token::Identifier("metadata".into()));
    assert_eq!(tokens[7].token, Token::Identifier("pins".into()));
    assert_eq!(tokens[8].token, Token::Identifier("layout".into()));
    assert_eq!(tokens[9].token, Token::Identifier("electrical".into()));
    assert_eq!(tokens[10].token, Token::Identifier("render".into()));
}

#[test]
fn test_more_property_keywords_are_identifiers() {
    // v0.1.6: More property keywords are now identifiers
    let source = "bindings protocols setup execute assert steps target layer";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("bindings".into()));
    assert_eq!(tokens[1].token, Token::Identifier("protocols".into()));
    assert_eq!(tokens[2].token, Token::Identifier("setup".into()));
    assert_eq!(tokens[3].token, Token::Identifier("execute".into()));
    assert_eq!(tokens[4].token, Token::Identifier("assert".into()));
    assert_eq!(tokens[5].token, Token::Identifier("steps".into()));
    assert_eq!(tokens[6].token, Token::Identifier("target".into()));
    assert_eq!(tokens[7].token, Token::Identifier("layer".into()));
}

// ========================================================================
// TASK A3: LOGIC OPERATOR KEYWORDS TESTS (v0.1.6)
// ========================================================================

#[test]
fn test_logic_operator_keywords() {
    // v0.1.6: Add word-form logic operators
    let source = "and or not xor";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::And);
    assert_eq!(tokens[1].token, Token::Or);
    assert_eq!(tokens[2].token, Token::Not);
    assert_eq!(tokens[3].token, Token::Xor);
}

#[test]
fn test_logic_operator_symbols_still_work() {
    // v0.1.6: Symbol operators still work (except caret)
    let source = "& | !";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Ampersand);
    assert_eq!(tokens[1].token, Token::Pipe);
    assert_eq!(tokens[2].token, Token::Exclamation);
}

#[test]
fn test_caret_removed() {
    // v0.1.6: Caret (^) is no longer recognized as a token
    // It should cause a lexer error
    let source = "a ^ b";
    let lexer = Lexer::new(source);
    let result = lexer.tokenize();

    // The lexer should fail because ^ is not a valid token
    assert!(
        result.is_err(),
        "Caret should not be recognized as a valid token"
    );
}

#[test]
fn test_logic_expressions_with_keywords() {
    // v0.1.6: Logic expressions can use word-form operators
    let source = "a and b or not c";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("a".into()));
    assert_eq!(tokens[1].token, Token::And);
    assert_eq!(tokens[2].token, Token::Identifier("b".into()));
    assert_eq!(tokens[3].token, Token::Or);
    assert_eq!(tokens[4].token, Token::Not);
    assert_eq!(tokens[5].token, Token::Identifier("c".into()));
}

#[test]
fn test_xor_keyword_only() {
    // v0.1.6: XOR is word-only (no symbol form)
    let source = "a xor b";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("a".into()));
    assert_eq!(tokens[1].token, Token::Xor);
    assert_eq!(tokens[2].token, Token::Identifier("b".into()));
}

#[test]
fn test_mixed_logic_operators() {
    // v0.1.6: Can mix word and symbol forms
    let source = "a & b or c and d | e";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("a".into()));
    assert_eq!(tokens[1].token, Token::Ampersand);
    assert_eq!(tokens[2].token, Token::Identifier("b".into()));
    assert_eq!(tokens[3].token, Token::Or);
    assert_eq!(tokens[4].token, Token::Identifier("c".into()));
    assert_eq!(tokens[5].token, Token::And);
    assert_eq!(tokens[6].token, Token::Identifier("d".into()));
    assert_eq!(tokens[7].token, Token::Pipe);
    assert_eq!(tokens[8].token, Token::Identifier("e".into()));
}

#[test]
fn test_lowercase_reg_keyword() {
    // v0.1.6: Register primitive is now lowercase 'reg'
    let source = "reg";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::RegisterInit);
}

#[test]
fn test_uppercase_reg_is_identifier() {
    // v0.1.6: Uppercase 'Reg' is no longer a keyword, should lex as identifier
    let source = "Reg";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("Reg".into()));
}

#[test]
fn test_reg_in_expression() {
    // v0.1.6: reg() should parse correctly in expressions
    let source = "reg(clock: Clk, reset: Rst, init: 0)";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::RegisterInit);
    assert_eq!(tokens[1].token, Token::OpenParen);
    assert_eq!(tokens[2].token, Token::Identifier("clock".into()));
    assert_eq!(tokens[3].token, Token::Colon);
    assert_eq!(tokens[4].token, Token::Identifier("Clk".into()));
}

#[test]
fn test_single_equals_for_comparison() {
    // v0.1.6: Single '=' is used for both assignment and comparison
    let source = "a = b";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("a".into()));
    assert_eq!(tokens[1].token, Token::Equals);
    assert_eq!(tokens[2].token, Token::Identifier("b".into()));
}

#[test]
fn test_double_equals_removed() {
    // v0.1.6: Double equals '==' is no longer recognized
    // It should lex as two separate '=' tokens
    let source = "a == b";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].token, Token::Identifier("a".into()));
    assert_eq!(tokens[1].token, Token::Equals);
    assert_eq!(tokens[2].token, Token::Equals);
    assert_eq!(tokens[3].token, Token::Identifier("b".into()));
}
