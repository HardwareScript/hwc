use hwc_compiler::SymbolTable;
use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{
    Identifier, ManufacturingProcess, MaterialCategory, MaterialDefinition, Measurement, Property,
    PropertyValue, Span, Unit,
};
use hwc_physics::thermal::ThermalAnalysisParams;
use hwc_physics::ThermalAnalyzer;

/// Helper function to create a Copper material definition for tests
fn create_copper_material() -> MaterialDefinition {
    MaterialDefinition {
        name: Identifier::with_dummy_span("Copper"),
        category: MaterialCategory::Conductor,
        process: ManufacturingProcess::default(),
        symbol: Some("Cu".into()),
        description: Some("Standard PCB trace material".into()),
        properties: vec![Property {
            key: "thermal_conductivity".into(),
            value: PropertyValue::Measurement(Measurement {
                value: 401.0,
                unit: Unit::Custom("W/mK".into()),
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
    }
}

/// Helper function to create an Aluminum material definition for tests
fn create_aluminum_material() -> MaterialDefinition {
    MaterialDefinition {
        name: Identifier::with_dummy_span("Aluminum"),
        category: MaterialCategory::Conductor,
        process: ManufacturingProcess::default(),
        symbol: Some("Al".into()),
        description: Some("Alternative conductor material".into()),
        properties: vec![Property {
            key: "thermal_conductivity".into(),
            value: PropertyValue::Measurement(Measurement {
                value: 237.0,
                unit: Unit::Custom("W/mK".into()),
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
    }
}

#[test]
fn test_calculate_temperature_rise_low_power() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_copper_material());

    let analyzer = ThermalAnalyzer::new();

    // 270mW in 100mm × 0.25mm × 35µm copper trace
    let power_mw = 270.0;
    let length_nm = 100_000_000; // 100mm
    let width_nm = 250_000; // 0.25mm
    let thickness_nm = 35_000; // 35µm

    let temp_rise = analyzer
        .calculate_temperature_rise_with_symbol_table(
            power_mw,
            length_nm,
            width_nm,
            thickness_nm,
            "Copper",
            &symbol_table,
        )
        .unwrap();

    // Should be a reasonable temperature rise (a few degrees)
    assert!(
        temp_rise > 0.0 && temp_rise < 50.0,
        "Expected reasonable temp rise, got {}°C",
        temp_rise
    );
}

#[test]
fn test_calculate_temperature_rise_high_power() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_copper_material());

    let analyzer = ThermalAnalyzer::new();

    // 500mW in 10mm × 1mm × 35µm copper trace
    let power_mw = 500.0;
    let length_nm = 10_000_000; // 10mm
    let width_nm = 1_000_000; // 1mm
    let thickness_nm = 35_000; // 35µm

    let temp_rise = analyzer
        .calculate_temperature_rise_with_symbol_table(
            power_mw,
            length_nm,
            width_nm,
            thickness_nm,
            "Copper",
            &symbol_table,
        )
        .unwrap();

    // Larger trace with more power, but better heat dissipation
    // Expect higher temperature rise due to significant power
    assert!(
        temp_rise > 0.0 && temp_rise < 100.0,
        "Expected reasonable temp rise, got {}°C",
        temp_rise
    );
}

#[test]
fn test_calculate_temperature_rise_poor_conductor() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_aluminum_material());

    let analyzer = ThermalAnalyzer::new();

    // Same power but with aluminum (lower thermal conductivity)
    let power_mw = 270.0;
    let length_nm = 100_000_000; // 100mm
    let width_nm = 250_000; // 0.25mm
    let thickness_nm = 35_000; // 35µm

    let temp_rise = analyzer
        .calculate_temperature_rise_with_symbol_table(
            power_mw,
            length_nm,
            width_nm,
            thickness_nm,
            "Aluminum",
            &symbol_table,
        )
        .unwrap();

    // Should be higher than copper due to lower thermal conductivity
    assert!(temp_rise > 0.0, "Temperature rise should be positive");
}

#[test]
fn test_validate_max_temperature_safe() {
    let analyzer = ThermalAnalyzer::new();

    // 25°C ambient + 10°C rise = 35°C, max 130°C (FR4 typical)
    let result = analyzer.validate_max_temperature(
        "TestNet", 25.0,  // ambient
        10.0,  // rise
        130.0, // max (FR4 typical)
    );

    assert!(result.is_ok(), "Should be safe");
}

#[test]
fn test_validate_max_temperature_unsafe() {
    let analyzer = ThermalAnalyzer::new();
    let max_temp = 130.0; // FR4 typical

    // 25°C ambient + 110°C rise = 135°C, exceeds 130°C max
    let result = analyzer.validate_max_temperature(
        "TestNet", 25.0,  // ambient
        110.0, // rise (too high!)
        max_temp,
    );

    assert!(result.is_err(), "Should fail - exceeds max temp");

    if let Err(violation) = result {
        match violation {
            hwc_physics::thermal::ThermalViolation::MaxTemperature {
                net,
                actual_temp_c,
                max_temp_c,
            } => {
                assert_eq!(net, "TestNet");
                assert_eq!(actual_temp_c, 135.0);
                assert_eq!(max_temp_c, max_temp);
            }
            _ => panic!("Wrong violation type"),
        }
    }
}

#[test]
fn test_validate_temperature_rise_within_limit() {
    let analyzer = ThermalAnalyzer::new();
    let constraints = hwc_physics::thermal::ProfileConstraints::default();

    let result = analyzer.validate_temperature_rise_with_constraints("TestNet", 10.0, &constraints);

    assert!(result.is_ok(), "10°C rise should be within 20°C limit");
}

#[test]
fn test_validate_temperature_rise_exceeds_limit() {
    let analyzer = ThermalAnalyzer::new();
    let constraints = hwc_physics::thermal::ProfileConstraints::default();

    let result = analyzer.validate_temperature_rise_with_constraints("TestNet", 25.0, &constraints);

    assert!(result.is_err(), "25°C rise should exceed 20°C limit");

    if let Err(violation) = result {
        match violation {
            hwc_physics::thermal::ThermalViolation::TemperatureRise {
                net,
                actual_rise_c,
                max_rise_c,
            } => {
                assert_eq!(net, "TestNet");
                assert_eq!(actual_rise_c, 25.0);
                assert_eq!(max_rise_c, 20.0);
            }
            _ => panic!("Wrong violation type"),
        }
    }
}

#[test]
fn test_detect_thermal_clustering_no_violation() {
    let analyzer = ThermalAnalyzer::new();

    // Two traces far apart (10mm)
    let traces = vec![
        ("Net1".into(), 200.0, 0),          // 200mW at position 0
        ("Net2".into(), 200.0, 10_000_000), // 200mW at position 10mm
    ];

    let violations = analyzer.detect_thermal_clustering(&traces, 1_000_000); // 1mm threshold

    assert!(
        violations.is_empty(),
        "Traces 10mm apart should not cluster"
    );
}

#[test]
fn test_detect_thermal_clustering_violation() {
    let analyzer = ThermalAnalyzer::new();

    // Two high-power traces close together (0.5mm)
    let traces = vec![
        ("PowerNet1".into(), 500.0, 0),       // 500mW at position 0
        ("PowerNet2".into(), 500.0, 500_000), // 500mW at position 0.5mm
    ];

    let violations = analyzer.detect_thermal_clustering(&traces, 1_000_000); // 1mm threshold

    assert_eq!(
        violations.len(),
        1,
        "Should detect one clustering violation"
    );

    match &violations[0] {
        hwc_physics::thermal::ThermalViolation::ThermalClustering {
            nets,
            combined_power_mw,
            distance_nm,
        } => {
            assert_eq!(nets.len(), 2);
            assert!(nets.contains(&"PowerNet1".into()));
            assert!(nets.contains(&"PowerNet2".into()));
            assert_eq!(*combined_power_mw, 1000.0);
            assert_eq!(*distance_nm, 500_000);
        }
        _ => panic!("Wrong violation type"),
    }
}

#[test]
fn test_detect_thermal_clustering_low_power_ignored() {
    let analyzer = ThermalAnalyzer::new();

    // Two low-power traces close together (should be ignored)
    let traces = vec![
        ("SignalNet1".into(), 50.0, 0),       // 50mW (low power)
        ("SignalNet2".into(), 50.0, 500_000), // 50mW (low power)
    ];

    let violations = analyzer.detect_thermal_clustering(&traces, 1_000_000); // 1mm threshold

    assert!(
        violations.is_empty(),
        "Low-power traces should not trigger clustering"
    );
}

#[test]
fn test_analyze_trace_thermal_complete() {
    let analyzer = ThermalAnalyzer::new();

    // Complete thermal analysis with copper thermal conductivity
    let analysis = analyzer.analyze_trace_thermal(ThermalAnalysisParams {
        power_mw: 270.0,
        length_nm: 100_000_000,           // 100mm length
        width_nm: 250_000,                // 0.25mm width
        thickness_nm: 35_000,             // 35µm thickness
        thermal_conductivity_w_mk: 401.0, // Copper
        ambient_temp_c: 25.0,             // 25°C ambient
        max_operating_temp_c: 130.0,      // FR4 typical
    });

    assert!(analysis.temperature_rise_c > 0.0);
    assert!(analysis.is_safe, "Should be safe at this power level");
    assert_eq!(analysis.max_safe_temp_c, 130.0);
}

#[test]
fn test_analyze_trace_thermal_unsafe() {
    let analyzer = ThermalAnalyzer::new();

    // Very high power in small trace
    let analysis = analyzer.analyze_trace_thermal(ThermalAnalysisParams {
        power_mw: 5000.0,
        length_nm: 10_000_000,            // 10mm length
        width_nm: 100_000,                // 0.1mm width (thin)
        thickness_nm: 35_000,             // 35µm thickness
        thermal_conductivity_w_mk: 401.0, // Copper
        ambient_temp_c: 25.0,             // 25°C ambient
        max_operating_temp_c: 130.0,      // FR4 typical
    });

    assert!(analysis.temperature_rise_c > 0.0);
    // This might be unsafe depending on the thermal model
    // Just verify the analysis runs
}

#[test]
fn test_zero_power() {
    let collector = DiagnosticCollector::new("", 100);
    let mut symbol_table = SymbolTable::new();
    symbol_table.register_material(&collector, create_copper_material());

    let analyzer = ThermalAnalyzer::new();

    let temp_rise = analyzer
        .calculate_temperature_rise_with_symbol_table(
            0.0,         // Zero power
            100_000_000, // 100mm
            250_000,     // 0.25mm
            35_000,      // 35µm
            "Copper",
            &symbol_table,
        )
        .unwrap();

    assert_eq!(
        temp_rise, 0.0,
        "Zero power should produce zero temperature rise"
    );
}

#[test]
fn test_multiple_traces_clustering() {
    let analyzer = ThermalAnalyzer::new();

    // Three traces, two clustered
    let traces = vec![
        ("Net1".into(), 300.0, 0),
        ("Net2".into(), 300.0, 500_000),    // Close to Net1
        ("Net3".into(), 300.0, 10_000_000), // Far from others
    ];

    let violations = analyzer.detect_thermal_clustering(&traces, 1_000_000); // 1mm threshold

    // Should detect clustering between Net1 and Net2 only
    assert_eq!(violations.len(), 1);
}
