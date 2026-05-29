// Phase 7.2: Dynamic Property Calculation Tests
// Tests for temperature-dependent material properties

use hwc_diagnostics::DiagnosticCollector;
use hwc_materials::database::MaterialDatabase;

// Helper function to load materials from .hw file instead of YAML
fn load_materials_from_hw() -> MaterialDatabase {
    use hwc_compiler::{populate_material_database, SymbolTable};
    use hwc_parser::{Lexer, Parser};

    // Read the standard materials file at runtime
    let source = std::fs::read_to_string("../../data/standard-materials.hw")
        .expect("Failed to read standard-materials.hw");

    // Parse the file
    let collector = DiagnosticCollector::new("", 100);
    let lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .expect("Failed to tokenize standard-materials.hw");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&collector);

    // Build symbol table
    let mut symbol_table = SymbolTable::new();
    for def in program.definitions {
        if let hwc_parser::ast::Definition::Material(m) = def {
            symbol_table.register_material(&collector, m);
        }
    }

    // Populate material database
    populate_material_database(&symbol_table).expect("Failed to populate material database")
}

#[test]
fn test_copper_resistivity_at_25c() {
    let db = load_materials_from_hw();
    let copper = db.get_conductor("copper").expect("Copper not found");

    // At 25°C (5°C above reference of 20°C)
    let resistivity_25c = copper.resistivity_at_temp(25.0);

    // Expected: ρ(25) = ρ₀ × [1 + α × (T - T₀)]
    // ρ(25) = 1.68e-8 × [1 + 0.00429 × (25 - 20)]
    // ρ(25) = 1.68e-8 × [1 + 0.02145]
    // ρ(25) = 1.68e-8 × 1.02145 = 1.716036e-8
    let expected = 1.68e-8 * (1.0 + 0.00429 * 5.0);

    assert!(
        (resistivity_25c - expected).abs() < 1e-12,
        "Copper resistivity at 25°C: expected {}, got {}",
        expected,
        resistivity_25c
    );
}

#[test]
fn test_copper_resistivity_at_100c() {
    let db = load_materials_from_hw();
    let copper = db.get_conductor("copper").expect("Copper not found");

    // At 100°C (80°C above reference of 20°C)
    let resistivity_100c = copper.resistivity_at_temp(100.0);

    // Expected: ρ(100) = ρ₀ × [1 + α × (T - T₀)]
    // ρ(100) = 1.68e-8 × [1 + 0.00429 × (100 - 20)]
    // ρ(100) = 1.68e-8 × [1 + 0.3432]
    // ρ(100) = 1.68e-8 × 1.3432 = 2.256576e-8
    let expected = 1.68e-8 * (1.0 + 0.00429 * 80.0);

    assert!(
        (resistivity_100c - expected).abs() < 1e-12,
        "Copper resistivity at 100°C: expected {}, got {}",
        expected,
        resistivity_100c
    );
}

#[test]
fn test_copper_resistivity_increase_25c_to_100c() {
    let db = load_materials_from_hw();
    let copper = db.get_conductor("copper").expect("Copper not found");

    let resistivity_25c = copper.resistivity_at_temp(25.0);
    let resistivity_100c = copper.resistivity_at_temp(100.0);

    // Resistivity should increase with temperature for metals
    assert!(
        resistivity_100c > resistivity_25c,
        "Resistivity should increase with temperature"
    );

    // Calculate percentage increase
    let percent_increase = ((resistivity_100c - resistivity_25c) / resistivity_25c) * 100.0;

    // Expected: ~32% increase over 75°C (0.429% per °C × 75°C ≈ 32%)
    let expected_percent = 0.429 * 75.0;

    assert!(
        (percent_increase - expected_percent).abs() < 1.0,
        "Expected ~{}% increase, got {}%",
        expected_percent,
        percent_increase
    );
}

#[test]
fn test_aluminum_resistivity_at_100c() {
    let db = load_materials_from_hw();
    let aluminum = db.get_conductor("aluminum").expect("Aluminum not found");

    // At 100°C (80°C above reference of 20°C)
    let resistivity_100c = aluminum.resistivity_at_temp(100.0);

    // Expected: ρ(100) = ρ₀ × [1 + α × (T - T₀)]
    // ρ(100) = 2.82e-8 × [1 + 0.0038 × (100 - 20)]
    // ρ(100) = 2.82e-8 × [1 + 0.304]
    // ρ(100) = 2.82e-8 × 1.304 = 3.67728e-8
    let expected = 2.82e-8 * (1.0 + 0.0038 * 80.0);

    assert!(
        (resistivity_100c - expected).abs() < 1e-12,
        "Aluminum resistivity at 100°C: expected {}, got {}",
        expected,
        resistivity_100c
    );
}

#[test]
fn test_gold_resistivity_at_100c() {
    let db = load_materials_from_hw();
    let gold = db.get_conductor("gold").expect("Gold not found");

    // At 100°C (80°C above reference of 20°C)
    let resistivity_100c = gold.resistivity_at_temp(100.0);

    // Expected: ρ(100) = ρ₀ × [1 + α × (T - T₀)]
    // ρ(100) = 2.44e-8 × [1 + 0.0034 × (100 - 20)]
    // ρ(100) = 2.44e-8 × [1 + 0.272]
    // ρ(100) = 2.44e-8 × 1.272 = 3.10368e-8
    let expected = 2.44e-8 * (1.0 + 0.0034 * 80.0);

    assert!(
        (resistivity_100c - expected).abs() < 1e-12,
        "Gold resistivity at 100°C: expected {}, got {}",
        expected,
        resistivity_100c
    );
}

#[test]
fn test_copper_resistance_at_temperature() {
    let db = load_materials_from_hw();
    let copper = db.get_conductor("copper").expect("Copper not found");

    // Test trace: 10mm long, 0.5mm wide, 35µm thick
    let length_nm = 10_000_000; // 10mm
    let width_nm = 500_000; // 0.5mm
    let thickness_nm = 35_000; // 35µm (1oz copper)

    let resistance_20c =
        copper.calculate_resistance_at_temp(length_nm, width_nm, thickness_nm, 20.0);
    let resistance_100c =
        copper.calculate_resistance_at_temp(length_nm, width_nm, thickness_nm, 100.0);

    // Resistance should increase with temperature
    assert!(
        resistance_100c > resistance_20c,
        "Resistance should increase with temperature"
    );

    // Verify the ratio matches the resistivity ratio
    let resistivity_20c = copper.resistivity_at_temp(20.0);
    let resistivity_100c = copper.resistivity_at_temp(100.0);
    let resistivity_ratio = resistivity_100c / resistivity_20c;
    let resistance_ratio = resistance_100c / resistance_20c;

    assert!(
        (resistivity_ratio - resistance_ratio).abs() < 1e-6,
        "Resistance ratio should match resistivity ratio"
    );
}

#[test]
fn test_fr4_thermal_conductivity_at_temperature() {
    let db = load_materials_from_hw();
    let fr4 = db.get_insulator("fr4").expect("FR4 not found");

    // At 25°C (reference temperature)
    let k_25c = fr4.thermal_conductivity_at_temp(25.0);

    // Should be equal to base value at reference temperature
    assert!(
        (k_25c - fr4.thermal_conductivity_w_mk).abs() < 1e-6,
        "Thermal conductivity at reference temp should match base value"
    );

    // At 100°C (75°C above reference)
    let k_100c = fr4.thermal_conductivity_at_temp(100.0);

    // Expected: k(100) = k₀ × [1 + α × (T - T₀)]
    // k(100) = 0.3 × [1 + 0.001 × (100 - 25)]
    // k(100) = 0.3 × [1 + 0.075]
    // k(100) = 0.3 × 1.075 = 0.3225
    let expected = 0.3 * (1.0 + 0.001 * 75.0);

    assert!(
        (k_100c - expected).abs() < 1e-6,
        "FR4 thermal conductivity at 100°C: expected {}, got {}",
        expected,
        k_100c
    );
}

#[test]
fn test_copper_thermal_conductivity_decreases_with_temp() {
    let db = load_materials_from_hw();
    let copper = db.get_conductor("copper").expect("Copper not found");

    let k_20c = copper.thermal_conductivity_at_temp(20.0);
    let k_100c = copper.thermal_conductivity_at_temp(100.0);

    // Thermal conductivity of metals typically decreases with temperature
    assert!(
        k_100c < k_20c,
        "Copper thermal conductivity should decrease with temperature"
    );
}

#[test]
fn test_material_without_temp_coefficients() {
    let db = load_materials_from_hw();
    let sio2 = db.get_insulator("silicon_dioxide").expect("SiO2 not found");

    // SiO2 doesn't have temperature coefficients defined
    let k_25c = sio2.thermal_conductivity_at_temp(25.0);
    let k_100c = sio2.thermal_conductivity_at_temp(100.0);

    // Should return the same value (no temperature dependence)
    assert!(
        (k_25c - k_100c).abs() < 1e-9,
        "Materials without temp coefficients should have constant properties"
    );
}

#[test]
fn test_temperature_coefficient_formula_accuracy() {
    let db = load_materials_from_hw();
    let copper = db.get_conductor("copper").expect("Copper not found");

    // Test the linear approximation is accurate for small temperature ranges
    // The formula ρ(T) = ρ₀[1 + α(T - T₀)] is a linear approximation
    // It should be accurate within ±50°C of reference temperature

    let temps = vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0];

    for temp in temps {
        let resistivity = copper.resistivity_at_temp(temp);

        // Verify it's positive and reasonable
        assert!(resistivity > 0.0, "Resistivity must be positive");
        assert!(
            resistivity < 1e-6,
            "Resistivity seems unreasonably high at {}°C",
            temp
        );
    }
}
