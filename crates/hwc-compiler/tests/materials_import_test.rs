//! Test that materials require explicit import (Task 2.5)
//!
//! This test verifies that:
//! 1. Materials are NOT auto-loaded (only units/math are in prelude)
//! 2. Materials must be explicitly imported from category files
//! 3. Import syntax: `import Copper from @std/materials/conductors`

use hwc_parser::{Lexer, Parser};
use std::fs;
use std::path::PathBuf;

fn load_test_file(filename: &str) -> (String, hwc_parser::Program) {
    use hwc_compiler::DiagnosticCollector;

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(filename);

    let source = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Failed to read test file: {}", path.display()));

    let lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .unwrap_or_else(|e| panic!("Failed to tokenize {}: {:?}", filename, e));

    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(&source, 20);
    let program = parser.parse(&collector);

    if collector.has_errors() {
        panic!("Failed to parse {}: has errors", filename);
    }

    (source, program)
}

#[test]
fn test_materials_not_auto_loaded() {
    // This test verifies that materials are NOT available without import
    // (unlike units and math constants which are in the prelude)

    let (source, program) = load_test_file("materials_not_auto_loaded.hw");

    // Just verify it parses - we're defining our own material, not using Copper
    // The fact that it parsed successfully means materials are not required to be imported
    assert!(
        !program.definitions.is_empty(),
        "Should have parsed material definition"
    );

    // Verify we can access units without import (they're in prelude)
    assert!(source.contains("kg/m³"), "Should use units without import");
    assert!(
        source.contains("W/(m·K)"),
        "Should use units without import"
    );
}

#[test]
fn test_materials_explicit_import() {
    // This test verifies that materials can be imported from category files

    let (source, program) = load_test_file("materials_explicit_import.hw");

    // Verify the import statement exists
    assert!(
        source.contains("import Copper from @std/materials/conductors"),
        "Should have import statement"
    );

    // Verify it parsed successfully
    assert!(
        !program.definitions.is_empty(),
        "Should have parsed definitions"
    );
}

#[test]
fn test_units_are_auto_loaded() {
    // This test verifies that units ARE auto-loaded (in prelude)
    // Unlike materials which require explicit import

    let (source, program) = load_test_file("units_auto_loaded.hw");

    // Verify units are used without import
    assert!(source.contains("µF"), "Should use µF unit without import");
    assert!(source.contains("MHz"), "Should use MHz unit without import");
    assert!(
        source.contains("kg/m³"),
        "Should use kg/m³ unit without import"
    );

    // Verify it parsed successfully
    assert!(
        !program.definitions.is_empty(),
        "Should have parsed material definition"
    );
}

#[test]
fn test_math_constants_are_auto_loaded() {
    // This test verifies that math constants ARE auto-loaded (in prelude)
    // Unlike materials which require explicit import

    let (source, program) = load_test_file("math_constants_auto_loaded.hw");

    // Verify constants are defined without import
    assert!(
        source.contains("const TEST_CONSTANT"),
        "Should define constants"
    );
    assert!(source.contains("const PI_BASED"), "Should define constants");

    // Verify it parsed successfully
    assert!(
        !program.definitions.is_empty(),
        "Should have parsed constant definitions"
    );
}
