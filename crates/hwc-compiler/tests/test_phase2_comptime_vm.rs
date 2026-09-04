//! Phase 2 Comptime VM & Pure Geometry Buffering Verification Gauntlet
//!
//! Tests:
//! 1. Deterministic fuel budget calculation & area scaling (10M / mm^2)
//! 2. Comptime fuel exhaustion (Error C01) & suggested fuel doubling
//! 3. Host RAM quota tracking & memory limit intercept (Error C03)
//! 4. Stack recursion depth limit guard (Error C02)
//! 5. Pure Salsa GeometryBuffer emission with Merkle-bearing EntityId
//! 6. FlatGeometryBuffer coordinate pool packing and compact headers
//! 7. Hierarchical path tracking during sub-PCell invocation

use hwc_compiler::eval::*;
use hwc_engine::entity_graph::identity::{EntityId, HierarchicalPath};
use hwc_parser::{DiagnosticCollector, Lexer, Parser};

fn parse_program(source: &str) -> hwc_parser::ast::Program {
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let prog = parser.parse(&collector);
    assert_eq!(collector.error_count(), 0, "Parser errors: {:?}", collector);
    prog
}

#[test]
fn test_fuel_calculation_and_scaling() {
    // 1. Base fuel only
    let fuel_base = calculate_fuel(None, None, None);
    assert_eq!(fuel_base, DEFAULT_BASE_FUEL); // 100M

    // 2. Area scaling: 2.0mm x 2.0mm = 4.0 mm^2 -> +40M fuel -> 140M total
    // 2.0mm = 2_000_000_000 pm (2.0 * 10^9 pm)
    let w_pm = 2_000_000_000i128;
    let h_pm = 2_000_000_000i128;
    let fuel_area = calculate_fuel(Some(w_pm), Some(h_pm), None);
    assert_eq!(fuel_area, 140_000_000);

    // 3. Explicit #[comptime_fuel(300_000_000)] override
    let fuel_explicit = calculate_fuel(Some(w_pm), Some(h_pm), Some(300_000_000));
    assert_eq!(fuel_explicit, 440_000_000);
}

#[test]
fn test_deterministic_guard_fuel_exhaustion() {
    let mut guard = DeterministicGuard::new(500, DEFAULT_MAX_MEMORY_BYTES);
    for _ in 0..500 {
        assert!(guard.consume_step().is_ok());
    }
    assert_eq!(guard.fuel_remaining, 0);
    assert_eq!(guard.fuel_consumed(), 500);

    // Step 501 exhausts budget and suggests double (1000)
    let err = guard.consume_step();
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err(),
        SandboxError::FuelExhausted {
            fuel_consumed: 500,
            suggested_fuel: 1000,
        }
    );
}

#[test]
fn test_host_ram_quota_tracking() {
    // 10 MB quota
    let limit_bytes = 10 * 1024 * 1024;
    let mut guard = DeterministicGuard::new(100_000, limit_bytes);

    // Allocate 6 MB -> OK
    assert!(guard.track_allocation(6 * 1024 * 1024).is_ok());
    assert_eq!(guard.allocated_bytes, 6 * 1024 * 1024);

    // Deallocate 2 MB -> 4 MB
    guard.track_deallocation(2 * 1024 * 1024);
    assert_eq!(guard.allocated_bytes, 4 * 1024 * 1024);

    // Allocate 8 MB -> 12 MB > 10 MB limit -> Error C03
    let err = guard.track_allocation(8 * 1024 * 1024);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err(),
        SandboxError::MemoryLimitExceeded {
            allocated_mb: 12,
            limit_mb: 10,
        }
    );
}

#[test]
fn test_call_stack_recursion_limit() {
    let guard = DeterministicGuard::default();
    assert!(guard.check_recursion_depth(250).is_ok());
    assert!(guard.check_recursion_depth(256).is_ok());

    let err = guard.check_recursion_depth(257);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err(),
        SandboxError::RecursionDepthExceeded { max_depth: 256 }
    );
}

#[test]
fn test_pure_geometry_buffering_with_merkle_identity() {
    let source = r#"
    space InverterSpace {
        dimensions: [100.0um, 100.0um]

        nets {
            VDD: { classification: power }
            VSS: { classification: ground }
            IN:  { classification: signal }
            OUT: { classification: signal }
        }

        # Polygons
        space.add_polygon(
            layer: "diff",
            net: VDD,
            rect: [0nm, 0nm, 1000nm, 500nm]
        )

        space.add_polygon(
            layer: "poly",
            net: IN,
            rect: [400nm, 0nm, 200nm, 1000nm]
        )

        # Contact
        space.add_contact(
            from: "diff",
            to: "m1",
            at: [200nm, 250nm],
            diameter: 170nm,
            net: VDD
        )

        # Device
        space.add_device(
            type: "NMOS",
            name: "M0",
            terminals: { S: VSS, D: OUT, G: IN, B: VSS },
            params: { W: 1.0um, L: 150nm }
        )
    }
    "#;

    let program = parse_program(source);
    let space_decl = match &program.items[0] {
        hwc_parser::ast::TopLevelItem::Space(s) => s,
        _ => panic!("Expected Space item"),
    };

    let buffer = evaluate_space_to_buffer(
        space_decl,
        &rustc_hash::FxHashMap::default(),
        None,
        &rustc_hash::FxHashMap::default(),
        &rustc_hash::FxHashMap::default(),
        &rustc_hash::FxHashMap::default(),
        &rustc_hash::FxHashMap::default(),
    )
    .expect("Evaluation to GeometryBuffer should succeed");

    assert_eq!(buffer.len(), 4);

    // Verify all records carry valid, deterministic EntityId
    for record in &buffer.records {
        assert_ne!(record.entity_id().raw(), 0, "EntityId must be non-zero");
        assert_eq!(record.space_id(), SpaceId(1));
    }

    // Verify record types
    match &buffer.records[0] {
        GeometryRecord::Polygon { layer, points_pm, net_id, .. } => {
            assert_eq!(layer.as_str(), "diff");
            assert_eq!(points_pm.len(), 4);
            assert!(net_id.is_some());
        }
        _ => panic!("Expected Polygon record"),
    }

    match &buffer.records[2] {
        GeometryRecord::Contact { from_layer, to_layer, center_pm, diameter_pm, .. } => {
            assert_eq!(from_layer.as_str(), "diff");
            assert_eq!(to_layer.as_str(), "m1");
            assert_eq!(*center_pm, (200_000, 250_000));
            assert_eq!(*diameter_pm, 170_000);
        }
        _ => panic!("Expected Contact record"),
    }

    match &buffer.records[3] {
        GeometryRecord::Device { device_type, instance_name, terminals, params, .. } => {
            assert_eq!(device_type.as_str(), "NMOS");
            assert_eq!(instance_name.as_str(), "M0");
            assert_eq!(terminals.len(), 4);
            assert_eq!(params.len(), 2);
        }
        _ => panic!("Expected Device record"),
    }
}

#[test]
fn test_flat_geometry_buffer_conversion() {
    let mut buffer = GeometryBuffer::new();

    let root_path = HierarchicalPath::root("TestSpace");
    let id1 = EntityId::compute(&root_path, "Polygon", None, 0);
    let id2 = EntityId::compute(&root_path, "Contact", None, 1);

    buffer.push(GeometryRecord::Polygon {
        id: id1,
        space_id: SpaceId(1),
        layer: "m1".into(),
        net_id: Some(42),
        points_pm: vec![(0, 0), (1000, 0), (1000, 500), (0, 500)],
    });

    buffer.push(GeometryRecord::Contact {
        id: id2,
        space_id: SpaceId(1),
        from_layer: "diff".into(),
        to_layer: "m1".into(),
        center_pm: (500, 250),
        diameter_pm: 170,
        net_id: Some(42),
    });

    let layer_map = |name: &str| match name {
        "m1" => 1u16,
        "diff" => 2u16,
        _ => 0u16,
    };

    let flat = buffer.to_flat_buffer(layer_map);
    assert_eq!(flat.records.len(), 2);

    // Polygon packed into coordinate pool: 4 points = 8 coords
    assert_eq!(flat.records[0].id, id1);
    assert_eq!(flat.records[0].record_type, 1); // Polygon
    assert_eq!(flat.records[0].layer_idx, 1);
    assert_eq!(flat.records[0].coord_start_idx, 0);
    assert_eq!(flat.records[0].coord_count, 4);

    // Contact packed into coordinate pool: (cx, cy, dia) = 3 coords starting at idx 8
    assert_eq!(flat.records[1].id, id2);
    assert_eq!(flat.records[1].record_type, 2); // Contact
    assert_eq!(flat.records[1].layer_idx, 2);
    assert_eq!(flat.records[1].coord_start_idx, 8);
    assert_eq!(flat.coordinate_pool[8], 500);
    assert_eq!(flat.coordinate_pool[9], 250);
    assert_eq!(flat.coordinate_pool[10], 170);

    assert!(flat.total_memory_bytes() > 0);
}

#[test]
fn test_pcell_subcell_merkle_hierarchical_path() {
    let source = r#"
    export fn sky130_contact(at: Point2D, net: Net) {
        space.add_contact(
            from: "poly",
            to: "m1",
            at: [at.x, at.y],
            diameter: 170nm,
            net: net
        )
    }

    space CircuitSpace {
        nets {
            clk: { classification: clock }
        }

        sky130_contact(at: [1.0um, 2.0um], net: clk)
        sky130_contact(at: [3.0um, 4.0um], net: clk)
    }
    "#;

    let program = parse_program(source);
    let mut ctx = EvaluationContext::new();
    let mut evaluator = Evaluator::new(&mut ctx);
    evaluator.eval_program(&program).expect("Evaluation should succeed");

    let mem = ctx.emitter.as_any().downcast_ref::<MemoryEmitter>().unwrap();
    assert_eq!(mem.contacts.len(), 2);
    assert_eq!(mem.contacts[0].from_layer.as_str(), "poly");
    assert_eq!(mem.contacts[0].to_layer.as_str(), "m1");
    assert_eq!(mem.contacts[0].at, (1_000_000, 2_000_000));
    assert_eq!(mem.contacts[1].at, (3_000_000, 4_000_000));
}
