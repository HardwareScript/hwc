use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, Parser};
use std::fs;

#[test]
fn test_parse_actual_resistor_component() {
    // Load the actual resistor component file
    let source = fs::read_to_string("../../data/components/resistor_0805_10k.hw")
        .expect("Failed to read component file");

    let lexer = Lexer::new(&source);
    let tokens = lexer.tokenize().expect("Tokenization failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &program.definitions[0] {
        assert_eq!(comp.name.as_str(), "Resistor_0805_10K");

        // Check metadata
        let metadata = comp.metadata.as_ref().unwrap();
        assert_eq!(metadata.manufacturer, Some("Yageo".into()));
        assert_eq!(metadata.part_number, Some("RC0805FR-0710KL".into()));
        assert_eq!(metadata.package, Some("0805".into()));
        assert_eq!(metadata.value, Some("10kΩ".into()));

        // Check pins
        assert_eq!(comp.pins.len(), 2);
        assert_eq!(comp.pins[0], "Pin1");
        assert_eq!(comp.pins[1], "Pin2");

        // Check layout
        let layout = comp.layout.as_ref().unwrap();
        assert!(layout.shape.is_some());
        assert_eq!(layout.pin_positions.len(), 2);
        assert!(layout.pin_positions.contains_key("Pin1"));
        assert!(layout.pin_positions.contains_key("Pin2"));

        // Check electrical
        let electrical = comp.electrical.as_ref().unwrap();
        assert!(electrical.properties.contains_key("resistance"));
        assert!(electrical.properties.contains_key("tolerance"));
        assert!(electrical.properties.contains_key("max_power"));
        assert!(electrical.properties.contains_key("max_voltage"));

        // Check render
        let render = comp.render.as_ref().unwrap();
        assert_eq!(render.render_type, Some("procedural".into()));
        assert_eq!(render.shape, Some("smd_passive".into()));
        assert_eq!(render.body_color, Some("#1a1a1a".into()));
        assert_eq!(render.endcap_color, Some("#c0c0c0".into()));
        assert_eq!(render.label, Some("103".into()));
    } else {
        panic!("Expected Component definition");
    }
}

#[test]
fn test_parse_actual_capacitor_component() {
    // Load the actual capacitor component file
    let source = fs::read_to_string("../../data/components/capacitor_0805_10uF.hw")
        .expect("Failed to read component file");

    let lexer = Lexer::new(&source);
    let tokens = lexer.tokenize().expect("Tokenization failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &program.definitions[0] {
        assert_eq!(comp.name.as_str(), "Capacitor_0805_10uF");

        // Check metadata
        let metadata = comp.metadata.as_ref().unwrap();
        assert_eq!(metadata.manufacturer, Some("Murata".into()));
        assert_eq!(metadata.value, Some("10µF".into()));

        // Check pins
        assert_eq!(comp.pins.len(), 2);

        // Check layout
        assert!(comp.layout.is_some());

        // Check electrical
        assert!(comp.electrical.is_some());

        // Check render
        assert!(comp.render.is_some());
    } else {
        panic!("Expected Component definition");
    }
}

#[test]
fn test_parse_minimal_component() {
    let source = r#"component MinimalTest:
    pins:
        Pin1
        Pin2
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");

    // Debug: print tokens
    println!("Tokens:");
    for (i, token) in tokens.iter().enumerate() {
        println!("{}: {:?}", i, token);
    }

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Component(comp) = &program.definitions[0] {
        assert_eq!(comp.name.as_str(), "MinimalTest");
        assert_eq!(comp.pins.len(), 2);
        assert_eq!(comp.pins[0], "Pin1");
        assert_eq!(comp.pins[1], "Pin2");
        assert!(comp.metadata.is_none());
        assert!(comp.layout.is_none());
        assert!(comp.electrical.is_none());
        assert!(comp.render.is_none());
    } else {
        panic!("Expected Component definition");
    }
}
