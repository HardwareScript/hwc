use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, Parser};

#[test]
fn test_pad_shape_circle() {
    let input = r#"
component TestChip:
    pins: VCC, GND
    layout:
        shape: Rectangle(5mm, 5mm, 1mm)
        pin_positions:
            VCC at [1mm, 1mm]
            GND at [4mm, 4mm]
        pad_shapes:
            VCC: Circle(0.5mm)
            GND: Circle(0.8mm)
"#;

    let lexer = Lexer::new(input);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(input, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::Definition::Component(comp) = &program.definitions[0] {
        assert_eq!(comp.name.as_str(), "TestChip");
        let layout = comp.layout.as_ref().unwrap();
        assert_eq!(layout.pad_shapes.len(), 2);
        assert_eq!(layout.pad_shapes.get("VCC"), Some(&"Circle(0.5mm)".into()));
        assert_eq!(layout.pad_shapes.get("GND"), Some(&"Circle(0.8mm)".into()));
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_pad_shape_rectangle() {
    let input = r#"
component SMDResistor:
    pins: Pin1, Pin2
    layout:
        shape: Rectangle(3.2mm, 1.6mm, 0.5mm)
        pin_positions:
            Pin1 at [0.5mm, 0.8mm]
            Pin2 at [2.7mm, 0.8mm]
        pad_shapes:
            Pin1: Rectangle(1mm, 0.8mm)
            Pin2: Rectangle(1mm, 0.8mm)
"#;

    let lexer = Lexer::new(input);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(input, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::Definition::Component(comp) = &program.definitions[0] {
        let layout = comp.layout.as_ref().unwrap();
        assert_eq!(layout.pad_shapes.len(), 2);
        assert_eq!(
            layout.pad_shapes.get("Pin1"),
            Some(&"Rectangle(1mm, 0.8mm)".into())
        );
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_pad_shape_obround() {
    let input = r#"
component Connector:
    pins: P1, P2
    layout:
        shape: Rectangle(10mm, 5mm, 2mm)
        pin_positions:
            P1 at [2mm, 2.5mm]
            P2 at [8mm, 2.5mm]
        pad_shapes:
            P1: Obround(2mm, 1mm)
            P2: Obround(2mm, 1mm)
"#;

    let lexer = Lexer::new(input);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(input, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::Definition::Component(comp) = &program.definitions[0] {
        let layout = comp.layout.as_ref().unwrap();
        assert_eq!(
            layout.pad_shapes.get("P1"),
            Some(&"Obround(2mm, 1mm)".into())
        );
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_pad_shape_rounded_rect() {
    let input = r#"
component ModernChip:
    pins: A1, A2
    layout:
        shape: Rectangle(8mm, 8mm, 1mm)
        pin_positions:
            A1 at [2mm, 2mm]
            A2 at [6mm, 6mm]
        pad_shapes:
            A1: RoundedRect(1.5mm, 1mm, 0.2mm)
            A2: RoundedRect(1.5mm, 1mm, 0.2mm)
"#;

    let lexer = Lexer::new(input);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(input, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::Definition::Component(comp) = &program.definitions[0] {
        let layout = comp.layout.as_ref().unwrap();
        assert_eq!(
            layout.pad_shapes.get("A1"),
            Some(&"RoundedRect(1.5mm, 1mm, 0.2mm)".into())
        );
    } else {
        panic!("Expected component definition");
    }
}

#[test]
fn test_pad_shape_polygon() {
    let input = r#"
component CustomPad:
    pins: PAD1
    layout:
        shape: Rectangle(5mm, 5mm, 1mm)
        pin_positions:
            PAD1 at [2.5mm, 2.5mm]
        pad_shapes:
            PAD1: Polygon(0mm, 0mm, 1mm, 0mm, 1mm, 1mm, 0mm, 1mm)
"#;

    let lexer = Lexer::new(input);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(input, 100);

    let program = parser.parse(&collector);

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::Definition::Component(comp) = &program.definitions[0] {
        let layout = comp.layout.as_ref().unwrap();
        assert_eq!(
            layout.pad_shapes.get("PAD1"),
            Some(&"Polygon(0mm, 0mm, 1mm, 0mm, 1mm, 1mm, 0mm, 1mm)".into())
        );
    } else {
        panic!("Expected component definition");
    }
}
