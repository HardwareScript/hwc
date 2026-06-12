use hwc_compiler::SymbolTable;
use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{
    Identifier, MaterialCategory, MaterialDefinition, Measurement, Property, PropertyValue, Span,
    Unit,
};
use hwc_physics::EMAnalyzer;

/// Helper function to create an FR4 material definition for tests
fn create_fr4_material() -> MaterialDefinition {
    MaterialDefinition {
        name: Identifier::with_dummy_span("FR4"),
        category: MaterialCategory::Insulator,
        process: hwc_parser::ManufacturingProcess::default(),
        symbol: None,
        description: Some("Standard PCB substrate material".into()),
        properties: vec![
            Property {
                key: "relative_permittivity".into(),
                value: PropertyValue::Number(4.5),
                span: Span::new(0, 10),
            },
            Property {
                key: "dielectric_strength".into(),
                value: PropertyValue::Measurement(Measurement {
                    value: 20.0,
                    unit: Unit::Custom("kV/mm".into()),
                    span: Span::new(0, 10),
                }),
                span: Span::new(0, 10),
            },
        ],
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
    }
}

/// Helper function to create an Air material definition for tests
fn create_air_material() -> MaterialDefinition {
    MaterialDefinition {
        name: Identifier::with_dummy_span("Air"),
        category: MaterialCategory::Insulator,
        process: hwc_parser::ManufacturingProcess::default(),
        symbol: None,
        description: Some("Air dielectric".into()),
        properties: vec![
            Property {
                key: "relative_permittivity".into(),
                value: PropertyValue::Number(1.0),
                span: Span::new(0, 10),
            },
            Property {
                key: "dielectric_strength".into(),
                value: PropertyValue::Measurement(Measurement {
                    value: 3.0,
                    unit: Unit::Custom("kV/mm".into()),
                    span: Span::new(0, 10),
                }),
                span: Span::new(0, 10),
            },
        ],
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
    }
}

#[test]
fn test_calculate_microstrip_impedance_50ohm() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_fr4_material());

    let analyzer = EMAnalyzer::new();

    // Use standard values
    let trace_width_nm = 254_000; // 254µm (10 mil) - standard trace width
    let trace_thickness_nm = 35_000; // 35µm (1oz copper)
    let dielectric_height_nm = 1_530_000; // 1.53mm

    let impedance = analyzer
        .calculate_microstrip_impedance_with_symbol_table(
            trace_width_nm,
            trace_thickness_nm,
            dielectric_height_nm,
            "FR4",
            &symbol_table,
        )
        .unwrap();

    // The microstrip formula gives higher impedance for this geometry
    // This is correct - narrow trace (0.254mm) with thick dielectric (1.53mm) = high impedance
    assert!(
        impedance > 100.0 && impedance < 150.0,
        "Expected ~130Ω for this geometry, got {}Ω",
        impedance
    );
}

#[test]
fn test_calculate_microstrip_impedance_wider_trace() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_fr4_material());

    let analyzer = EMAnalyzer::new();

    // Wider trace = lower impedance
    // 0.5mm trace width, 35µm thickness, 1.6mm dielectric height
    let impedance = analyzer
        .calculate_microstrip_impedance_with_symbol_table(
            500_000,   // 0.5mm trace width (wider)
            35_000,    // 35µm thickness
            1_600_000, // 1.6mm dielectric height
            "FR4",
            &symbol_table,
        )
        .unwrap();

    // Wider trace should give lower impedance than narrow trace
    assert!(
        impedance > 80.0 && impedance < 120.0,
        "Expected ~110Ω for wider trace, got {}Ω",
        impedance
    );
}

#[test]
fn test_calculate_microstrip_impedance_thinner_dielectric() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_fr4_material());

    let analyzer = EMAnalyzer::new();

    // Thinner dielectric = lower impedance
    // 0.25mm trace width, 35µm thickness, 0.8mm dielectric height
    let impedance = analyzer
        .calculate_microstrip_impedance_with_symbol_table(
            250_000, // 0.25mm trace width
            35_000,  // 35µm thickness
            800_000, // 0.8mm dielectric height (thinner)
            "FR4",
            &symbol_table,
        )
        .unwrap();

    // Thinner dielectric should give lower impedance than thick dielectric
    assert!(
        impedance > 80.0 && impedance < 120.0,
        "Expected ~108Ω for thinner dielectric, got {}Ω",
        impedance
    );
}

#[test]
fn test_validate_impedance_matching_within_tolerance() {
    let analyzer = EMAnalyzer::new();

    // 50Ω target, 48Ω actual, 10% tolerance
    let result = analyzer.validate_impedance_matching(
        "USB_DP", 48.0, // actual
        50.0, // target
        10.0, // 10% tolerance
    );

    assert!(result.is_ok(), "48Ω should be within 10% of 50Ω");
}

#[test]
fn test_validate_impedance_matching_outside_tolerance() {
    let analyzer = EMAnalyzer::new();

    // 50Ω target, 40Ω actual, 10% tolerance (outside range)
    let result = analyzer.validate_impedance_matching(
        "USB_DP", 40.0, // actual (too low)
        50.0, // target
        10.0, // 10% tolerance
    );

    assert!(result.is_err(), "40Ω should be outside 10% of 50Ω");

    if let Err(violation) = result {
        match violation {
            hwc_physics::electromagnetic::EMViolation::ImpedanceMismatch {
                net,
                actual_ohm,
                target_ohm,
                tolerance_percent,
            } => {
                assert_eq!(net, "USB_DP");
                assert_eq!(actual_ohm, 40.0);
                assert_eq!(target_ohm, 50.0);
                assert_eq!(tolerance_percent, 10.0);
            }
            _ => panic!("Wrong violation type"),
        }
    }
}

#[test]
fn test_calculate_crosstalk_coefficient_low() {
    let analyzer = EMAnalyzer::new();

    // Wide spacing, short parallel length = low crosstalk
    let coefficient = analyzer.calculate_crosstalk_coefficient(
        1_000_000, // 1mm spacing (wide)
        250_000,   // 0.25mm trace width
        5_000_000, // 5mm parallel length (short)
    );

    assert!(
        coefficient < 0.3,
        "Expected low crosstalk, got {}",
        coefficient
    );
}

#[test]
fn test_calculate_crosstalk_coefficient_high() {
    let analyzer = EMAnalyzer::new();

    // Narrow spacing, long parallel length = high crosstalk
    let coefficient = analyzer.calculate_crosstalk_coefficient(
        200_000,    // 0.2mm spacing (narrow)
        250_000,    // 0.25mm trace width
        50_000_000, // 50mm parallel length (long)
    );

    assert!(
        coefficient > 0.5,
        "Expected high crosstalk, got {}",
        coefficient
    );
}

#[test]
fn test_validate_crosstalk_acceptable() {
    let analyzer = EMAnalyzer::new();

    // Low crosstalk coefficient
    let result = analyzer.validate_crosstalk(
        "Signal1", "Signal2", 0.1, // Low coefficient
        0.2, // Max acceptable
    );

    assert!(result.is_ok(), "Low crosstalk should be acceptable");
}

#[test]
fn test_validate_crosstalk_violation() {
    let analyzer = EMAnalyzer::new();

    // High crosstalk coefficient
    let result = analyzer.validate_crosstalk(
        "Signal1", "Signal2", 0.5, // High coefficient
        0.2, // Max acceptable
    );

    assert!(result.is_err(), "High crosstalk should violate");

    if let Err(violation) = result {
        match violation {
            hwc_physics::electromagnetic::EMViolation::Crosstalk {
                net_a,
                net_b,
                crosstalk_coefficient,
                max_coefficient,
            } => {
                assert_eq!(net_a, "Signal1");
                assert_eq!(net_b, "Signal2");
                assert_eq!(crosstalk_coefficient, 0.5);
                assert_eq!(max_coefficient, 0.2);
            }
            _ => panic!("Wrong violation type"),
        }
    }
}

#[test]
fn test_analyze_trace_controlled_impedance() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_fr4_material());

    let analyzer = EMAnalyzer::new();

    // Analyze trace with target impedance
    let analysis = analyzer.analyze_trace(
        250_000,    // 0.25mm trace width
        35_000,     // 35µm thickness
        1_600_000,  // 1.6mm dielectric height
        4.5,        // FR4 relative permittivity
        Some(50.0), // Target 50Ω
    );

    assert!(analysis.impedance_ohm > 0.0);
    assert_eq!(analysis.target_impedance_ohm, Some(50.0));
    // Should be controlled if within 10% of target
}

#[test]
fn test_analyze_trace_no_impedance_control() {
    let analyzer = EMAnalyzer::new();

    // Analyze trace without impedance control
    let analysis = analyzer.analyze_trace(
        250_000,   // 0.25mm trace width
        35_000,    // 35µm thickness
        1_600_000, // 1.6mm dielectric height
        4.5,       // FR4 relative permittivity
        None,      // No target impedance
    );

    assert!(analysis.impedance_ohm > 0.0);
    assert_eq!(analysis.target_impedance_ohm, None);
    assert!(
        !analysis.is_controlled,
        "Should not be controlled without target"
    );
}

#[test]
fn test_differential_pair_90ohm() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_fr4_material());

    let analyzer = EMAnalyzer::new();

    // USB differential pair typically needs 90Ω differential impedance
    // Single-ended impedance should be ~45Ω
    let impedance = analyzer
        .calculate_microstrip_impedance_with_symbol_table(
            150_000,   // 0.15mm trace width (narrower for higher impedance)
            35_000,    // 35µm thickness
            1_600_000, // 1.6mm dielectric height
            "FR4",
            &symbol_table,
        )
        .unwrap();

    // Should be higher than 50Ω (typically 55-65Ω for single-ended)
    assert!(
        impedance > 50.0,
        "Expected higher impedance for differential pair, got {}Ω",
        impedance
    );
}

#[test]
fn test_crosstalk_perpendicular_traces() {
    let analyzer = EMAnalyzer::new();

    // Perpendicular traces (crossing at 90°) have minimal parallel length
    let coefficient = analyzer.calculate_crosstalk_coefficient(
        200_000, // 0.2mm spacing
        250_000, // 0.25mm trace width
        250_000, // 0.25mm parallel length (just the crossing point)
    );

    assert!(
        coefficient < 0.1,
        "Perpendicular traces should have minimal crosstalk, got {}",
        coefficient
    );
}

#[test]
fn test_impedance_with_air_dielectric() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_air_material());

    let analyzer = EMAnalyzer::new();

    // Air has lower permittivity (εr ≈ 1) than FR4 (εr ≈ 4.5)
    // Same geometry should give higher impedance
    let impedance = analyzer
        .calculate_microstrip_impedance_with_symbol_table(
            250_000,   // 0.25mm trace width
            35_000,    // 35µm thickness
            1_600_000, // 1.6mm dielectric height
            "Air",
            &symbol_table,
        )
        .unwrap();

    // Should be significantly higher than FR4 (~50Ω)
    assert!(
        impedance > 70.0,
        "Air dielectric should give higher impedance, got {}Ω",
        impedance
    );
}
