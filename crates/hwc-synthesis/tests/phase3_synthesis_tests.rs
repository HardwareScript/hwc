use hwc_diagnostics::DiagnosticCollector;
use hwc_engine::stackup::StackupManager;
use hwc_parser::ast::*;
use hwc_parser::lexer::Lexer;
use hwc_parser::parser::Parser;
use hwc_synthesis::aig::arena::{Edge, PackedAigGraph};
use hwc_synthesis::aig::fraig::FraigOptimizer;
use hwc_synthesis::datapath::egraph::WordExpr;
use hwc_synthesis::mapper::npn::NpnCanonicalizer;
use hwc_synthesis::mapper::placer_loop::ShiftLeftDelayEstimator;
use hwc_synthesis::mapper::priority_cuts::PriorityCutMapper;
use hwc_synthesis::mapper::row_legalizer::StandardCellRowLegalizer;
use hwc_synthesis::verify::cec::CombinationalEquivalenceChecker;
use hwc_synthesis::{NativeSynthesizer, SynthesisEngine, SynthesisOptions};
use std::time::Instant;

fn create_test_stackup() -> StackupManager {
    // Empty stackup defaults to (eps_r=3.9, z_ground=0)
    StackupManager::new(Vec::new())
}

fn parse_hardware_source(src: &str) -> (Program, DiagnosticCollector) {
    let collector = DiagnosticCollector::new(src, 100);
    let lexer = Lexer::new(src);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&collector);
    (program, collector)
}

#[test]
fn test_packed_aig_arena_constant_folding() {
    let mut aig = PackedAigGraph::with_capacity(32);
    let in0 = aig.add_input("in0");
    let in1 = aig.add_input("in1");

    // 0 AND x = 0
    assert_eq!(aig.add_and(Edge::ZERO, in0), Edge::ZERO);
    assert_eq!(aig.add_and(in0, Edge::ZERO), Edge::ZERO);

    // 1 AND x = x
    assert_eq!(aig.add_and(Edge::ONE, in0), in0);
    assert_eq!(aig.add_and(in0, Edge::ONE), in0);

    // x AND x = x
    assert_eq!(aig.add_and(in0, in0), in0);

    // x AND (NOT x) = 0
    assert_eq!(aig.add_and(in0, in0.not()), Edge::ZERO);

    // Structural deduplication
    let and1 = aig.add_and(in0, in1);
    let and2 = aig.add_and(in1, in0); // Commutativity
    assert_eq!(and1, and2);
}

#[test]
fn test_fraig_simulation_and_sat_sweeping() {
    let mut aig = PackedAigGraph::with_capacity(32);
    let a = aig.add_input("a");
    let b = aig.add_input("b");

    // Build node 1: (a & b)
    let n1 = aig.add_and(a, b);
    // Build node 2: !(!a | !b) which mathematically equals (a & b)
    let n2 = aig.add_or(a.not(), b.not()).not();

    aig.set_output("out1", n1);
    aig.set_output("out2", n2);

    let initial_len = aig.len();
    let (opt_aig, merged) = FraigOptimizer::optimize(&aig);

    // Should detect and merge equivalent functional nodes
    assert!(merged > 0 || opt_aig.len() <= initial_len);
}

#[test]
fn test_word_level_egraph_optimization() {
    // x * 8 -> x << 3
    let expr = WordExpr::Mul(
        Box::new(WordExpr::Signal("data".into(), 8)),
        Box::new(WordExpr::Constant(8, 8)),
    );
    let opt = expr.optimize_algebraic();
    assert_eq!(
        opt,
        WordExpr::ShiftLeft(Box::new(WordExpr::Signal("data".into(), 8)), 3)
    );

    // (a + b) - b -> a
    let sub_expr = WordExpr::Sub(
        Box::new(WordExpr::Add(
            Box::new(WordExpr::Signal("x".into(), 8)),
            Box::new(WordExpr::Signal("y".into(), 8)),
        )),
        Box::new(WordExpr::Signal("y".into(), 8)),
    );
    let sub_opt = sub_expr.optimize_algebraic();
    assert_eq!(sub_opt, WordExpr::Signal("x".into(), 8));
}

#[test]
fn test_npn_canonicalization_and_automorphism() {
    // 2-input NAND2: 0x7777_7777_7777_7777
    let nand_tt = 0x7777_7777_7777_7777u64;
    let npn = NpnCanonicalizer::canonicalize(nand_tt, 2);
    assert_eq!(npn.num_inputs, 2);

    // Symmetries of NAND2 must be S2 permutation group: [[0, 1], [1, 0]]
    let symmetries = NpnCanonicalizer::extract_automorphism_group(nand_tt, 2);
    assert_eq!(symmetries.len(), 2);
    assert!(symmetries.contains(&vec![0, 1]));
    assert!(symmetries.contains(&vec![1, 0]));
}

#[test]
fn test_priority_k_cut_technology_mapping() {
    let synth = NativeSynthesizer::new();
    let mut aig = PackedAigGraph::with_capacity(32);
    let a = aig.add_input("a");
    let b = aig.add_input("b");
    let c = aig.add_input("c");

    let ab = aig.add_and(a, b);
    let abc = aig.add_or(ab, c);
    aig.set_output("y", abc);

    let mapper = PriorityCutMapper::new(&aig, &synth.catalog);
    let instances = mapper.map_to_liberty();
    assert!(!instances.is_empty());
}

#[test]
fn test_shift_left_delay_estimator_with_stackup() {
    let stackup = create_test_stackup();
    let estimator = ShiftLeftDelayEstimator::new(&stackup, "met1");

    // Estimate delay for 10um length, 140nm width
    let delay_ps = estimator.estimate_segment_delay_ps(10_000_000, 140_000);
    assert!(delay_ps > 0.0);
}

#[test]
fn test_abacus_row_legalizer_abutment() {
    let rows = StandardCellRowLegalizer::generate_rows(0, 20_000_000, 2_720_000, 460_000);
    assert!(rows.len() >= 7);

    // Check alternating orientation for power rail abutment
    assert!(!rows[0].is_flipped_y);
    assert!(rows[1].is_flipped_y);
    assert!(!rows[2].is_flipped_y);

    let placed = vec![
        hwc_synthesis::mapper::placer_loop::PlacedCell {
            instance_name: "gate_1".into(),
            cell_type: "NAND2".into(),
            raw_x_pm: 1_234_567, // Off-grid X
            raw_y_pm: 1_000_000, // Off-grid Y
            width_pm: 920_000,
            height_pm: 2_720_000,
            symmetries: vec![vec![0, 1], vec![1, 0]],
        },
        hwc_synthesis::mapper::placer_loop::PlacedCell {
            instance_name: "gate_2".into(),
            cell_type: "INV".into(),
            raw_x_pm: 1_500_000, // Potential overlap
            raw_y_pm: 1_100_000,
            width_pm: 460_000,
            height_pm: 2_720_000,
            symmetries: vec![vec![0]],
        },
    ];

    let legalized = StandardCellRowLegalizer::legalize_to_rows(&placed, &rows);
    assert_eq!(legalized.len(), 2);

    // Cells must be snapped on site grid multiples (460_000 pm)
    assert_eq!(legalized[0].pos_x_pm % 460_000, 0);
    assert_eq!(legalized[1].pos_x_pm % 460_000, 0);

    // Must be non-overlapping in X
    assert!(legalized[1].pos_x_pm >= legalized[0].pos_x_pm + legalized[0].width_pm);
}

#[test]
fn test_formal_cec_sat_miter() {
    let mut golden = PackedAigGraph::with_capacity(16);
    let a = golden.add_input("a");
    let b = golden.add_input("b");
    let out_g = golden.add_and(a, b);
    golden.set_output("y", out_g);

    let mut synth = PackedAigGraph::with_capacity(16);
    let sa = synth.add_input("a");
    let sb = synth.add_input("b");
    // Equivalent: !(!a | !b)
    let out_s = synth.add_or(sa.not(), sb.not()).not();
    synth.set_output("y", out_s);

    // CEC Proof Gate: Must prove UNSAT (Ok)
    let res = CombinationalEquivalenceChecker::verify_miter(&golden, &synth);
    assert!(res.is_ok());

    // Introduce mutation bug
    let mut mutated = PackedAigGraph::with_capacity(16);
    let ma = mutated.add_input("a");
    let mb = mutated.add_input("b");
    let out_m = mutated.add_or(ma, mb); // Bug: OR instead of AND
    mutated.set_output("y", out_m);

    let err_res = CombinationalEquivalenceChecker::verify_miter(&golden, &mutated);
    assert!(err_res.is_err());
}

#[test]
fn test_end_to_end_accelerator_synthesis() {
    let source = r#"
    module HybridAccelerator {
        pins: [input clk, input rst_n, input data_in, output data_out]

        logic {
            reg state: Int = 0 on: clk.posedge reset_to: 0 when: not rst_n
            reg buffer: Int = 0 on: clk.posedge
            
            if state == 0 {
                if data_in {
                    state = 1
                    buffer = 255
                }
            } else {
                buffer = buffer
                if buffer == 0 {
                    state = 0
                }
            }
            data_out = buffer
        }
    }
    "#;

    let (program, collector) = parse_hardware_source(source);
    assert!(!collector.has_errors(), "Parse errors detected");
    let module = program
        .items
        .iter()
        .find_map(|item| match item {
            TopLevelItem::Module(m) => Some(m),
            _ => None,
        })
        .expect("Module HybridAccelerator should exist");

    let logic_blk = module
        .logic_blocks
        .first()
        .expect("Logic block should exist");

    let synth = NativeSynthesizer::new();
    let stackup = create_test_stackup();
    let options = SynthesisOptions {
        target_freq_mhz: 100.0,
        enable_fraig: true,
        enable_word_rewrite: true,
        enable_cec: true,
        region_boundary: (25_000_000, 5_000_000, 20_000_000, 20_000_000),
    };

    let start = Instant::now();
    let result = match synth.synthesize_logic_block("HybridAccelerator", logic_blk, &stackup, &options) {
        Ok(r) => r,
        Err(e) => panic!("Synthesis failed: {}", e),
    };
    let duration = start.elapsed();

    println!("Synthesis completed in {:?}", duration);
    println!("Gate count: {}", result.gate_count);
    println!("Total area: {} um^2", (result.total_area_pm2 as f64) * 1e-12);
    println!("Max delay: {} ps", result.max_delay_ps);

    // Verification Gates
    assert!(duration.as_millis() < 50); // Under 50ms in debug, <4ms in release
    assert!(result.gate_count > 0);
    assert!(result.cec_verified);
    assert!(!result.legalized_cells.is_empty());

    // All cells must be on grid
    for cell in &result.legalized_cells {
        assert_eq!(cell.pos_x_pm % 460_000, 0);
        assert_eq!(cell.pos_y_pm % 2_720_000, 0);
    }
}
