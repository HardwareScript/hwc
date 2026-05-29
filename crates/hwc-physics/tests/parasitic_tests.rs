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
                symbol: Some("Cu".into()),
                description: Some("Pure copper conductor".into()),
                properties: vec![Property {
                    key: "resistivity".into(),
                    value: PropertyValue::Measurement(Measurement {
                        value: 1.68e-8, // Ω·m
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
            },
        );

        // Add FR4 material
        materials.insert(
            "FR4".into(),
            MaterialDefinition {
                name: Identifier::with_dummy_span("FR4"),
                category: MaterialCategory::Insulator,
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

    // Test case: 10mm long, 0.2mm wide, 35µm thick copper trace
    let length_nm = 10_000_000; // 10mm
    let width_nm = 200_000; // 0.2mm
    let thickness_nm = 35_000; // 35µm

    let result = extractor.extract_trace_resistance(
        length_nm,
        width_nm,
        thickness_nm,
        "Copper",
        &symbol_table,
    );

    assert!(result.is_ok());
    let resistance = result.unwrap();

    // Expected: R = ρ × (L / A)
    // ρ = 1.68e-8 Ω·m
    // L = 0.01 m
    // A = 0.0002 m × 0.000035 m = 7e-9 m²
    // R = 1.68e-8 × (0.01 / 7e-9) = 0.024 Ω
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

    // Test case: 1mm² copper area, 1.6mm FR4 thickness
    let surface_area_nm2 = 1_000_000_000_000; // 1mm²
    let dielectric_thickness_nm = 1_600_000; // 1.6mm

    let result = extractor.extract_trace_capacitance(
        surface_area_nm2,
        dielectric_thickness_nm,
        "FR4",
        &symbol_table,
    );

    assert!(result.is_ok());
    let capacitance = result.unwrap();

    // Expected: C = ε₀ × εᵣ × (A / d)
    // ε₀ = 8.854e-12 F/m
    // εᵣ = 4.5
    // A = 1mm² = 1e-6 m²
    // d = 1.6mm = 1.6e-3 m
    // C = 8.854e-12 × 4.5 × (1e-6 / 1.6e-3) = 8.854e-12 × 4.5 × 6.25e-4 = 24.9e-15 F = 0.0249 pF
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
    // Test case: 100 voxels, each 100nm × 100nm
    let voxel_count = 100;
    let voxel_size_nm = 100;

    let area = ParasiticExtractor::calculate_surface_area(voxel_count, voxel_size_nm);

    // Expected: 100 × (100 × 100) = 1,000,000 nm²
    assert_eq!(area, 1_000_000);
}

#[test]
fn test_calculate_surface_area_zero_voxels() {
    let area = ParasiticExtractor::calculate_surface_area(0, 100);
    assert_eq!(area, 0);
}

#[test]
fn test_extract_parasitics_complete() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    // Test case: 10mm long, 0.2mm wide, 35µm thick copper trace
    // 100 voxels, 100nm voxel size, 1.6mm FR4 thickness
    let params = ParasiticExtractionParams {
        length_nm: 10_000_000,
        width_nm: 200_000,
        thickness_nm: 35_000,
        voxel_count: 100,
        voxel_size_nm: 100,
        dielectric_thickness_nm: 1_600_000,
        conductor_material_name: "Copper",
        dielectric_material_name: "FR4",
    };

    let result = extractor.extract_parasitics(&params, &symbol_table);

    assert!(result.is_ok());
    let parasitics = result.unwrap();

    // Verify resistance
    assert!(
        (parasitics.resistance_ohm - 0.024).abs() < 0.001,
        "Expected ~0.024Ω, got {}Ω",
        parasitics.resistance_ohm
    );

    // Verify capacitance (will be very small due to small surface area)
    assert!(parasitics.capacitance_pf > 0.0);

    // Verify metadata
    assert_eq!(parasitics.length_nm, params.length_nm);
    assert_eq!(parasitics.surface_area_nm2, 1_000_000);
}

#[test]
fn test_extract_parasitics_thin_trace() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    // Test case: Very thin trace (high resistance)
    let length_nm = 50_000_000; // 50mm
    let params = ParasiticExtractionParams {
        length_nm,
        width_nm: 100_000,    // 0.1mm
        thickness_nm: 18_000, // 18µm (half-ounce copper)
        voxel_count: 500,
        voxel_size_nm: 100,
        dielectric_thickness_nm: 1_600_000,
        conductor_material_name: "Copper",
        dielectric_material_name: "FR4",
    };

    let result = extractor.extract_parasitics(&params, &symbol_table);

    assert!(result.is_ok());
    let parasitics = result.unwrap();

    // Thin trace should have higher resistance
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

    // Test case: Wide trace (low resistance, high capacitance)
    let params = ParasiticExtractionParams {
        length_nm: 10_000_000, // 10mm
        width_nm: 1_000_000,   // 1mm
        thickness_nm: 70_000,  // 70µm (2-ounce copper)
        voxel_count: 10000,
        voxel_size_nm: 100,
        dielectric_thickness_nm: 1_600_000,
        conductor_material_name: "Copper",
        dielectric_material_name: "FR4",
    };

    let result = extractor.extract_parasitics(&params, &symbol_table);

    assert!(result.is_ok());
    let parasitics = result.unwrap();

    // Wide trace should have lower resistance
    assert!(
        parasitics.resistance_ohm < 0.01,
        "Expected low resistance for wide trace, got {}Ω",
        parasitics.resistance_ohm
    );

    // Wide trace should have higher capacitance (but still small due to voxel count)
    // 10000 voxels × (100nm)² = 0.1 mm² surface area
    // This is much smaller than the 1mm² test case, so capacitance will be proportionally smaller
    assert!(
        parasitics.capacitance_pf > 0.000001,
        "Expected positive capacitance for wide trace, got {}pF",
        parasitics.capacitance_pf
    );
}

#[test]
fn test_extract_parasitics_performance() {
    let extractor = ParasiticExtractor::new();
    let symbol_table = MockSymbolTable::new();

    // Performance test: Extract parasitics for 1000 traces
    let start = std::time::Instant::now();

    let params = ParasiticExtractionParams {
        length_nm: 10_000_000,
        width_nm: 200_000,
        thickness_nm: 35_000,
        voxel_count: 100,
        voxel_size_nm: 100,
        dielectric_thickness_nm: 1_600_000,
        conductor_material_name: "Copper",
        dielectric_material_name: "FR4",
    };

    for _ in 0..1000 {
        let _ = extractor.extract_parasitics(&params, &symbol_table);
    }

    let elapsed = start.elapsed();

    // Should complete in < 10ms for 1000 traces (< 10μs per trace)
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

    // Extract resistance for 10mm trace
    let r1 = extractor
        .extract_trace_resistance(10_000_000, 200_000, 35_000, "Copper", &symbol_table)
        .unwrap();

    // Extract resistance for 20mm trace (2× length)
    let r2 = extractor
        .extract_trace_resistance(20_000_000, 200_000, 35_000, "Copper", &symbol_table)
        .unwrap();

    // Resistance should double with length
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

    // Extract resistance for 0.2mm wide trace
    let r1 = extractor
        .extract_trace_resistance(10_000_000, 200_000, 35_000, "Copper", &symbol_table)
        .unwrap();

    // Extract resistance for 0.4mm wide trace (2× width)
    let r2 = extractor
        .extract_trace_resistance(10_000_000, 400_000, 35_000, "Copper", &symbol_table)
        .unwrap();

    // Resistance should halve with width (2× cross-section)
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

    // Extract capacitance for 1mm² area
    let c1 = extractor
        .extract_trace_capacitance(1_000_000_000_000, 1_600_000, "FR4", &symbol_table)
        .unwrap();

    // Extract capacitance for 2mm² area (2× area)
    let c2 = extractor
        .extract_trace_capacitance(2_000_000_000_000, 1_600_000, "FR4", &symbol_table)
        .unwrap();

    // Capacitance should double with area
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

    // Extract capacitance for 1.6mm thickness
    let c1 = extractor
        .extract_trace_capacitance(1_000_000_000_000, 1_600_000, "FR4", &symbol_table)
        .unwrap();

    // Extract capacitance for 0.8mm thickness (0.5× thickness)
    let c2 = extractor
        .extract_trace_capacitance(1_000_000_000_000, 800_000, "FR4", &symbol_table)
        .unwrap();

    // Capacitance should double with half thickness (inverse relationship)
    assert!(
        (c2 / c1 - 2.0).abs() < 0.01,
        "Expected 2× capacitance, got {}× (c1={}, c2={})",
        c2 / c1,
        c1,
        c2
    );
}
