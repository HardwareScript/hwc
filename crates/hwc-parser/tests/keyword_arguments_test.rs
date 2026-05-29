//! Tests for v0.1.6 keyword argument requirement in component instantiation

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::parser::Parser;
use hwc_parser::{lexer::Lexer, Parameter, ParameterValue};

fn parse(source: &str) -> hwc_parser::Program {
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenize failed");
    let mut parser = Parser::new(tokens);
    parser.parse(&DiagnosticCollector::new("", 100))
}

fn parse_with_collector(source: &str, collector: &DiagnosticCollector) -> hwc_parser::Program {
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenize failed");
    let mut parser = Parser::new(tokens);
    parser.parse(collector)
}

#[test]
fn test_keyword_argument_single() {
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
    add Resistor (resistance: 10kΩ) named R1 at [x: 1mm, y: 1mm, z: 1]
"#;

    let program = parse(source);
    let space = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(s) = def {
                Some(s)
            } else {
                None
            }
        })
        .expect("No space found");

    assert_eq!(space.components().len(), 1);
    let component = &space.components()[0];
    assert_eq!(component.component_type.as_str(), "Resistor");
    assert_eq!(component.parameters.len(), 1);

    let Parameter::Keyword { name, value } = &component.parameters[0];
    assert_eq!(name, "resistance");
    match value {
        ParameterValue::Measurement(m) => {
            // The value is stored as-is with the unit
            assert_eq!(m.value, 10.0); // 10 (with kΩ unit)
        }
        _ => panic!("Expected measurement value"),
    }
}

#[test]
fn test_keyword_argument_multiple() {
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
    add Capacitor (capacitance: 100nF, voltage: 50V) named C1 at [x: 1mm, y: 1mm, z: 1]
"#;

    let program = parse(source);
    let space = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(s) = def {
                Some(s)
            } else {
                None
            }
        })
        .expect("No space found");

    assert_eq!(space.components().len(), 1);
    let component = &space.components()[0];
    assert_eq!(component.component_type.as_str(), "Capacitor");
    assert_eq!(component.parameters.len(), 2);

    // Check first parameter
    let Parameter::Keyword { name, value } = &component.parameters[0];
    assert_eq!(name, "capacitance");
    match value {
        ParameterValue::Measurement(_) => {} // 100nF
        _ => panic!("Expected measurement value"),
    }

    // Check second parameter
    let Parameter::Keyword { name, value } = &component.parameters[1];
    assert_eq!(name, "voltage");
    match value {
        ParameterValue::Measurement(_) => {} // 50V
        _ => panic!("Expected measurement value"),
    }
}

#[test]
fn test_keyword_argument_string_value() {
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
    add LED (color: "Red") named LED1 at [x: 1mm, y: 1mm, z: 1]
"#;

    let program = parse(source);
    let space = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(s) = def {
                Some(s)
            } else {
                None
            }
        })
        .expect("No space found");

    assert_eq!(space.components().len(), 1);
    let component = &space.components()[0];
    assert_eq!(component.parameters.len(), 1);

    let Parameter::Keyword { name, value } = &component.parameters[0];
    assert_eq!(name, "color");
    match value {
        ParameterValue::String(s) => {
            assert_eq!(s, "Red");
        }
        _ => panic!("Expected string value"),
    }
}

#[test]
fn test_keyword_argument_number_value() {
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
    add Counter (count: 8) named CNT1 at [x: 1mm, y: 1mm, z: 1]
"#;

    let program = parse(source);
    let space = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(s) = def {
                Some(s)
            } else {
                None
            }
        })
        .expect("No space found");

    assert_eq!(space.components().len(), 1);
    let component = &space.components()[0];
    assert_eq!(component.parameters.len(), 1);

    match &component.parameters[0] {
        Parameter::Keyword { name, value } => {
            assert_eq!(name, "count");
            match value {
                ParameterValue::Number(n) => {
                    assert_eq!(*n, 8.0);
                }
                _ => panic!("Expected number value"),
            }
        }
    }
}

#[test]
fn test_empty_parameters() {
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
    add LED() named LED1 at [x: 1mm, y: 1mm, z: 1]
"#;

    let program = parse(source);
    let space = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(s) = def {
                Some(s)
            } else {
                None
            }
        })
        .expect("No space found");

    assert_eq!(space.components().len(), 1);
    let component = &space.components()[0];
    assert_eq!(component.parameters.len(), 0);
}

#[test]
fn test_no_parameters() {
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
    add LED named LED1 at [x: 1mm, y: 1mm, z: 1]
"#;

    let program = parse(source);
    let space = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(s) = def {
                Some(s)
            } else {
                None
            }
        })
        .expect("No space found");

    assert_eq!(space.components().len(), 1);
    let component = &space.components()[0];
    assert_eq!(component.parameters.len(), 0);
}

#[test]
fn test_positional_argument_fails() {
    // v0.1.6: Positional arguments should fail
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
    add Battery (5V) named BAT1 at [x: 1mm, y: 1mm, z: 1]
"#;

    let collector = DiagnosticCollector::new(source, 100);
    let _result = parse_with_collector(source, &collector);
    assert!(
        collector.has_errors(),
        "Positional arguments should not be allowed in v0.1.6"
    );
}

#[test]
fn test_mixed_parameters() {
    let source = r#"space Test:
    dimensions: 10mm by 10mm by 2mm
    grid: 10 by 10 by 2
    add Resistor (resistance: 10kΩ, tolerance: 5%, power: 0.25W) named R1 at [x: 1mm, y: 1mm, z: 1]
"#;

    let program = parse(source);
    let space = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(s) = def {
                Some(s)
            } else {
                None
            }
        })
        .expect("No space found");

    assert_eq!(space.components().len(), 1);
    let component = &space.components()[0];
    assert_eq!(component.parameters.len(), 3);

    // All should be keyword parameters
    for param in &component.parameters {
        assert!(
            matches!(param, Parameter::Keyword { .. }),
            "All parameters should be keyword arguments"
        );
    }
}
