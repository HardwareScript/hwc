use hwc_compiler::eval::*;
use hwc_parser::{DiagnosticCollector, Lexer, Parser};
use hwc_types::{UnitInfo, UnitRegistry};
use std::sync::Arc;

fn parse_program(source: &str) -> hwc_parser::ast::Program {
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let prog = parser.parse(&collector);
    assert_eq!(collector.error_count(), 0, "Parser errors occurred");
    prog
}

#[test]
fn test_dimensional_unit_algebra() {
    let l1 = Value::Measurement(MeasurementValue::from_unit_str(10.0, "um", None).unwrap());
    let l2 = Value::Measurement(MeasurementValue::from_unit_str(250.0, "nm", None).unwrap());

    // 10um + 250nm = 10.25um (10_250_000 pm)
    let sum = l1.add(&l2).expect("Addition should succeed");
    if let Value::Measurement(m) = sum {
        assert_eq!(m.dimension, UnitDimension::Length);
        assert_eq!(m.raw, 10_250_000);
    } else {
        panic!("Expected measurement");
    }

    // Scalar scaling: 150nm * 4 = 600nm (600_000 pm)
    let l3 = Value::Measurement(MeasurementValue::from_unit_str(150.0, "nm", None).unwrap());
    let scaled = l3.mul(&Value::Int(4)).expect("Scaling should succeed");
    if let Value::Measurement(m) = scaled {
        assert_eq!(m.raw, 600_000);
    } else {
        panic!("Expected measurement");
    }

    // Unit mismatch: 10um + 1.8V -> Error S22
    let v1 = Value::Measurement(MeasurementValue::from_unit_str(1.8, "V", None).unwrap());
    let err = l1.add(&v1);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("Unit Mismatch"));
}

#[test]
fn test_unit_registry_integration() {
    let units = vec![
        UnitInfo {
            symbol: "mA".into(),
            aliases: vec!["milliamp".into()],
            multiplier: Some(1e-3),
            dimension: "current".into(),
        },
        UnitInfo {
            symbol: "kOhm".into(),
            aliases: vec!["kohm".into()],
            multiplier: Some(1e3),
            dimension: "resistance".into(),
        },
    ];
    let registry = UnitRegistry::new(units);

    let current = MeasurementValue::from_unit_str(10.0, "mA", Some(&registry)).unwrap();
    assert_eq!(current.dimension, UnitDimension::Current);
    // 10 mA = 0.01 A = 10_000_000_000 pA
    assert_eq!(current.raw, 10_000_000_000);

    let res = MeasurementValue::from_unit_str(2.5, "kOhm", Some(&registry)).unwrap();
    assert_eq!(res.dimension, UnitDimension::Resistance);
    // 2.5 kOhm = 2500 Ohm = 2_500_000_000 uOhm
    assert_eq!(res.raw, 2_500_000_000);

    // Ohm's law multiplication: I * R -> V
    // 10mA * 2.5kOhm = 25V (25_000_000_000 nV)
    let v_val = Value::Measurement(current).mul(&Value::Measurement(res)).unwrap();
    if let Value::Measurement(v) = v_val {
        assert_eq!(v.dimension, UnitDimension::Voltage);
        assert_eq!(v.raw, 25_000_000_000);
    } else {
        panic!("Expected voltage measurement");
    }
}

#[test]
fn test_let_mutability_check() {
    let source = r#"
    space TestSpace {
        let pitch = 400nm
        pitch = 500nm
    }
    "#;

    let program = parse_program(source);
    let mut ctx = EvaluationContext::new();
    let mut evaluator = Evaluator::new(&mut ctx);
    let result = evaluator.eval_program(&program);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Error S14"));
}

#[test]
fn test_let_mut_allowed() {
    let source = r#"
    space TestSpace {
        let mut cur_x = 0nm
        cur_x += 400nm
        cur_x += 200nm
        assert(cur_x == 600nm)
    }
    "#;

    let program = parse_program(source);
    let mut ctx = EvaluationContext::new();
    let mut evaluator = Evaluator::new(&mut ctx);
    evaluator.eval_program(&program).expect("Evaluation should succeed");
}

#[test]
fn test_array_to_point2d_coercion() {
    let arr = Value::Array(Arc::new(vec![
        Value::Measurement(MeasurementValue::from_unit_str(10.0, "um", None).unwrap()),
        Value::Measurement(MeasurementValue::from_unit_str(5.0, "um", None).unwrap()),
    ]));

    let coerced = arr.coerce_to_point2d().expect("Coercion should succeed");
    assert_eq!(
        coerced,
        Value::Point2D {
            x: 10_000_000,
            y: 5_000_000
        }
    );
}

#[test]
fn test_pcell_geometric_emission() {
    let source = r#"
    export fn sky130_nmos(
        name: String,
        W: Measurement,
        L: Measurement,
        at: Point2D,
        source: Net,
        drain: Net,
        gate: Net,
        bulk: Net
    ) {
        # Active diff rect
        space.add_polygon(
            layer: "diff",
            net: source,
            rect: [at.x - 500nm, at.y - W/2, 1000nm, W]
        )

        # Contact
        space.add_contact(
            from: "diff",
            to: "li1",
            at: [at.x, at.y],
            diameter: 170nm,
            net: source
        )

        # Device model contract
        space.add_device(
            type: "NMOS",
            name: name,
            terminals: { S: source, D: drain, G: gate, B: bulk },
            params: { W: W, L: L }
        )
    }

    space InverterSpace {
        nets {
            VDD: { classification: power }
            VSS: { classification: ground }
            In:  { classification: signal }
            Out: { classification: signal }
        }

        let m1 = sky130_nmos(
            name: "M_NMOS",
            W: 1.0um,
            L: 150nm,
            at: [10.0um, 5.0um],
            source: VSS,
            drain: Out,
            gate: In,
            bulk: VSS
        )
    }
    "#;

    let program = parse_program(source);
    let memory_emitter = MemoryEmitter::new();
    let mut ctx = EvaluationContext::with_emitter(Box::new(memory_emitter));
    let mut evaluator = Evaluator::new(&mut ctx);

    evaluator.eval_program(&program).expect("Evaluation should succeed");
    let mem = ctx.emitter.as_any().downcast_ref::<MemoryEmitter>().unwrap();
    assert_eq!(mem.polygons.len(), 1);
    assert_eq!(mem.contacts.len(), 1);
    assert_eq!(mem.devices.len(), 1);
    assert_eq!(mem.devices[0].name.as_str(), "M_NMOS");
    assert_eq!(mem.devices[0].device_type.as_str(), "NMOS");
}

#[test]
fn test_sandbox_fuel_limit_guard() {
    let mut guard = DeterministicGuard::new(100, DEFAULT_MAX_MEMORY_BYTES);
    let mut err = None;
    for _ in 0..105 {
        if let Err(e) = guard.consume_step() {
            err = Some(e);
            break;
        }
    }
    assert!(err.is_some());
    assert!(matches!(
        err.unwrap(),
        SandboxError::FuelExhausted {
            fuel_consumed: 100,
            suggested_fuel: 200,
        }
    ));
}
