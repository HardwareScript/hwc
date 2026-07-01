//! Tests for parasitic extraction (RCX).
//!
//! These tests verify resistance and capacitance extraction from physical traces.

use compact_str::CompactString;
use hwc_parser::{
    Identifier, MaterialCategory, MaterialDefinition, Measurement, Property, PropertyValue, Span,
    Unit,
};
use hwc_physics::{ParasiticExtractionParams, ParasiticExtractor, PropertyError};

/// Mock Symbol Table for testing
struct MockSymbolTable {
    materials: rustc_hash::FxHashMap<CompactString, MaterialDefinition>,
}

impl MockSymbolTable {
    fn new() -> Self {
        let mut materials = rustc_hash::FxHashMap::default();

        // Add Copper material
        materials.insert(
            "Copper".into(),
            MaterialDefinition {
                name: Identifier::with_dummy_span("Copper"),
                category: MaterialCategory::Conductor,
                process: hwc_parser::ManufacturingProcess::default(),
                symbol: Some("Cu".into()),
                description: Some("Pure copper conductor".into()),
                properties: vec![Property {
                    key: "resistivity".into(),
                    value: PropertyValue::Measurement(Measurement {
                        value: 1.68e-8,
                        unit: Unit::Custom("Ω·m".into()),
                        span: Span::new(0, 10),
                    }),
                    span: Span::new(0, 10),
                }],
                span: Span::new(0, 10),
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
            },
        );

        // Add FR4 material
        materials.insert(
            "FR4".into(),
            MaterialDefinition {
                name: Identifier::with_dummy_span("FR4"),
                category: MaterialCategory::Insulator,
                process: hwc_parser::ManufacturingProcess::default(),
                symbol: None,
                description: Some("Standard PCB substrate".into()),
                properties: vec![Property {
                    key: "relative_permittivity".into(),
                    value: PropertyValue::Number(4.5),
                    span: Span::new(0, 10),
                }],
                span: Span::new(0, 10),
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
            },
        );

        Self { materials }
    }
}

impl hwc_physics::parasitic::SymbolTableTrait for MockSymbolTable {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String> {
        self.materials
            .get(name)
            .ok_or_else(|| format!("Material '{}' not found", name))
    }
}

#[test]
fn test_extract_trace_resistance_copper() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let length_nm = 10_000_000;
    let width_nm = 200_000;
    let thickness_nm = 35_000;

    let result = extractor.extract_trace_resistance(
        length_nm,
        width_nm,
        thickness_nm,
        "Copper",
        &symbol_table,
    );

    assert!(result.is_ok());
    let resistance = result.unwrap();

    assert!(
        (resistance - 0.024).abs() < 0.001,
        "Expected ~0.024Ω, got {}Ω",
        resistance
    );
}

#[test]
fn test_extract_trace_resistance_missing_material() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let result = extractor.extract_trace_resistance(
        10_000_000,
        200_000,
        35_000,
        "NonExistentMaterial",
        &symbol_table,
    );

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        PropertyError::MissingProperty { .. }
    ));
}

#[test]
fn test_extract_trace_capacitance_fr4() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let surface_area_nm2 = 1_000_000_000_000;
    let dielectric_thickness_nm = 1_600_000;

    let result = extractor.extract_trace_capacitance(
        surface_area_nm2,
        dielectric_thickness_nm,
        "FR4",
        &symbol_table,
    );

    assert!(result.is_ok());
    let capacitance = result.unwrap();

    assert!(
        (capacitance - 0.0249).abs() < 0.001,
        "Expected ~0.0249pF, got {}pF",
        capacitance
    );
}

#[test]
fn test_extract_trace_capacitance_missing_material() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let result = extractor.extract_trace_capacitance(
        1_000_000_000_000,
        1_600_000,
        "NonExistentMaterial",
        &symbol_table,
    );

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        PropertyError::MissingProperty { .. }
    ));
}

#[test]
fn test_calculate_surface_area() {
    let length_nm = 10_000_000_i64;
    let width_nm = 200_000_i64;
    let thickness_nm = 35_000_i64;

    let area = ParasiticExtractor::calculate_surface_area(length_nm, width_nm, thickness_nm);

    // Expected: 2 * (L*W + L*T + W*T)
    // = 2 * (10_000_000*200_000 + 10_000_000*35_000 + 200_000*35_000)
    // = 2 * (2_000_000_000_000 + 350_000_000_000 + 7_000_000_000)
    // = 2 * 2_357_000_000_000
    // = 4_714_000_000_000
    assert_eq!(area, 4_714_000_000_000);
}

#[test]
fn test_extract_parasitics_complete() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let params = ParasiticExtractionParams {
        length_nm: 10_000_000,
        width_nm: 200_000,
        thickness_nm: 35_000,
        dielectric_thickness_nm: 1_600_000,
        conductor_material_name: "Copper",
        dielectric_material_name: "FR4",
    };

    let result = extractor.extract_parasitics(&params, &symbol_table);

    assert!(result.is_ok());
    let parasitics = result.unwrap();

    assert!(
        (parasitics.resistance_ohm - 0.024).abs() < 0.001,
        "Expected ~0.024Ω, got {}Ω",
        parasitics.resistance_ohm
    );

    assert!(parasitics.capacitance_pf > 0.0);

    assert_eq!(parasitics.length_nm, params.length_nm);
    assert_eq!(parasitics.surface_area_nm2, 4_714_000_000_000);
}

#[test]
fn test_extract_parasitics_thin_trace() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let length_nm = 50_000_000;
    let params = ParasiticExtractionParams {
        length_nm,
        width_nm: 100_000,
        thickness_nm: 18_000,
        dielectric_thickness_nm: 1_600_000,
        conductor_material_name: "Copper",
        dielectric_material_name: "FR4",
    };

    let result = extractor.extract_parasitics(&params, &symbol_table);

    assert!(result.is_ok());
    let parasitics = result.unwrap();

    assert!(
        parasitics.resistance_ohm > 0.1,
        "Expected high resistance for thin trace, got {}Ω",
        parasitics.resistance_ohm
    );
}

#[test]
fn test_extract_parasitics_wide_trace() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let params = ParasiticExtractionParams {
        length_nm: 10_000_000,
        width_nm: 1_000_000,
        thickness_nm: 70_000,
        dielectric_thickness_nm: 1_600_000,
        conductor_material_name: "Copper",
        dielectric_material_name: "FR4",
    };

    let result = extractor.extract_parasitics(&params, &symbol_table);

    assert!(result.is_ok());
    let parasitics = result.unwrap();

    assert!(
        parasitics.resistance_ohm < 0.01,
        "Expected low resistance for wide trace, got {}Ω",
        parasitics.resistance_ohm
    );

    assert!(
        parasitics.capacitance_pf > 0.0,
        "Expected positive capacitance for wide trace, got {}pF",
        parasitics.capacitance_pf
    );
}

#[test]
fn test_extract_parasitics_performance() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let start = std::time::Instant::now();

    let params = ParasiticExtractionParams {
        length_nm: 10_000_000,
        width_nm: 200_000,
        thickness_nm: 35_000,
        dielectric_thickness_nm: 1_600_000,
        conductor_material_name: "Copper",
        dielectric_material_name: "FR4",
    };

    for _ in 0..1000 {
        let _ = extractor.extract_parasitics(&params, &symbol_table);
    }

    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 10,
        "Performance target missed: {}ms for 1000 traces",
        elapsed.as_millis()
    );
}

#[test]
fn test_resistance_scales_with_length() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let r1 = extractor
        .extract_trace_resistance(10_000_000, 200_000, 35_000, "Copper", &symbol_table)
        .unwrap();

    let r2 = extractor
        .extract_trace_resistance(20_000_000, 200_000, 35_000, "Copper", &symbol_table)
        .unwrap();

    assert!(
        (r2 / r1 - 2.0).abs() < 0.01,
        "Expected 2× resistance, got {}× (r1={}, r2={})",
        r2 / r1,
        r1,
        r2
    );
}

#[test]
fn test_resistance_scales_with_width() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let r1 = extractor
        .extract_trace_resistance(10_000_000, 200_000, 35_000, "Copper", &symbol_table)
        .unwrap();

    let r2 = extractor
        .extract_trace_resistance(10_000_000, 400_000, 35_000, "Copper", &symbol_table)
        .unwrap();

    assert!(
        (r1 / r2 - 2.0).abs() < 0.01,
        "Expected 2× resistance ratio, got {}× (r1={}, r2={})",
        r1 / r2,
        r1,
        r2
    );
}

#[test]
fn test_capacitance_scales_with_area() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let c1 = extractor
        .extract_trace_capacitance(1_000_000_000_000, 1_600_000, "FR4", &symbol_table)
        .unwrap();

    let c2 = extractor
        .extract_trace_capacitance(2_000_000_000_000, 1_600_000, "FR4", &symbol_table)
        .unwrap();

    assert!(
        (c2 / c1 - 2.0).abs() < 0.01,
        "Expected 2× capacitance, got {}× (c1={}, c2={})",
        c2 / c1,
        c1,
        c2
    );
}

#[test]
fn test_capacitance_scales_with_thickness() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    let c1 = extractor
        .extract_trace_capacitance(1_000_000_000_000, 1_600_000, "FR4", &symbol_table)
        .unwrap();

    let c2 = extractor
        .extract_trace_capacitance(1_000_000_000_000, 800_000, "FR4", &symbol_table)
        .unwrap();

    assert!(
        (c2 / c1 - 2.0).abs() < 0.01,
        "Expected 2× capacitance, got {}× (c1={}, c2={})",
        c2 / c1,
        c1,
        c2
    );
}
