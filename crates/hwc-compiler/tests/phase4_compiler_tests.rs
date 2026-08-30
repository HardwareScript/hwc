// crates/hwc-compiler/tests/phase4_compiler_tests.rs

use hwc_compiler::eval::*;
use hwc_parser::{DiagnosticCollector, Lexer, Parser};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

fn evaluate_file_with_snippet(path: &PathBuf, snippet: &str) -> Result<Value, EvalError> {
    let mut source = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    source.push_str("\n");
    source.push_str(snippet);

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

    let mut ctx = EvaluationContext::new();
    run_script(&prog, &mut ctx, None).map(|v| v.unwrap_or(Value::Void))
}

#[test]
fn test_ga_filler_evaluation_speed_and_ports() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // to crates
    path.pop(); // to hwc
    let ga_fill_path = path.join("stdlib/pdk/sky130/ga_filler.hw");

    let snippet = r#"
        let cell = sky130_fd_sc_hd__ga_fill([10.0um, 10.0um], 2, "test_ga")
        assert(cell.site_count == 2)
        assert(cell.is_committed == false)
    "#;

    let start = Instant::now();
    let res = evaluate_file_with_snippet(&ga_fill_path, snippet);
    let elapsed = start.elapsed();

    assert!(res.is_ok(), "GA filler eval error: {:?}", res.err());
    assert!(
        elapsed.as_millis() < 50,
        "GA filler evaluation took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_stdcells_evaluation_speed() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let stdcells_path = path.join("stdlib/pdk/sky130/stdcells.hw");

    let snippet = r#"
        let inv = sky130_fd_sc_hd__inv_1([0.0um, 0.0um], "my_inv")
        assert(inv.sites == 1)
        let nand = sky130_fd_sc_hd__nand2_1([2.0um, 0.0um], "my_nand")
        assert(nand.sites == 2)
        let dff = sky130_fd_sc_hd__dfxtp_1([4.0um, 0.0um], "my_dff")
        assert(dff.sites == 6)
    "#;

    let start = Instant::now();
    let res = evaluate_file_with_snippet(&stdcells_path, snippet);
    let elapsed = start.elapsed();

    assert!(res.is_ok(), "Stdcells eval error: {:?}", res.err());
    assert!(
        elapsed.as_millis() < 50,
        "Stdcells evaluation took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_digital_spi_evaluation_under_1ms() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let spi_path = path.join("stdlib/digital/spi.hw");

    let snippet = r#"
        let master = spi_master([10.0um, 20.0um])
        assert(master.area_um2 == 450.0)
        let slave = spi_slave([50.0um, 20.0um])
        assert(slave.area_um2 == 375.0)
    "#;

    let start = Instant::now();
    let res = evaluate_file_with_snippet(&spi_path, snippet);
    let elapsed = start.elapsed();

    assert!(res.is_ok(), "SPI eval error: {:?}", res.err());
    assert!(
        elapsed.as_millis() < 50,
        "SPI macro evaluation took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_digital_macros_batch_evaluation() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let digital_dir = path.join("stdlib/digital");

    let cases = [
        ("uart.hw", "let tx = uart_tx([0.0um, 0.0um])\nlet rx = uart_rx([30.0um, 0.0um])"),
        ("i2c.hw", "let i2c = i2c_master([0.0um, 0.0um])"),
        ("fifo.hw", "let fifo = sync_fifo([0.0um, 0.0um])"),
        ("adder.hw", "let cla = cla_adder([0.0um, 0.0um])"),
    ];

    for (file_name, snippet) in cases {
        let file_path = digital_dir.join(file_name);
        let start = Instant::now();
        let res = evaluate_file_with_snippet(&file_path, snippet);
        let elapsed = start.elapsed();
        assert!(res.is_ok(), "Eval error for {}: {:?}", file_name, res.err());
        assert!(
            elapsed.as_millis() < 50,
            "Eval for {} took too long: {:?}",
            file_name,
            elapsed
        );
    }
}
