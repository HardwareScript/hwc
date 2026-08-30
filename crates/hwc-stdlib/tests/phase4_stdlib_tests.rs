// crates/hwc-stdlib/tests/phase4_stdlib_tests.rs

use hwc_parser::{DiagnosticCollector, Lexer, Parser};
use std::fs;
use std::path::PathBuf;

fn parse_hw_file(path: &PathBuf) -> hwc_parser::ast::Program {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    let lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .unwrap_or_else(|e| panic!("Lexing failed for {}: {:?}", path.display(), e));
    let collector = DiagnosticCollector::new(&source, 100);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse(&collector);
    assert_eq!(
        collector.error_count(),
        0,
        "Parser errors in {}: {:?}",
        path.display(),
        collector
    );
    prog
}

#[test]
fn test_ga_filler_pcell_parses() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // to crates
    path.pop(); // to hwc
    let ga_fill_path = path.join("stdlib/pdk/sky130/ga_filler.hw");
    let prog = parse_hw_file(&ga_fill_path);
    assert!(!prog.items.is_empty() || !prog.imports.is_empty());
}

#[test]
fn test_standard_cells_parses() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let stdcells_path = path.join("stdlib/pdk/sky130/stdcells.hw");
    let prog = parse_hw_file(&stdcells_path);
    assert!(!prog.items.is_empty() || !prog.imports.is_empty());
}

#[test]
fn test_pdk_resistor_and_profile_parses() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let resistor_path = path.join("stdlib/pdk/sky130/resistor.hw");
    let profile_path = path.join("stdlib/pdk/sky130/profile.hw");
    let reexport_path = path.join("stdlib/pdk/sky130/mod.hw");

    parse_hw_file(&resistor_path);
    parse_hw_file(&profile_path);
    parse_hw_file(&reexport_path);
}

#[test]
fn test_digital_ip_macros_parse() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let digital_dir = path.join("stdlib/digital");

    let files = ["spi.hw", "uart.hw", "i2c.hw", "fifo.hw", "adder.hw", "mod.hw"];
    for f in files {
        let file_path = digital_dir.join(f);
        let prog = parse_hw_file(&file_path);
        assert!(!prog.items.is_empty() || !prog.imports.is_empty(), "Empty program for {}", f);
    }
}

#[test]
fn test_divider_eco_parses_cleanly() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let divider_path = path.join("examples/divider_eco.hw");
    let prog = parse_hw_file(&divider_path);
    assert!(!prog.items.is_empty() || !prog.imports.is_empty());
}
