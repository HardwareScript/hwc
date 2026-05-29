use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::lexer::Lexer;
use hwc_parser::parser::Parser;

/// Test B8: Struct parsing without fields: keyword (v0.1.6)
///
/// v0.1.6 removes the `fields:` keyword from struct definitions.
/// Structs are now bare bit-width tables.

#[test]
fn test_struct_without_fields_keyword() {
    let source = r#"struct Instruction:
    opcode[4]
    func[4]
    imm[8]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    assert!(
        result.is_ok(),
        "Should parse struct without fields: keyword: {:?}",
        result.err()
    );

    let struct_def = result.unwrap();
    assert_eq!(struct_def.name.as_str(), "Instruction");
    assert_eq!(struct_def.fields.len(), 3);
    assert_eq!(struct_def.fields[0].name, "opcode");
    assert_eq!(struct_def.fields[0].width, 4);
    assert_eq!(struct_def.fields[1].name, "func");
    assert_eq!(struct_def.fields[1].width, 4);
    assert_eq!(struct_def.fields[2].name, "imm");
    assert_eq!(struct_def.fields[2].width, 8);
}

#[test]
fn test_struct_single_field() {
    let source = r#"struct Status:
    ready[1]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    assert!(
        result.is_ok(),
        "Should parse single-field struct: {:?}",
        result.err()
    );

    let struct_def = result.unwrap();
    assert_eq!(struct_def.name.as_str(), "Status");
    assert_eq!(struct_def.fields.len(), 1);
    assert_eq!(struct_def.fields[0].name, "ready");
    assert_eq!(struct_def.fields[0].width, 1);
}

#[test]
fn test_struct_multiple_fields() {
    let source = r#"struct Register:
    opcode[4]
    rs1[5]
    rs2[5]
    rd[5]
    funct3[3]
    funct7[7]
    imm[12]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    assert!(
        result.is_ok(),
        "Should parse multi-field struct: {:?}",
        result.err()
    );

    let struct_def = result.unwrap();
    assert_eq!(struct_def.name.as_str(), "Register");
    assert_eq!(struct_def.fields.len(), 7);

    // Verify all fields
    assert_eq!(struct_def.fields[0].name, "opcode");
    assert_eq!(struct_def.fields[0].width, 4);
    assert_eq!(struct_def.fields[1].name, "rs1");
    assert_eq!(struct_def.fields[1].width, 5);
    assert_eq!(struct_def.fields[2].name, "rs2");
    assert_eq!(struct_def.fields[2].width, 5);
    assert_eq!(struct_def.fields[3].name, "rd");
    assert_eq!(struct_def.fields[3].width, 5);
    assert_eq!(struct_def.fields[4].name, "funct3");
    assert_eq!(struct_def.fields[4].width, 3);
    assert_eq!(struct_def.fields[5].name, "funct7");
    assert_eq!(struct_def.fields[5].width, 7);
    assert_eq!(struct_def.fields[6].name, "imm");
    assert_eq!(struct_def.fields[6].width, 12);
}

#[test]
fn test_struct_with_various_widths() {
    let source = r#"struct DataPacket:
    header[8]
    payload[256]
    checksum[16]
    footer[8]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    assert!(
        result.is_ok(),
        "Should parse struct with various widths: {:?}",
        result.err()
    );

    let struct_def = result.unwrap();
    assert_eq!(struct_def.fields.len(), 4);
    assert_eq!(struct_def.fields[0].width, 8);
    assert_eq!(struct_def.fields[1].width, 256);
    assert_eq!(struct_def.fields[2].width, 16);
    assert_eq!(struct_def.fields[3].width, 8);
}

#[test]
fn test_struct_field_name_format() {
    let source = r#"struct Config:
    enable_flag[1]
    mode_select[2]
    data_width[8]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    assert!(
        result.is_ok(),
        "Should parse struct with underscore names: {:?}",
        result.err()
    );

    let struct_def = result.unwrap();
    assert_eq!(struct_def.fields[0].name, "enable_flag");
    assert_eq!(struct_def.fields[1].name, "mode_select");
    assert_eq!(struct_def.fields[2].name, "data_width");
}

#[test]
fn test_struct_with_fields_keyword_should_fail() {
    // This tests the migration guard - old v0.1.5 syntax should fail
    let source = r#"struct Instruction:
    fields:
        opcode[4]
        func[4]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    assert!(
        result.is_err(),
        "Should fail when fields: keyword is present (v0.1.5 syntax)"
    );
}

#[test]
fn test_struct_empty_should_fail() {
    let source = r#"struct Empty:
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    // Empty structs should fail - structs must have at least one field
    assert!(result.is_err(), "Should fail for empty struct");
}

#[test]
fn test_struct_missing_width_should_fail() {
    let source = r#"struct Bad:
    opcode
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    assert!(result.is_err(), "Should fail when field width is missing");
}

#[test]
fn test_struct_invalid_width_format_should_fail() {
    let source = r#"struct Bad:
    opcode(4)
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let result = parser.parse_struct();
    assert!(
        result.is_err(),
        "Should fail when width uses parentheses instead of brackets"
    );
}

#[test]
fn test_struct_in_full_definition() {
    // Test struct as part of a complete file parse
    let source = r#"struct Instruction:
    opcode[4]
    func[4]

module Processor:
    pins: [Clk, Data[8]]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexer should succeed");
    let mut parser = Parser::new(tokens);

    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);
    assert!(
        !collector.has_errors(),
        "Should parse file with struct definition"
    );
    assert_eq!(program.definitions.len(), 2);
}
