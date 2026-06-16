// Phase 5: Parallel Analyzer Execution Tests
// Tests for parallel physics validation using Rayon with Symbol Table integration

use hwc_compiler::SymbolTable;
use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{
    Identifier, MaterialCategory, MaterialDefinition, Measurement, Property, PropertyValue, Span,
    Unit,
};
use hwc_physics::PhysicsEngine;
use std::time::Instant;

// Helper function to create a test symbol table
fn create_test_symbol_table() -> SymbolTable {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();

    let copper = MaterialDefinition {
        name: Identifier::with_dummy_span("Copper"),
        category: MaterialCategory::Conductor,
        process: hwc_parser::ManufacturingProcess::default(),
        symbol: Some("Cu".into()),
        description: Some("Standard PCB trace material".into()),
        properties: vec![Property {
            key: "resistivity".into(),
            value: PropertyValue::Measurement(Measurement {
                value: 1.68e-8,
                unit: Unit::Custom("Ω·m".into()),
                span: Span::new(0, 10),
            }),
            span: Span::new(0, 10),
        }],
        span: Span::new(0, 100),
        color: None,
        opacity: None,
        outline_opacity: None,
        roughness: None,
        metallic: None,
        ior: None,
        clearcoat: None,
        clearcoat_roughness: None,
        subsurface: None,
        anisotropy: None,
        anisotropy_rotation: None,
        texture: None,
    };
    symbol_table.register_material(&collector, copper);
    symbol_table
}

#[test]
fn test_parallel_validation_returns_report() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();

    // Should return a valid report (even if empty for now)
    let report = engine.validate_design_parallel(&symbol_table, None);

    assert!(report.is_valid(), "Empty design should pass validation");
    assert_eq!(report.total_violations(), 0);
}

#[test]
fn test_parallel_validation_determinism() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();

    // Run validation multiple times
    let report1 = engine.validate_design_parallel(&symbol_table, None);
    let report2 = engine.validate_design_parallel(&symbol_table, None);
    let report3 = engine.validate_design_parallel(&symbol_table, None);

    // All reports should be identical (deterministic)
    assert_eq!(
        report1.total_violations(),
        report2.total_violations(),
        "Parallel validation should be deterministic"
    );
    assert_eq!(
        report2.total_violations(),
        report3.total_violations(),
        "Parallel validation should be deterministic"
    );
}

#[test]
fn test_sequential_vs_parallel_same_results() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();

    // Run both sequential and parallel validation
    let sequential_report = engine.validate_design(&symbol_table, None);
    let parallel_report = engine.validate_design_parallel(&symbol_table, None);

    // Results should be identical
    assert_eq!(
        sequential_report.total_violations(),
        parallel_report.total_violations(),
        "Sequential and parallel validation should produce same results"
    );

    assert_eq!(
        sequential_report.electrical_violations.len(),
        parallel_report.electrical_violations.len()
    );

    assert_eq!(
        sequential_report.thermal_violations.len(),
        parallel_report.thermal_violations.len()
    );

    assert_eq!(
        sequential_report.em_violations.len(),
        parallel_report.em_violations.len()
    );

    assert_eq!(
        sequential_report.clearance_violations.len(),
        parallel_report.clearance_violations.len()
    );
}

#[test]
fn test_parallel_validation_thread_safety() {
    use std::sync::Arc;
    use std::thread;

    let engine = Arc::new(PhysicsEngine::new());
    let symbol_table = Arc::new(create_test_symbol_table());
    let mut handles = vec![];

    // Spawn multiple threads running validation simultaneously
    for _ in 0..4 {
        let engine_clone = Arc::clone(&engine);
        let symbol_table_clone = Arc::clone(&symbol_table);
        let handle = thread::spawn(move || {
            engine_clone.validate_design_parallel(&*symbol_table_clone, None)
        });
        handles.push(handle);
    }

    // Collect all results
    let mut reports = vec![];
    for handle in handles {
        reports.push(handle.join().expect("Thread panicked"));
    }

    // All reports should be identical (thread-safe, deterministic)
    for i in 1..reports.len() {
        assert_eq!(
            reports[0].total_violations(),
            reports[i].total_violations(),
            "Parallel validation should be thread-safe"
        );
    }
}

#[test]
fn test_parallel_validation_performance_baseline() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();

    // Measure sequential validation time
    let start = Instant::now();
    let _report = engine.validate_design(&symbol_table, None);
    let sequential_duration = start.elapsed();

    // Measure parallel validation time
    let start = Instant::now();
    let _report = engine.validate_design_parallel(&symbol_table, None);
    let parallel_duration = start.elapsed();

    // For empty design, both should be very fast
    // This is just a baseline - real performance gains appear with actual data
    assert!(
        sequential_duration.as_micros() < 10_000,
        "Sequential validation should be fast for empty design"
    );
    assert!(
        parallel_duration.as_micros() < 10_000,
        "Parallel validation should be fast for empty design"
    );
}

#[test]
fn test_parallel_validation_read_only_access() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();

    // Run validation multiple times - should not modify engine state
    let report1 = engine.validate_design_parallel(&symbol_table, None);
    let report2 = engine.validate_design_parallel(&symbol_table, None);

    // Engine state should be unchanged (read-only access)
    assert_eq!(report1.is_valid(), report2.is_valid());
}

#[test]
fn test_parallel_validation_empty_report_structure() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();
    let report = engine.validate_design_parallel(&symbol_table, None);

    // Verify report structure
    assert!(report.electrical_violations.is_empty());
    assert!(report.thermal_violations.is_empty());
    assert!(report.em_violations.is_empty());
    assert!(report.clearance_violations.is_empty());
    assert_eq!(report.total_violations(), 0);
    assert!(report.is_valid());
}

#[test]
fn test_parallel_validation_report_format() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();
    let report = engine.validate_design_parallel(&symbol_table, None);

    let formatted = report.format_report();

    // Empty design should show success message
    assert!(formatted.contains("✓ Design passes all physics checks"));
}
