//! Test profile definition parsing (v0.1.4)

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Definition, Lexer, Parser};

#[test]
fn test_parse_profile_complete() {
    let source = r#"profile HighVoltage:
    description: "IPC-2221 compliant constraints for >150V"
    trace:
        min_width: 254µm
        min_spacing: 508µm
    via:
        min_diameter: 508µm
        min_annular_ring: 254µm
    layer:
        max_count: 4
        min_thickness: 70µm
    clearance:
        high_voltage: 8.0mm
        safety_factor: 3.0
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Profile(profile) = &program.definitions[0] {
        assert_eq!(profile.name.as_str(), "HighVoltage");
        assert_eq!(
            profile.description,
            Some("IPC-2221 compliant constraints for >150V".into())
        );

        // Check trace constraints
        assert!(profile.trace.is_some());
        let trace = profile.trace.as_ref().unwrap();
        assert_eq!(trace.min_width.value, 254.0);
        assert_eq!(trace.min_spacing.value, 508.0);

        // Check via constraints
        assert!(profile.via.is_some());
        let via = profile.via.as_ref().unwrap();
        assert_eq!(via.min_diameter.value, 508.0);
        assert_eq!(via.min_annular_ring.value, 254.0);

        // Check layer constraints
        assert!(profile.layer.is_some());
        let layer = profile.layer.as_ref().unwrap();
        assert_eq!(layer.max_count, Some(4));
        assert!(layer.min_thickness.is_some());

        // Check clearance constraints
        assert!(profile.clearance.is_some());
        let clearance = profile.clearance.as_ref().unwrap();
        assert!(clearance.high_voltage.is_some());
        assert_eq!(clearance.safety_factor, Some(3.0));
    } else {
        panic!("Expected Profile definition");
    }
}

#[test]
fn test_parse_profile_minimal() {
    let source = r#"profile Standard:
    trace:
        min_width: 100µm
        min_spacing: 100µm
    via:
        min_diameter: 300µm
        min_annular_ring: 150µm
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Profile(profile) = &program.definitions[0] {
        assert_eq!(profile.name.as_str(), "Standard");
        assert_eq!(profile.description, None);
        assert!(profile.trace.is_some());
        assert!(profile.via.is_some());
        assert!(profile.layer.is_none());
        assert!(profile.clearance.is_none());
    } else {
        panic!("Expected Profile definition");
    }
}

#[test]
fn test_parse_profile_trace_only() {
    let source = r#"profile TraceOnly:
    trace:
        min_width: 200µm
        min_spacing: 200µm
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Profile(profile) = &program.definitions[0] {
        assert_eq!(profile.name.as_str(), "TraceOnly");
        assert!(profile.trace.is_some());
        assert!(profile.via.is_none());
        assert!(profile.layer.is_none());
        assert!(profile.clearance.is_none());
    } else {
        panic!("Expected Profile definition");
    }
}
