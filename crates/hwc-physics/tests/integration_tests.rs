/// Physics engine integration tests for Phase 5.
///
/// These tests validate the unified physics validation with Symbol Table integration.
use hwc_compiler::SymbolTable;
use hwc_diagnostics::DiagnosticCollector;
use hwc_engine::constraint_manager::calculate_trace_width_nm;
use hwc_parser::{
    Identifier, MaterialCategory, MaterialDefinition, Measurement, Property, PropertyValue, Span,
    Unit,
};
use hwc_physics::{
    clearance::{ClearanceAnalyzer, ClearanceViolation},
    electrical::{ElectricalAnalyzer, ElectricalViolation},
    electromagnetic::EMAnalyzer,
    thermal::ThermalAnalyzer,
    PhysicsEngine, PhysicsReport,
};

// ============================================================================
// Test Helper Functions
// ============================================================================

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

// ============================================================================
// Phase 5: PhysicsEngine with Symbol Table Tests
// ============================================================================

#[test]
fn test_physics_engine_creation() {
    let engine = PhysicsEngine::new();
    assert!(
        engine
            .electrical
            .calculate_trace_resistance(1_000_000, 500_000, 35_000, 1.68e-8)
            > 0.0
    );
}

#[test]
fn test_physics_engine_default() {
    let engine = PhysicsEngine::default();
    assert!(
        engine
            .electrical
            .calculate_trace_resistance(1_000_000, 500_000, 35_000, 1.68e-8)
            > 0.0
    );
}

#[test]
fn test_validate_design_empty() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();
    let report = engine.validate_design(&symbol_table, None);
    assert!(report.is_valid());
}

#[test]
fn test_validate_design_parallel_empty() {
    let engine = PhysicsEngine::new();
    let symbol_table = create_test_symbol_table();
    let report = engine.validate_design_parallel(&symbol_table, None);
    assert!(report.is_valid());
}

#[test]
fn test_physics_report_creation() {
    let report = PhysicsReport::new();
    assert!(report.is_valid());
    assert_eq!(report.total_violations(), 0);
}

#[test]
fn test_physics_report_with_violations() {
    let mut report = PhysicsReport::new();

    report
        .electrical_violations
        .push(ElectricalViolation::Ampacity {
            net: "trace1".into(),
            current_ma: 5000,
            required_width_nm: 2_500_000,
            actual_width_nm: 500_000,
        });

    assert!(!report.is_valid());
    assert_eq!(report.total_violations(), 1);
}

#[test]
fn test_physics_report_format() {
    let report = PhysicsReport::new();
    let formatted = report.format_report();
    assert!(formatted.contains("✓ Design passes all physics checks"));
}

#[test]
fn test_physics_report_to_errors() {
    let mut report = PhysicsReport::new();

    report
        .clearance_violations
        .push(ClearanceViolation::DielectricBreakdown {
            net_a: "VCC".into(),
            net_b: "GND".into(),
            voltage_diff_mv: 120_000,
            actual_clearance_nm: 50_000,
            required_clearance_nm: 80_000,
            material: "air".into(),
        });

    let errors = report.to_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "P16");
}

#[test]
fn test_high_current_power_trace() {
    let _analyzer = ElectricalAnalyzer::new();
    // 10A power trace - calculate required width using IPC-2221
    let width = calculate_trace_width_nm(10_000, 10, true);
    // IPC-2221 is conservative - 10A requires very wide trace (>50mm)
    assert!(width > 50_000_000);
}

#[test]
fn test_high_speed_differential_pair() {
    let analyzer = EMAnalyzer::new();
    // Test parameters: 250µm trace, 1.6mm height, 35µm thickness, εr=4.5
    let impedance = analyzer.calculate_microstrip_impedance(250_000, 35_000, 1_600_000, 4.5);
    // Verify impedance is a valid number (not NaN or infinite)
    assert!(impedance.is_finite());
    // Microstrip impedance should be positive
    assert!(impedance > 0.0);
}

#[test]
fn test_clearance_high_voltage() {
    let analyzer = ClearanceAnalyzer::new();
    let clearance = analyzer.calculate_required_clearance(400_000, 3.0, 2.0);
    assert!(clearance > 250_000 && clearance < 300_000);
}

#[test]
fn test_thermal_analysis_high_power() {
    let analyzer = ThermalAnalyzer::new();
    let temp_rise = analyzer.calculate_temperature_rise(5000.0, 10_000_000, 500_000, 35_000, 401.0);
    assert!(temp_rise > 10.0);
}
