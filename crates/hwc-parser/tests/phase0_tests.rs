//! Phase 0 Verification Tests for HardwareScript v0.3.1
//!
//! Tests:
//! 1. Behavioral Logic Blocks (`logic { ... }`, `reg`, `on: clk.posedge`, `reset_to: 0 when: not rst_n`, `.next` assignments)
//! 2. Loop Semantic Keys (`for ch in 0..4 key: "chan_{ch}" { ... }`)
//! 3. Comptime Attributes (`#[comptime_fuel(500_000_000)]`)
//! 4. Full `accelerator.hw` end-to-end AST validation

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::ast::*;
use hwc_parser::lexer::Lexer;
use hwc_parser::parser::Parser;

fn parse_hardware_source(src: &str) -> (Program, DiagnosticCollector) {
    let collector = DiagnosticCollector::new(src, 100);
    let lexer = Lexer::new(src);
    let tokens = lexer.tokenize().expect("Lexing should succeed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&collector);
    (program, collector)
}

#[test]
fn test_parse_logic_block() {
    let src = r#"
module TestModule {
    pins: [input clk, input rst_n, input data_in, output data_out]

    logic {
        reg state: Int = 0 on: clk.posedge reset_to: 0 when: not rst_n
        reg buffer: Int = 0 on: clk.posedge
        
        if state == 0 {
            if data_in {
                state.next = 1
                buffer.next = 0xFF
            }
        } else {
            buffer.next = buffer >> 1
            if buffer == 0 {
                state.next = 0
            }
        }
        data_out = (buffer & 1) != 0
    }
}
"#;

    let (program, collector) = parse_hardware_source(src);
    assert!(!collector.has_errors(), "Parsing failed with {} errors", collector.error_count());

    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        TopLevelItem::Module(m) => {
            assert_eq!(m.name.name, "TestModule");
            assert_eq!(m.pins.len(), 4);
            assert_eq!(m.logic_blocks.len(), 1);

            let logic_blk = &m.logic_blocks[0];
            assert_eq!(logic_blk.statements.len(), 4); // reg state, reg buffer, if, data_out = ...

            // Statement 0: reg state
            match &logic_blk.statements[0] {
                LogicStatement::Reg(r) => {
                    assert_eq!(r.name, "state");
                    assert_eq!(r.clock_edge.edge, ClockEdgeType::Posedge);
                    assert!(r.reset.is_some());
                    let rst = r.reset.as_ref().unwrap();
                    match &rst.condition {
                        Expression::Unary { operator, .. } => {
                            assert_eq!(operator, &UnaryOperator::Not);
                        }
                        other => panic!("Expected Unary Not for reset condition, got {:?}", other),
                    }
                }
                other => panic!("Expected Reg statement, got {:?}", other),
            }

            // Statement 1: reg buffer
            match &logic_blk.statements[1] {
                LogicStatement::Reg(r) => {
                    assert_eq!(r.name, "buffer");
                    assert_eq!(r.clock_edge.edge, ClockEdgeType::Posedge);
                    assert!(r.reset.is_none());
                }
                other => panic!("Expected Reg, got {:?}", other),
            }

            // Statement 2: if statement
            match &logic_blk.statements[2] {
                LogicStatement::If { condition, then_block, else_branch, .. } => {
                    assert!(then_block.len() >= 1);
                    assert!(else_branch.is_some());
                }
                other => panic!("Expected If, got {:?}", other),
            }

            // Statement 3: assignment
            match &logic_blk.statements[3] {
                LogicStatement::Assignment { target, operator, .. } => {
                    assert_eq!(*operator, AssignmentOperator::Assign);
                    match target {
                        Expression::Variable { name, .. } => assert_eq!(name, "data_out"),
                        other => panic!("Expected Variable target, got {:?}", other),
                    }
                }
                other => panic!("Expected Assignment, got {:?}", other),
            }
        }
        other => panic!("Expected Module, got {:?}", other),
    }
}

#[test]
fn test_loop_semantic_key() {
    let src = r#"
space ChannelArray {
    for ch in 0..4 key: "chan_{ch}" {
        let cell = sky130_nmos(name: "M_{ch}", at: [ch * 2.0um, 5.0um])
    }
}
"#;

    let (program, collector) = parse_hardware_source(src);
    assert!(!collector.has_errors(), "Parsing failed with {} errors", collector.error_count());

    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        TopLevelItem::Space(sp) => {
            assert_eq!(sp.name.name, "ChannelArray");
            assert_eq!(sp.statements.len(), 1);

            match &sp.statements[0] {
                Statement::For { variables, key, body, .. } => {
                    assert_eq!(variables.len(), 1);
                    assert_eq!(variables[0], "ch");
                    assert!(key.is_some(), "Expected loop key to be parsed");
                    match key.as_ref().unwrap() {
                        Expression::StringLiteral { value, .. } => {
                            assert_eq!(value, "chan_{ch}");
                        }
                        other => panic!("Expected StringLiteral for loop key, got {:?}", other),
                    }
                    assert_eq!(body.statements.len(), 1);
                }
                other => panic!("Expected For statement, got {:?}", other),
            }
        }
        other => panic!("Expected Space, got {:?}", other),
    }
}

#[test]
fn test_comptime_fuel_attribute() {
    let src = r#"
#[comptime_fuel(500_000_000)]
space NeuralCrossbar_1024x1024 implements MatrixAccelerator {
    dimensions: [40.0mm, 40.0mm]
    profile: SKY130_1V8_CMOS
}
"#;

    let (program, collector) = parse_hardware_source(src);
    assert!(!collector.has_errors(), "Parsing failed with {} errors", collector.error_count());

    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        TopLevelItem::Space(sp) => {
            assert_eq!(sp.name.name, "NeuralCrossbar_1024x1024");
            assert_eq!(sp.attributes.len(), 1);
            assert_eq!(sp.attributes[0].name.name, "comptime_fuel");
            assert_eq!(sp.comptime_fuel(), Some(500_000_000));
            assert!(sp.implements.is_some());
            assert_eq!(sp.implements.as_ref().unwrap().name, "MatrixAccelerator");
        }
        other => panic!("Expected Space, got {:?}", other),
    }
}

#[test]
fn test_accelerator_e2e_spec() {
    let src = r#"
import * from @std/primitives/units
import { sky130_nmos } from @std/pdk/sky130/nmos
import { spi_master } from @std/digital/spi
import { SKY130_1V8_CMOS } from @std/pdk/sky130/profile

module HybridAccelerator {
    pins: [input clk, input rst_n, input data_in, output data_out]

    logic {
        reg state: Int = 0 on: clk.posedge reset_to: 0 when: not rst_n
        reg buffer: Int = 0 on: clk.posedge
        
        if state == 0 {
            if data_in {
                state.next = 1
                buffer.next = 0xFF
            }
        } else {
            buffer.next = buffer >> 1
            if buffer == 0 {
                state.next = 0
            }
        }
        data_out = (buffer & 1) != 0
    }
}

space Chip_Layout implements HybridAccelerator {
    dimensions: [50.0um, 30.0um]
    profile: SKY130_1V8_CMOS

    nets {
        VDD:      { classification: power,  potential: 1.8V, current: 20mA }
        VSS:      { classification: ground, potential: 0.0V, current: 20mA }
        clk:      { classification: signal }
        rst_n:    { classification: signal }
        data_in:  { classification: signal }
        data_out: { classification: signal }
    }

    let spi = spi_master(at: [5.0um, 5.0um], clk: clk, rst_n: rst_n, mosi: data_in, miso: data_out)

    region Custom_Logic_Zone {
        boundary: [25.0um, 5.0um, 20.0um, 20.0um]
        synthesize: HybridAccelerator.logic
    }

    route spi.done to Custom_Logic_Zone.data_in { intent: Signal }
}
"#;

    let (program, collector) = parse_hardware_source(src);
    assert!(!collector.has_errors(), "Parsing failed with {} errors", collector.error_count());

    assert_eq!(program.imports.len(), 4);
    assert_eq!(program.items.len(), 2);

    // Verify module HybridAccelerator
    match &program.items[0] {
        TopLevelItem::Module(m) => {
            assert_eq!(m.name.name, "HybridAccelerator");
            assert_eq!(m.pins.len(), 4);
            assert_eq!(m.logic_blocks.len(), 1);
        }
        other => panic!("Expected Module, got {:?}", other),
    }

    // Verify space Chip_Layout
    match &program.items[1] {
        TopLevelItem::Space(sp) => {
            assert_eq!(sp.name.name, "Chip_Layout");
            assert!(sp.implements.is_some());
            assert_eq!(sp.implements.as_ref().unwrap().name, "HybridAccelerator");
            assert_eq!(sp.nets.len(), 6);
            assert_eq!(sp.statements.len(), 3); // let spi, region Custom_Logic_Zone, route

            match &sp.statements[1] {
                Statement::Region(rg) => {
                    assert_eq!(rg.name, "Custom_Logic_Zone");
                    assert_eq!(rg.properties.len(), 2);
                    assert_eq!(rg.properties[0].0, "boundary");
                    assert_eq!(rg.properties[1].0, "synthesize");
                }
                other => panic!("Expected Region statement, got {:?}", other),
            }
        }
        other => panic!("Expected Space, got {:?}", other),
    }
}
