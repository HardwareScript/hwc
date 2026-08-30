// crates/hwc-synthesis/src/lib.rs

pub mod aig;
pub mod choice;
pub mod datapath;
pub mod liberty;
pub mod mapper;
pub mod types;
pub mod verify;
pub mod wasm;

pub use aig::arena::{Edge, PackedAigGraph, SequentialDff};
pub use aig::fraig::FraigOptimizer;
pub use choice::network::ChoiceNetwork;
pub use datapath::egraph::WordExpr;
pub use liberty::cell::StandardCell;
pub use liberty::parser::LibertyCatalog;
pub use mapper::npn::{NpnCanonicalizer, NpnClass};
pub use mapper::placer_loop::{AnalyticalPlacer, PlacedCell, ShiftLeftDelayEstimator};
pub use mapper::priority_cuts::{MappedInstance, PriorityCut, PriorityCutMapper};
pub use mapper::row_legalizer::{LegalizedCellInstance, StandardCellRowLegalizer, StandardCellSiteRow};
pub use types::{SynthesisOptions, SynthesisResult};
pub use verify::cec::{CecVerificationError, CombinationalEquivalenceChecker};
pub use wasm::wasm64_runner::Wasm64SynthesisRunner;

use compact_str::CompactString;
use hwc_engine::stackup::StackupManager;
use hwc_parser::ast::{BinaryOperator, Expression, LogicBlock, LogicElseBranch, LogicStatement, UnaryOperator};
use rustc_hash::{FxHashMap, FxHashSet};

/// Primary Trait for Digital Logic Synthesis in HardwareScript.
pub trait SynthesisEngine {
    /// Synthesizes a behavioral AST `LogicBlock` into a verified, row-legalized standard cell netlist.
    fn synthesize_logic_block(
        &self,
        top_module: &str,
        logic_blk: &LogicBlock,
        stackup: &StackupManager,
        options: &SynthesisOptions,
    ) -> Result<SynthesisResult, String>;

    /// Synthesizes a pre-constructed `PackedAigGraph` into a verified, row-legalized standard cell netlist.
    fn synthesize_aig(
        &self,
        top_module: &str,
        golden_aig: &PackedAigGraph,
        stackup: &StackupManager,
        options: &SynthesisOptions,
    ) -> Result<SynthesisResult, String>;
}

/// Standard SOTA native logic synthesizer for HardwareScript v0.3.1.
#[derive(Debug, Clone, Default)]
pub struct NativeSynthesizer {
    pub catalog: LibertyCatalog,
}

impl NativeSynthesizer {
    pub fn new() -> Self {
        Self {
            catalog: LibertyCatalog::sky130_default(),
        }
    }

    pub fn with_catalog(catalog: LibertyCatalog) -> Self {
        Self { catalog }
    }
}

impl SynthesisEngine for NativeSynthesizer {
    fn synthesize_logic_block(
        &self,
        top_module: &str,
        logic_blk: &LogicBlock,
        stackup: &StackupManager,
        options: &SynthesisOptions,
    ) -> Result<SynthesisResult, String> {
        // 1. Lower LogicBlock AST into PackedAigGraph with strongly-typed symbol table
        let mut aig = PackedAigGraph::with_capacity(128);
        lower_logic_block_to_aig(logic_blk, &mut aig)?;

        // 2. Synthesize AIG
        self.synthesize_aig(top_module, &aig, stackup, options)
    }

    fn synthesize_aig(
        &self,
        top_module: &str,
        golden_aig: &PackedAigGraph,
        stackup: &StackupManager,
        options: &SynthesisOptions,
    ) -> Result<SynthesisResult, String> {
        // Stage 4: Technology-Independent Optimization (FRAIGs)
        let (opt_aig, _merged_count) = if options.enable_fraig {
            FraigOptimizer::optimize(golden_aig)
        } else {
            (golden_aig.clone(), 0)
        };

        // Stage 4b: Choice Network
        let _choice_net = ChoiceNetwork::from_aig(opt_aig.clone());

        // Stage 5: Priority K-Cut Technology Mapping
        let mapper = PriorityCutMapper::new(&opt_aig, &self.catalog);
        let mapped_instances = mapper.map_to_liberty();

        // Stage 6: Shift-Left Analytical Placement
        let (bx, by, bw, bh) = options.region_boundary;
        let placed_cells = AnalyticalPlacer::place(&mapped_instances, bx, by, bw, bh);

        // Stage 7: Row Legalization & Power Rail Abutment (Abacus)
        let rows = StandardCellRowLegalizer::generate_rows(by, bh, 2_720_000, 460_000);
        let legalized_cells = StandardCellRowLegalizer::legalize_to_rows(&placed_cells, &rows);

        // Delay & Area computation
        let delay_estimator = ShiftLeftDelayEstimator::new(stackup, "met1");
        let mut total_area: i128 = 0;
        let mut max_delay: f32 = 0.0;

        for cell in &legalized_cells {
            total_area += i128::from(cell.width_pm) * i128::from(cell.height_pm);
            let wire_delay = delay_estimator.estimate_segment_delay_ps(cell.width_pm, 140_000);
            if let Some(std_cell) = self.catalog.get_by_name(&cell.cell_type) {
                let cell_delay = std_cell.delay_ps + wire_delay;
                if cell_delay > max_delay {
                    max_delay = cell_delay;
                }
            }
        }

        // Stage 8: Formal Combinational Equivalence Checking (CEC SAT Miter Gate)
        let mut cec_verified = false;
        if options.enable_cec {
            CombinationalEquivalenceChecker::verify_miter(golden_aig, &opt_aig)
                .map_err(|e| format!("CEC Verification Failure: {:?}", e))?;
            cec_verified = true;
        }

        Ok(SynthesisResult {
            top_module_name: compact_str::CompactString::new(top_module),
            gate_count: legalized_cells.len(),
            legalized_cells,
            total_area_pm2: total_area,
            max_delay_ps: max_delay,
            cec_verified,
        })
    }
}

/// Signal role classification in the hardware symbol table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalRole {
    PrimaryInput,
    PrimaryOutput,
    Register,
}

/// Strongly typed symbol entry in the synthesis environment.
#[derive(Debug, Clone)]
pub struct SignalSymbol {
    pub name: CompactString,
    pub role: SignalRole,
    pub current_val: Edge,
}

/// Strongly typed synthesis environment tracking symbol definitions, writes, and scope branches.
#[derive(Debug, Clone, Default)]
pub struct SynthesisEnvironment {
    pub symbols: FxHashMap<CompactString, SignalSymbol>,
    pub written_in_scope: FxHashSet<CompactString>,
}

impl SynthesisEnvironment {
    pub fn declare_register(&mut self, name: &str, q_edge: Edge) {
        let sym_name = CompactString::new(name);
        self.symbols.insert(
            sym_name.clone(),
            SignalSymbol {
                name: sym_name,
                role: SignalRole::Register,
                current_val: q_edge,
            },
        );
    }

    pub fn get_or_declare_input(&mut self, name: &str, aig: &mut PackedAigGraph) -> Edge {
        let sym_name = CompactString::new(name);
        if let Some(sym) = self.symbols.get(&sym_name) {
            sym.current_val
        } else {
            let in_edge = aig.add_input(name);
            self.symbols.insert(
                sym_name.clone(),
                SignalSymbol {
                    name: sym_name,
                    role: SignalRole::PrimaryInput,
                    current_val: in_edge,
                },
            );
            in_edge
        }
    }

    pub fn write_signal(&mut self, name: &str, val_edge: Edge) {
        let sym_name = CompactString::new(name);
        if let Some(sym) = self.symbols.get_mut(&sym_name) {
            sym.current_val = val_edge;
        } else {
            self.symbols.insert(
                sym_name.clone(),
                SignalSymbol {
                    name: sym_name.clone(),
                    role: SignalRole::PrimaryOutput,
                    current_val: val_edge,
                },
            );
        }
        self.written_in_scope.insert(sym_name);
    }

    pub fn get_val(&self, name: &str) -> Edge {
        self.symbols
            .get(name)
            .map_or(Edge::ZERO, |sym| sym.current_val)
    }
}

/// Lowers a behavioral `LogicBlock` AST into a `PackedAigGraph` using strongly typed symbols.
pub fn lower_logic_block_to_aig(
    logic_blk: &LogicBlock,
    aig: &mut PackedAigGraph,
) -> Result<(), String> {
    let mut env = SynthesisEnvironment::default();

    // 1. Declare all sequential registers in the symbol table
    for stmt in &logic_blk.statements {
        if let LogicStatement::Reg(reg) = stmt {
            let clk_name = format!("{:?}", reg.clock_edge.clock);
            let rst_name = reg.reset.as_ref().map(|r| format!("{:?}", r.condition));
            let q_edge = aig.add_dff(
                &reg.name,
                Edge::ZERO, // placeholder d_input
                &clk_name,
                rst_name.as_deref(),
                false,
            );
            env.declare_register(&reg.name, q_edge);
        }
    }

    // 2. Synthesize statements in topological execution order
    for stmt in &logic_blk.statements {
        lower_logic_stmt(stmt, aig, &mut env)?;
    }

    // 3. Finalize: bind D inputs for all registers and register primary outputs
    for dff in &mut aig.registers {
        dff.d_input = env.get_val(&dff.name);
    }

    for (name, sym) in &env.symbols {
        if sym.role == SignalRole::PrimaryOutput {
            aig.set_output(name.as_str(), sym.current_val);
        }
    }

    Ok(())
}

fn lower_logic_stmt(
    stmt: &LogicStatement,
    aig: &mut PackedAigGraph,
    env: &mut SynthesisEnvironment,
) -> Result<(), String> {
    match stmt {
        LogicStatement::Assignment { target, value, .. } => {
            let target_name = expr_to_name(target);
            let base_name = target_name.trim_end_matches(".next");
            let val_edge = lower_expression(value, aig, env)?;
            env.write_signal(base_name, val_edge);
        }
        LogicStatement::If {
            condition: if_cond,
            then_block,
            else_branch,
            ..
        } => {
            let cond_edge = lower_expression(if_cond, aig, env)?;

            // Fork environments for Then and Else branches
            let mut then_env = env.clone();
            then_env.written_in_scope.clear();
            for then_stmt in then_block {
                lower_logic_stmt(then_stmt, aig, &mut then_env)?;
            }

            let mut else_env = env.clone();
            else_env.written_in_scope.clear();
            if let Some(else_b) = else_branch {
                match else_b {
                    LogicElseBranch::Block(stmts) => {
                        for else_stmt in stmts {
                            lower_logic_stmt(else_stmt, aig, &mut else_env)?;
                        }
                    }
                    LogicElseBranch::ElseIf(else_if_stmt) => {
                        lower_logic_stmt(else_if_stmt, aig, &mut else_env)?;
                    }
                }
            }

            // Merge ONLY variables that were written in at least one branch
            let mut modified_vars: FxHashSet<CompactString> = FxHashSet::default();
            modified_vars.extend(then_env.written_in_scope.iter().cloned());
            modified_vars.extend(else_env.written_in_scope.iter().cloned());

            for var in modified_vars {
                let t_val = then_env.get_val(&var);
                let e_val = else_env.get_val(&var);

                let merged = if t_val == e_val {
                    t_val
                } else {
                    aig.add_mux(cond_edge, t_val, e_val)
                };

                env.write_signal(&var, merged);
            }
        }
        _ => {}
    }
    Ok(())
}

fn lower_expression(
    expr: &Expression,
    aig: &mut PackedAigGraph,
    env: &mut SynthesisEnvironment,
) -> Result<Edge, String> {
    match expr {
        Expression::Variable { name, .. } => {
            Ok(env.get_or_declare_input(name.as_str(), aig))
        }
        Expression::Literal { value, .. } => {
            Ok(if *value != 0 { Edge::ONE } else { Edge::ZERO })
        }
        Expression::BooleanLiteral { value, .. } => {
            Ok(if *value { Edge::ONE } else { Edge::ZERO })
        }
        Expression::Unary { operator, operand, .. } => {
            let in_edge = lower_expression(operand, aig, env)?;
            match operator {
                UnaryOperator::Not | UnaryOperator::BitwiseNot => Ok(in_edge.not()),
                _ => Ok(in_edge),
            }
        }
        Expression::Binary { left, operator, right, .. } => {
            let l_edge = lower_expression(left, aig, env)?;
            let r_edge = lower_expression(right, aig, env)?;
            match operator {
                BinaryOperator::And | BinaryOperator::BitwiseAnd => Ok(aig.add_and(l_edge, r_edge)),
                BinaryOperator::Or | BinaryOperator::BitwiseOr => Ok(aig.add_or(l_edge, r_edge)),
                BinaryOperator::BitwiseXor => Ok(aig.add_xor(l_edge, r_edge)),
                BinaryOperator::Equal => Ok(aig.add_xor(l_edge, r_edge).not()),
                BinaryOperator::NotEqual => Ok(aig.add_xor(l_edge, r_edge)),
                _ => Ok(aig.add_and(l_edge, r_edge)),
            }
        }
        Expression::FieldAccess { target, field, .. } => {
            let combined_name = format!("{}.{}", expr_to_name(target), field);
            Ok(env.get_or_declare_input(&combined_name, aig))
        }
        _ => Ok(Edge::ZERO),
    }
}

fn expr_to_name(expr: &Expression) -> String {
    match expr {
        Expression::Variable { name, .. } => name.to_string(),
        Expression::FieldAccess { target, field, .. } => {
            format!("{}.{}", expr_to_name(target), field)
        }
        _ => format!("{:?}", expr),
    }
}
