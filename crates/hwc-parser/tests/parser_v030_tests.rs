use hwc_parser::{DiagnosticCollector, Lexer, Parser, TopLevelItem, Statement, Expression, BinaryOperator};

#[test]
fn test_parse_canonical_cmos_inverter() {
    let source = r#"
# Import certified generators from @std
import { sky130_nmos, sky130_pmos, sky130_tap, pad, route_strap } from @std/pdk/sky130
import * from @std/primitives/units

module CMOS_Inverter {
    pins: [input In, output Out, power VDD, ground VSS]
}

space CMOS_Inverter_Space implements CMOS_Inverter {
    dimensions: [20.0um, 18.0um]
    profile: SKY130_1V8_CMOS

    nets {
        VDD: { classification: power, potential: 1.8V, current: 20.0uA }
        VSS: { classification: ground, potential: 0.0V, current: 20.0uA }
        In:  { classification: signal, potential: 1.8V, current: 0.1uA }
        Out: { classification: signal, current: 20.0uA }
    }

    let nmos = sky130_nmos(
        name: "M_NMOS", W: 1.0um, L: 150nm, at: [10.0um, 5.0um],
        source: VSS, drain: Out, gate: In, bulk: VSS
    )

    let pmos = sky130_pmos(
        name: "M_PMOS", W: 2.0um, L: 150nm, at: [10.0um, 10.5um],
        source: VDD, drain: Out, gate: In, bulk: VDD
    )

    route sub_tap.port to nmos.source { intent: Power }
    route well_tap.port to pmos.source { intent: Power }
}

test CMOS_Inverter_VTC_Test for CMOS_Inverter_Space {
    dc: { sweep: In, start: 0.0V, stop: 1.8V, step: 0.02V }
    tran: { step: 5ps, stop: 10ns }
}
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);

    assert_eq!(collector.error_count(), 0, "Errors found: {:?}", collector.error_count());
    assert_eq!(program.imports.len(), 2);
    assert_eq!(program.items.len(), 3); // Module, Space, Test
}

#[test]
fn test_parse_turing_parametric_nmos_function() {
    let source = r#"
export fn sky130_nmos(
    name: String,
    W: Measurement,
    L: Measurement,
    at: Point2D,
    source: Net,
    drain: Net,
    gate: Net,
    bulk: Net,
    sd_len: Measurement = 750nm
) -> NMOSLayout {
    let diff_len = (2 * sd_len) + L
    let mut num_vias = 1

    if W > 5.0um and L == 150nm {
        println("Wide driver instance detected")
    }

    for i in 0..num_vias {
        let vy = at.y + (i * 400nm)
        space.add_contact(from: "diff", to: "li1", at: [at.x, vy], diameter: 170nm, net: source)
    }

    return NMOSLayout {
        source: TransistorPort { x: at.x, y: at.y, layer: "metal1", net: source },
        num_vias: num_vias
    }
}
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);

    assert_eq!(collector.error_count(), 0);
    assert_eq!(program.items.len(), 1);

    if let TopLevelItem::Function(f) = &program.items[0] {
        assert_eq!(f.name.name.as_str(), "sky130_nmos");
        assert!(f.is_exported);
        assert_eq!(f.parameters.len(), 9);
        assert_eq!(f.body.statements.len(), 5);
    } else {
        panic!("Expected Function declaration");
    }
}

#[test]
fn test_pratt_expression_precedence() {
    let source = "
    fn test_expr() {
        let res = a + b * c == d and not e or f
    }
    ";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing should succeed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let program = parser.parse(&collector);

    assert_eq!(collector.error_count(), 0);
    if let TopLevelItem::Function(f) = &program.items[0] {
        if let Statement::Let { value, .. } = &f.body.statements[0] {
            if let Expression::Binary { operator, .. } = value {
                assert_eq!(*operator, BinaryOperator::Or);
            } else {
                panic!("Root operator should be Or");
            }
        }
    }
}
