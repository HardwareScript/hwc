// Task B7: Flexible Metadata and Profile Block Parsing Tests
// Tests that metadata and profile blocks accept custom fields without errors

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::lexer::Lexer;
use hwc_parser::parser::Parser;

#[test]
fn test_metadata_with_standard_fields() {
    let source = r#"component Resistor:
    metadata:
        manufacturer: "Yageo"
        part_number: "RC0805FR-0710KL"
        package: "0805"
        value: "10kΩ"
        description: "Thick film resistor"
        datasheet: "https://example.com/datasheet.pdf"
    pins: [A, B]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let _result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing
}

#[test]
fn test_metadata_with_custom_fields() {
    let source = r#"component Resistor:
    metadata:
        manufacturer: "Yageo"
        part_number: "RC0805FR-0710KL"
        internal_code: "PROJ-2024-001"
        certification: "RoHS compliant"
        supplier: "Digi-Key"
        cost_center: "Engineering"
        revision: "Rev B"
    pins: [A, B]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));

    let program = result;
    assert_eq!(program.definitions.len(), 1);

    // Verify custom fields are stored
    if let hwc_parser::ast::Definition::Component(comp) = &program.definitions[0] {
        let metadata = comp.metadata.as_ref().unwrap();
        assert!(metadata.other.contains_key("internal_code"));
        assert!(metadata.other.contains_key("certification"));
        assert!(metadata.other.contains_key("supplier"));
        assert!(metadata.other.contains_key("cost_center"));
        assert!(metadata.other.contains_key("revision"));
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_metadata_mixed_standard_and_custom() {
    let source = r#"component Capacitor:
    metadata:
        manufacturer: "Murata"
        custom_field_1: "Value 1"
        part_number: "GRM188R71C104KA01D"
        custom_field_2: "Value 2"
        package: "0603"
        tracking_id: "TRACK-2024-042"
    pins: [P, N]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let _result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing
}

#[test]
fn test_profile_with_standard_constraints() {
    let source = r#"profile Standard:
    description: "Standard PCB manufacturing profile"
    trace:
        min_width: 0.15mm
        min_spacing: 0.15mm
    via:
        min_diameter: 0.3mm
        min_annular_ring: 0.15mm
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let _result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing
}

#[test]
fn test_profile_with_unknown_constraint_block() {
    let source = r#"profile Advanced:
    description: "Advanced profile with custom constraints"
    trace:
        min_width: 0.1mm
        min_spacing: 0.1mm
    custom_constraint:
        field1: 10mm
        field2: 5mm
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let _result = parser.parse(&DiagnosticCollector::new("", 100));
    // After Task B7, unknown constraint blocks should be skipped without error
    // Test passes if no panic occurs during parsing
}

#[test]
fn test_profile_with_custom_string_field() {
    let source = r#"profile CustomProfile:
    description: "Profile with custom tracking fields"
    trace:
        min_width: 0.15mm
        min_spacing: 0.15mm
    project_code: "PROJ-2024-001"
    revision: "Rev A"
    approved_by: "Engineering Team"
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let result = parser.parse(&DiagnosticCollector::new("", 100));

    let program = result;
    assert_eq!(program.definitions.len(), 1);

    // Verify custom fields are stored
    if let hwc_parser::ast::Definition::Profile(profile) = &program.definitions[0] {
        assert!(profile.other.contains_key("project_code"));
        assert!(profile.other.contains_key("revision"));
        assert!(profile.other.contains_key("approved_by"));
    } else {
        panic!("Expected profile definition");
    }
}

#[test]
fn test_metadata_only_custom_fields() {
    let source = r#"component CustomPart:
    metadata:
        project_code: "PROJ-X"
        revision: "A1"
        approved_by: "Engineering"
        date: "2024-01-15"
    pins: [P1, P2]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let _result = parser.parse(&DiagnosticCollector::new("", 100));
    // Test passes if no panic occurs during parsing
}
