//! Tests for mechanical, interface, and test definition parsing

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, Parser, Value};

#[test]
fn test_parse_minimal_mechanical() {
    let source = r#"
mechanical SimpleEnclosure:
    dimensions: 100mm by 50mm by 25mm
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Mechanical(mech) = &program.definitions[0] {
        assert_eq!(mech.name.as_str(), "SimpleEnclosure");
        assert!(mech.dimensions.is_some());
        assert_eq!(mech.mounting_holes.len(), 0);
        assert_eq!(mech.keepouts.len(), 0);
    } else {
        panic!("Expected mechanical definition");
    }
}

#[test]
fn test_parse_mechanical_with_mounting_holes() {
    let source = r#"
mechanical RobotEnclosure:
    dimensions: 150mm by 100mm by 50mm
    mounting_holes:
        - at [x:5mm, y:5mm, z:1] diameter 3mm
        - at [x:145mm, y:95mm, z:1] diameter 3mm
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Mechanical(mech) = &program.definitions[0] {
        assert_eq!(mech.name.as_str(), "RobotEnclosure");
        assert!(mech.dimensions.is_some());
        assert_eq!(mech.mounting_holes.len(), 2);
        assert_eq!(mech.keepouts.len(), 0);

        // Check first mounting hole
        let hole1 = &mech.mounting_holes[0];
        let (x, y, _z) = hole1.position.evaluate_const().expect("Failed to evaluate");
        // With measurements, x and y are Measurement values
        match x {
            Value::Measurement { value, .. } => assert_eq!(value, 5.0),
            Value::Number(n) => assert_eq!(n, 5),
            _ => panic!("Expected measurement or number for x"),
        }
        match y {
            Value::Measurement { value, .. } => assert_eq!(value, 5.0),
            Value::Number(n) => assert_eq!(n, 5),
            _ => panic!("Expected measurement or number for y"),
        }
    } else {
        panic!("Expected mechanical definition");
    }
}

#[test]
fn test_parse_mechanical_with_keepouts() {
    let source = r#"
mechanical ComplexEnclosure:
    dimensions: 150mm by 100mm by 50mm
    keepout:
        - region [x:20mm, y:20mm, z:1] to [x:60mm, y:60mm, z:1] height 15mm
        - region [x:80mm, y:30mm, z:1] to [x:120mm, y:70mm, z:1]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Mechanical(mech) = &program.definitions[0] {
        assert_eq!(mech.name.as_str(), "ComplexEnclosure");
        assert_eq!(mech.keepouts.len(), 2);

        // Check first keepout
        let keepout1 = &mech.keepouts[0];
        let (from_x, from_y, _) = keepout1.from.evaluate_const().expect("Failed to evaluate");
        let (to_x, to_y, _) = keepout1.to.evaluate_const().expect("Failed to evaluate");

        // With measurements, coordinates are Measurement values
        match from_x {
            Value::Measurement { value, .. } => assert_eq!(value, 20.0),
            Value::Number(n) => assert_eq!(n, 20),
            _ => panic!("Expected measurement or number"),
        }
        match from_y {
            Value::Measurement { value, .. } => assert_eq!(value, 20.0),
            Value::Number(n) => assert_eq!(n, 20),
            _ => panic!("Expected measurement or number"),
        }
        match to_x {
            Value::Measurement { value, .. } => assert_eq!(value, 60.0),
            Value::Number(n) => assert_eq!(n, 60),
            _ => panic!("Expected measurement or number"),
        }
        match to_y {
            Value::Measurement { value, .. } => assert_eq!(value, 60.0),
            Value::Number(n) => assert_eq!(n, 60),
            _ => panic!("Expected measurement or number"),
        }
        assert!(keepout1.height.is_some());

        // Check second keepout (no height)
        let keepout2 = &mech.keepouts[1];
        assert!(keepout2.height.is_none());
    } else {
        panic!("Expected mechanical definition");
    }
}

#[test]
fn test_parse_minimal_interface() {
    let source = r#"
interface SimpleController:
    target: "ESP32_WROOM_32"
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Interface(iface) = &program.definitions[0] {
        assert_eq!(iface.name.as_str(), "SimpleController");
        assert_eq!(
            iface.target.as_ref().map(|t| t.as_str()),
            Some("ESP32_WROOM_32")
        );
        assert_eq!(iface.bindings.len(), 0);
        assert_eq!(iface.protocols.len(), 0);
    } else {
        panic!("Expected interface definition");
    }
}

#[test]
fn test_parse_interface_with_bindings() {
    let source = r#"
interface RobotController:
    target: "ESP32_WROOM_32"
    bindings:
        Motor_PWM = DriverIC.Pin_4
        Status_LED = LED1.Anode
        Temp_Sensor = Thermistor.Out
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Interface(iface) = &program.definitions[0] {
        assert_eq!(iface.name.as_str(), "RobotController");
        assert_eq!(iface.bindings.len(), 3);

        // Check first binding
        let binding1 = &iface.bindings[0];
        assert_eq!(binding1.signal_name, "Motor_PWM");
        assert_eq!(binding1.pin_ref.component, "DriverIC");
        assert_eq!(binding1.pin_ref.pin, "Pin_4");
    } else {
        panic!("Expected interface definition");
    }
}

#[test]
fn test_parse_interface_with_protocols() {
    let source = r#"
interface I2CController:
    target: "MCU"
    protocols:
        I2C_Bus_1:
            SDA: MCU.GPIO21
            SCL: MCU.GPIO22
            speed: 400kHz
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Interface(iface) = &program.definitions[0] {
        assert_eq!(iface.name.as_str(), "I2CController");
        assert_eq!(iface.protocols.len(), 1);

        let protocol = &iface.protocols[0];
        assert_eq!(protocol.name.as_str(), "I2C_Bus_1");
        assert_eq!(protocol.pins.len(), 2);
        assert!(protocol.speed.is_some());

        // Check pins
        assert_eq!(protocol.pins[0].signal, "SDA");
        assert_eq!(protocol.pins[0].pin_ref.component, "MCU");
        assert_eq!(protocol.pins[0].pin_ref.pin, "GPIO21");
    } else {
        panic!("Expected interface definition");
    }
}

#[test]
fn test_parse_minimal_test() {
    let source = r#"
test BasicTest:
    setup:
        apply 12V to PowerSource.VIN
    execute:
        wait 1ms
    assert:
        PowerSource.VOUT < 13V
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Test(test) = &program.definitions[0] {
        assert_eq!(test.name.as_str(), "BasicTest");
        assert_eq!(test.setup.len(), 1);
        assert_eq!(test.execute.len(), 1);
        assert_eq!(test.assertions.len(), 1);
    } else {
        panic!("Expected test definition");
    }
}

#[test]
fn test_parse_test_with_all_actions() {
    let source = r#"
test Short_Circuit_Protection:
    setup:
        apply 12V to PowerSource.VIN
        apply 0V to PowerSource.GND
    execute:
        short Regulator.VOUT to GND
        wait 1ms
    assert:
        Regulator.VOUT < 0.5V
        Regulator.temperature < 100C
        PowerSource.current < 2A
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Test(test) = &program.definitions[0] {
        assert_eq!(test.name.as_str(), "Short_Circuit_Protection");
        assert_eq!(test.setup.len(), 2);
        assert_eq!(test.execute.len(), 2);
        assert_eq!(test.assertions.len(), 3);

        // Check setup actions
        use hwc_parser::ast::TestActionType;
        if let TestActionType::Apply { voltage, pin } = &test.setup[0].action_type {
            assert_eq!(voltage.value, 12.0);
            assert_eq!(pin.component, "PowerSource");
            assert_eq!(pin.pin, "VIN");
        } else {
            panic!("Expected Apply action");
        }

        // Check execute actions
        if let TestActionType::Short { from, to } = &test.execute[0].action_type {
            assert_eq!(from.component, "Regulator");
            assert_eq!(from.pin, "VOUT");
            assert_eq!(to.component, "");
            assert_eq!(to.pin, "GND");
        } else {
            panic!("Expected Short action");
        }

        // Check assertions
        use hwc_parser::ast::TestCondition;
        if let TestCondition::LessThan(value) = &test.assertions[0].condition {
            assert_eq!(value.value, 0.5);
        } else {
            panic!("Expected LessThan condition");
        }
    } else {
        panic!("Expected test definition");
    }
}

#[test]
fn test_parse_mechanical_mounting_holes_optional_z() {
    let source = r#"
mechanical SimpleEnclosure:
    dimensions: 100mm by 100mm by 20mm
    mounting_holes:
        - at [x:5mm, y:5mm] diameter 3mm
        - at [x:95mm, y:95mm] diameter 3mm
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");

    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.definitions.len(), 1);

    if let hwc_parser::ast::Definition::Mechanical(mech) = &program.definitions[0] {
        assert_eq!(mech.name.as_str(), "SimpleEnclosure");
        assert_eq!(mech.mounting_holes.len(), 2);

        // Check first mounting hole - z should default to 0
        let hole1 = &mech.mounting_holes[0];
        let (x, y, z) = hole1.position.evaluate_const().expect("Failed to evaluate");

        match x {
            Value::Measurement { value, .. } => assert_eq!(value, 5.0),
            Value::Number(n) => assert_eq!(n, 5),
            _ => panic!("Expected measurement or number for x"),
        }
        match y {
            Value::Measurement { value, .. } => assert_eq!(value, 5.0),
            Value::Number(n) => assert_eq!(n, 5),
            _ => panic!("Expected measurement or number for y"),
        }
        assert_eq!(z, Value::Number(0)); // Should default to 0 when not specified

        // Check second mounting hole
        let hole2 = &mech.mounting_holes[1];
        let (x, y, z) = hole2.position.evaluate_const().expect("Failed to evaluate");

        match x {
            Value::Measurement { value, .. } => assert_eq!(value, 95.0),
            Value::Number(n) => assert_eq!(n, 95),
            _ => panic!("Expected measurement or number for x"),
        }
        match y {
            Value::Measurement { value, .. } => assert_eq!(value, 95.0),
            Value::Number(n) => assert_eq!(n, 95),
            _ => panic!("Expected measurement or number for y"),
        }
        assert_eq!(z, Value::Number(0)); // Should default to 0 when not specified
    } else {
        panic!("Expected mechanical definition");
    }
}
