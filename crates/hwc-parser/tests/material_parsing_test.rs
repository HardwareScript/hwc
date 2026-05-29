//! Test material definition parsing (v0.1.4)

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Definition, Lexer, MaterialCategory, Parser, PropertyValue};

#[test]
fn test_parse_material_definition() {
    let source = r#"material Copper:
    category: conductor
    symbol: "Cu"
    description: "Universal PCB trace material"
    properties:
        density: 8960
        resistivity: 1.68e-8
        color: "B87333"
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Material(mat) = &program.definitions[0] {
        assert_eq!(mat.name.as_str(), "Copper");
        assert_eq!(mat.category, MaterialCategory::Conductor);
        assert_eq!(mat.symbol, Some("Cu".into()));
        assert_eq!(mat.description, Some("Universal PCB trace material".into()));
        assert_eq!(mat.properties.len(), 3);

        // Check properties
        assert_eq!(mat.properties[0].key, "density");
        assert!(matches!(mat.properties[0].value, PropertyValue::Number(_)));

        assert_eq!(mat.properties[1].key, "resistivity");
        assert!(matches!(mat.properties[1].value, PropertyValue::Number(_)));

        assert_eq!(mat.properties[2].key, "color");
        assert!(matches!(mat.properties[2].value, PropertyValue::String(_)));
    } else {
        panic!("Expected Material definition");
    }
}

#[test]
fn test_parse_material_minimal() {
    let source = r#"material Silicon:
    category: semiconductor
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Material(mat) = &program.definitions[0] {
        assert_eq!(mat.name.as_str(), "Silicon");
        assert_eq!(mat.category, MaterialCategory::Semiconductor);
        assert_eq!(mat.symbol, None);
        assert_eq!(mat.description, None);
        assert_eq!(mat.properties.len(), 0);
    } else {
        panic!("Expected Material definition");
    }
}

#[test]
fn test_parse_material_with_measurements() {
    let source = r#"material FR4:
    category: insulator
    properties:
        thickness: 1.6mm
        dielectric_strength: 20kV
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Failed to tokenize");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let Definition::Material(mat) = &program.definitions[0] {
        assert_eq!(mat.name.as_str(), "FR4");
        assert_eq!(mat.category, MaterialCategory::Insulator);
        assert_eq!(mat.properties.len(), 2);

        // Check measurement properties
        assert_eq!(mat.properties[0].key, "thickness");
        assert!(matches!(
            mat.properties[0].value,
            PropertyValue::Measurement(_)
        ));

        assert_eq!(mat.properties[1].key, "dielectric_strength");
        assert!(matches!(
            mat.properties[1].value,
            PropertyValue::Measurement(_)
        ));
    } else {
        panic!("Expected Material definition");
    }
}
