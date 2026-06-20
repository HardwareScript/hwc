//! NATIVE Logic Synthesis - Direct HardwareSpace Mutation
//!
//! This is the TRUE NATIVE implementation where the logic synthesizer
//! directly places components into the HardwareSpace netlist instead of
//! creating intermediate representations.
//!
//! ## Architecture:
//! - LogicSynthesizer takes `&mut HardwareSpace`
//! - Components are placed directly using `space.netlist.add_component()`
//! - Nets are created directly using `space.netlist.add_net()`
//! - NO intermediate data structures
//! - NO conversion layers

mod dependency_graph;
mod error;
mod validation;

pub use dependency_graph::DependencyGraph;
pub use error::SynthesisError;

use crate::electrical_symbol_table::ElectricalSymbolTable;
use crate::symbol_table::SymbolTable;
use crate::width_inference::{WidthInference, WidthValidationResult, WidthWarning};
use crate::DiagnosticReporterAdapter;
use compact_str::CompactString;
use hwc_diagnostics::DiagnosticCollector;
use hwc_engine::{ComponentId, ComponentPlacer, HardwareSpace, PinId, PlacementParams, Point3D};
use hwc_parser::ast::LogicOperator;
use hwc_parser::logic::*;

/// Native logic synthesizer - directly mutates HardwareSpace.
pub struct LogicSynthesizer<'a> {
    /// Reference to the hardware space being built
    space: &'a mut HardwareSpace,
    /// Symbol table for component lookups
    symbol_table: &'a SymbolTable,
    /// Electrical symbol table for tracking wires and pins
    electrical_symbols: ElectricalSymbolTable,
    /// Width inference engine for tracking bit widths
    width_inference: WidthInference<'a>,
    /// Next unique component ID
    next_component_id: usize,
    /// Dependency graph for combinational loop detection
    dependency_graph: DependencyGraph,
    /// Warnings collected during synthesis
    warnings: Vec<WidthWarning>,
    /// Diagnostic collector for reporting waivers
    collector: &'a DiagnosticCollector,
}

impl<'a> LogicSynthesizer<'a> {
    /// Create a new native logic synthesizer that directly mutates the space.
    pub fn new(
        space: &'a mut HardwareSpace,
        symbol_table: &'a SymbolTable,
        collector: &'a DiagnosticCollector,
    ) -> Self {
        Self {
            space,
            symbol_table,
            electrical_symbols: ElectricalSymbolTable::new(),
            width_inference: WidthInference::new(symbol_table),
            next_component_id: 0,
            dependency_graph: DependencyGraph::new(),
            warnings: Vec::new(),
            collector,
        }
    }

    /// Synthesize a logic block directly into the hardware space.
    ///
    /// This is the NATIVE approach - components are placed immediately,
    /// nets are created immediately, no intermediate representations.
    pub fn synthesize_logic_block(
        &mut self,
        collector: &DiagnosticCollector,
        logic_block: &LogicBlock,
        module_pins: &[(String, Option<usize>)],
    ) -> Result<Vec<WidthWarning>, SynthesisError> {
        // PASS 0: Load module pins
        for (pin_name, bit_width) in module_pins {
            if let Err(e) =
                self.electrical_symbols
                    .add_output_pin(pin_name.clone().into(), *bit_width, None)
            {
                collector.report(SynthesisError::from(e));
            }

            if let Some(width) = bit_width {
                self.width_inference
                    .register_width(pin_name.clone().into(), *width);
            }
        }

        // PASS 1: Dependency analysis & combinational loop detection
        for statement in &logic_block.statements {
            match statement {
                LogicStatement::Let {
                    name, expression, ..
                } => {
                    let dependencies = self.dependency_graph.extract_variables(expression);
                    self.dependency_graph
                        .add_dependencies(name.clone(), dependencies);

                    if matches!(expression, LogicExpression::RegisterInit { .. }) {
                        self.dependency_graph.mark_as_register(name.clone());
                    }
                }
                LogicStatement::Assignment {
                    target, expression, ..
                } => {
                    let target_name = match target {
                        AssignmentTarget::Variable { name, .. } => name.clone(),
                        AssignmentTarget::RegisterNext { name, .. } => {
                            format!("{}.next", name).into()
                        }
                        AssignmentTarget::Slice { name, .. } => name.clone(),
                    };
                    let dependencies = self.dependency_graph.extract_variables(expression);
                    self.dependency_graph
                        .add_dependencies(target_name, dependencies);
                }
                _ => {}
            }
        }

        // Detect combinational loops
        if let Err(e) = self.dependency_graph.detect_combinational_loops() {
            collector.report(e);
            return Ok(self.warnings.clone());
        }

        // PASS 2: Name resolution - register all wires
        for statement in &logic_block.statements {
            if let Err(e) = self.register_wires_recursive(statement) {
                collector.report(e);
            }
            if collector.should_stop() {
                return Ok(self.warnings.clone());
            }
        }

        // PASS 3: Width inference & validation
        for statement in &logic_block.statements {
            if let Err(e) = self.register_let_statements_recursive(statement) {
                collector.report(e);
            }
            if collector.should_stop() {
                return Ok(self.warnings.clone());
            }
        }

        // Validate widths
        for statement in &logic_block.statements {
            if let LogicStatement::Let {
                name,
                width: Some(specified_width),
                expression,
                ..
            } = statement
            {
                match self.width_inference.validate_assignment(
                    name,
                    *specified_width,
                    expression,
                    true,
                ) {
                    WidthValidationResult::Ok => {}
                    WidthValidationResult::Warning(warning) => {
                        self.warnings.push(warning);
                    }
                    WidthValidationResult::Error(err) => {
                        collector.report(SynthesisError::WidthError(err));
                    }
                }
            }
            if collector.should_stop() {
                return Ok(self.warnings.clone());
            }
        }

        // PASS 4: Hardware generation - NATIVE direct placement
        for statement in &logic_block.statements {
            if let Err(e) = self.synthesize_statement(statement) {
                collector.report(e);
            }
            if collector.should_stop() {
                break;
            }
        }

        // Validate clock domains
        if let Err(e) = self.validate_clock_domains() {
            collector.report(e);
        }

        Ok(self.warnings.clone())
    }

    /// Generate a unique component name.into().
    fn generate_component_name(&mut self, prefix: &str) -> CompactString {
        let name = format!("{}_{}", prefix, self.next_component_id);
        self.next_component_id += 1;
        name.into()
    }

    /// Place a synthesized component directly into the hardware space.
    fn place_component(
        &mut self,
        name: CompactString,
        component_type: CompactString,
        position: Point3D,
    ) -> Result<(), SynthesisError> {
        let placer = ComponentPlacer::new();
        placer
            .place_component(PlacementParams {
                entity_graph: &mut self.space.entity_graph,
                voxel_size: &self.space.voxel_size,
                arena: &mut self.space.netlist,
                symbol_table: self.symbol_table,
                material_registry: &mut self.space.material_registry,
                name,
                component_type,
                position,
                rotation_deg: 0.0,
                merge_waiver: hwc_parser::MergeWaiver::None,
                collector: Some(&DiagnosticReporterAdapter(self.collector)),
            })
            .map_err(|e| SynthesisError::internal(e.to_string().into(), None))?;

        Ok(())
    }

    /// Helper: Find a pin by name.into() on a component
    fn find_pin_by_name(&self, component_id: ComponentId, pin_name: &str) -> Option<PinId> {
        let pins = self.space.netlist.get_component_pins(component_id);
        for pin_id in pins {
            if let Some(pin_data) = self.space.netlist.get_pin(pin_id) {
                if pin_data.name == *pin_name {
                    return Some(pin_id);
                }
            }
        }
        None
    }

    /// Create a net connection directly in the hardware space.
    fn add_net(
        &mut self,
        from_comp: &str,
        from_pin: &str,
        to_comp: &str,
        to_pin: &str,
    ) -> Result<(), SynthesisError> {
        let from_comp_id = self
            .space
            .netlist
            .get_component_by_name(from_comp)
            .ok_or_else(|| {
                SynthesisError::internal(
                    format!("Component '{}' not found", from_comp).into(),
                    None,
                )
            })?;

        let to_comp_id = self
            .space
            .netlist
            .get_component_by_name(to_comp)
            .ok_or_else(|| {
                SynthesisError::internal(format!("Component '{}' not found", to_comp).into(), None)
            })?;

        let from_pin_id = self
            .find_pin_by_name(from_comp_id, from_pin)
            .ok_or_else(|| {
                SynthesisError::internal(
                    format!("Pin '{}' not found on component '{}'", from_pin, from_comp).into(),
                    None,
                )
            })?;

        let to_pin_id = self.find_pin_by_name(to_comp_id, to_pin).ok_or_else(|| {
            SynthesisError::internal(
                format!("Pin '{}' not found on component '{}'", to_pin, to_comp).into(),
                None,
            )
        })?;

        let net_name = format!("{}.{} -> {}.{}", from_comp, from_pin, to_comp, to_pin);
        let net_id = self.space.netlist.add_net(net_name.clone().into(), 1000, 0); // 1um width, material 0
        self.space.netlist.connect_pin(from_pin_id, net_id);
        self.space.netlist.connect_pin(to_pin_id, net_id);

        Ok(())
    }

    // Synthesis methods - all directly mutate space
    fn synthesize_statement(&mut self, statement: &LogicStatement) -> Result<(), SynthesisError> {
        match statement {
            LogicStatement::Let { expression, .. } => {
                let _wire_name = self.synthesize_expression(expression)?;
                // Wire is registered in electrical_symbols, expression result is connected
                Ok(())
            }
            LogicStatement::Assignment {
                target: _,
                expression,
                ..
            } => {
                let _result = self.synthesize_expression(expression)?;
                // TODO: Connect result to target
                Ok(())
            }
            LogicStatement::Expression(expr) => {
                let _result = self.synthesize_expression(expr)?;
                Ok(())
            }
            LogicStatement::If { .. } => {
                // TODO: Implement if statement synthesis
                Ok(())
            }
        }
    }

    fn synthesize_expression(&mut self, expr: &LogicExpression) -> Result<String, SynthesisError> {
        match expr {
            LogicExpression::Literal { value, .. } => self.synthesize_literal(*value),
            LogicExpression::Boolean { value, .. } => self.synthesize_boolean(*value),
            LogicExpression::Variable { name, .. } => Ok(name.to_string()),
            LogicExpression::Binary {
                operator,
                left,
                right,
                ..
            } => self.synthesize_binary(*operator, left, right),
            LogicExpression::RegisterInit {
                clock, reset, init, ..
            } => self.synthesize_register(clock, reset, init),
            LogicExpression::If {
                condition,
                then_expr,
                else_expr,
                ..
            } => self.synthesize_if_expression(condition, then_expr, else_expr),
            LogicExpression::Match { selector, arms, .. } => {
                self.synthesize_match_expression(selector, arms)
            }
            _ => Err(SynthesisError::internal(
                "Unsupported expression type".into(),
                None,
            )),
        }
    }

    fn synthesize_literal(&mut self, _value: i64) -> Result<String, SynthesisError> {
        let comp_name = self.generate_component_name("const");
        let position = Point3D::new(0, 0, 0);
        self.place_component(comp_name.clone(), "Constant".into(), position)?;
        Ok(comp_name.to_string())
    }

    fn synthesize_boolean(&mut self, _value: bool) -> Result<String, SynthesisError> {
        let comp_name = self.generate_component_name("const");
        let position = Point3D::new(0, 0, 0);
        self.place_component(comp_name.clone(), "Constant".into(), position)?;
        Ok(comp_name.to_string())
    }

    fn synthesize_binary(
        &mut self,
        op: LogicOperator,
        left: &LogicExpression,
        right: &LogicExpression,
    ) -> Result<String, SynthesisError> {
        let left_wire = self.synthesize_expression(left)?;
        let right_wire = self.synthesize_expression(right)?;

        let (comp_type, out_pin) = match op {
            LogicOperator::Add => ("RippleCarryAdder8", "Sum"),
            LogicOperator::Subtract => ("Subtractor8", "Diff"),
            LogicOperator::BitwiseAnd => ("AND", "Out"),
            LogicOperator::BitwiseOr => ("OR", "Out"),
            LogicOperator::BitwiseXor => ("XOR", "Out"),
            LogicOperator::ShiftLeft => ("LeftShifter8", "Out"),
            LogicOperator::ShiftRight => ("RightShifter8", "Out"),
            LogicOperator::Equal => ("Comparator_Equal", "Out"),
            LogicOperator::NotEqual => ("Comparator_NotEqual", "Out"),
            LogicOperator::LessThan => ("Comparator_LessThan", "Out"),
            LogicOperator::GreaterThan => ("Comparator_GreaterThan", "Out"),
            LogicOperator::LessThanOrEqual => ("Comparator_LessOrEqual", "Out"),
            LogicOperator::GreaterThanOrEqual => ("Comparator_GreaterOrEqual", "Out"),
            LogicOperator::Multiply => ("Multiplier8", "Product"),
            LogicOperator::Divide => ("Divider8", "Quotient"),
            LogicOperator::Modulo => ("Modulo8", "Remainder"),
        };

        let comp_name = self.generate_component_name(&comp_type.to_lowercase());
        let position = Point3D::new(0, 0, 0);
        self.place_component(comp_name.clone(), comp_type.into(), position)?;

        // Connect inputs
        self.add_net(&left_wire, "Out", &comp_name, "A")?;
        self.add_net(&right_wire, "Out", &comp_name, "B")?;

        Ok(format!("{}.{}", comp_name, out_pin))
    }

    fn synthesize_register(
        &mut self,
        clock: &LogicExpression,
        reset: &LogicExpression,
        init: &LogicExpression,
    ) -> Result<String, SynthesisError> {
        let clock_wire = self.synthesize_expression(clock)?;
        let reset_wire = self.synthesize_expression(reset)?;
        let init_wire = self.synthesize_expression(init)?;

        let comp_name = self.generate_component_name("reg");
        let position = Point3D::new(0, 0, 0);
        self.place_component(comp_name.clone(), "Register8".into(), position)?;

        // Connect inputs
        self.add_net(&clock_wire, "Out", &comp_name, "CLK")?;
        self.add_net(&reset_wire, "Out", &comp_name, "RST")?;
        self.add_net(&init_wire, "Out", &comp_name, "D")?;

        Ok(format!("{}.Q", comp_name))
    }

    fn synthesize_if_expression(
        &mut self,
        condition: &LogicExpression,
        then_expr: &BlockOrExpr,
        else_expr: &BlockOrExpr,
    ) -> Result<String, SynthesisError> {
        // Synthesize condition
        let cond_wire = self.synthesize_expression(condition)?;

        // Synthesize then branch
        let then_wire = self.synthesize_block_or_expr(then_expr)?;

        // Synthesize else branch
        let else_wire = self.synthesize_block_or_expr(else_expr)?;

        // Create 2-to-1 MUX
        let mux_name = self.generate_component_name("mux2to1");
        let position = Point3D::new(0, 0, 0);
        self.place_component(mux_name.clone(), "Mux2to1".into(), position)?;

        // Connect: condition -> Sel, then -> In1, else -> In0
        self.add_net(&cond_wire, "Out", &mux_name, "Sel")?;
        self.add_net(&then_wire, "Out", &mux_name, "In1")?;
        self.add_net(&else_wire, "Out", &mux_name, "In0")?;

        Ok(format!("{}.Out", mux_name))
    }

    fn synthesize_match_expression(
        &mut self,
        selector: &LogicExpression,
        arms: &[MatchArm],
    ) -> Result<String, SynthesisError> {
        let selector_wire = self.synthesize_expression(selector)?;

        // Determine MUX size based on number of arms
        let mux_type = match arms.len() {
            2 => "Mux2to1",
            4 => "Mux4to1",
            8 => "Mux8to1",
            _ => {
                return Err(SynthesisError::internal(
                    format!("Unsupported match arm count: {}", arms.len()).into(),
                    None,
                ))
            }
        };

        let mux_name = self.generate_component_name("mux");
        let position = Point3D::new(0, 0, 0);
        self.place_component(mux_name.clone(), mux_type.into(), position)?;

        // Connect selector
        self.add_net(&selector_wire, "Out", &mux_name, "Sel")?;

        // Connect each arm
        for (i, arm) in arms.iter().enumerate() {
            let arm_wire = self.synthesize_block_or_expr(&arm.body)?;
            let input_pin = format!("In{}", i);
            self.add_net(&arm_wire, "Out", &mux_name, &input_pin)?;
        }

        Ok(format!("{}.Out", mux_name))
    }

    fn synthesize_block_or_expr(
        &mut self,
        block_or_expr: &BlockOrExpr,
    ) -> Result<String, SynthesisError> {
        match block_or_expr {
            BlockOrExpr::Expression(expr) => self.synthesize_expression(expr),
            BlockOrExpr::Block(statements) => {
                // Synthesize all statements in the block
                let mut last_result = None;
                for statement in statements {
                    match statement {
                        LogicStatement::Expression(expr) => {
                            last_result = Some(self.synthesize_expression(expr)?);
                        }
                        LogicStatement::Let { expression, .. } => {
                            last_result = Some(self.synthesize_expression(expression)?);
                        }
                        _ => {
                            self.synthesize_statement(statement)?;
                        }
                    }
                }
                last_result.ok_or_else(|| {
                    SynthesisError::internal("Block has no result expression".into(), None)
                })
            }
            BlockOrExpr::Pass(_) => {
                // Pass means no value - create a zero constant
                let zero_name = self.generate_component_name("zero");
                let position = Point3D::new(0, 0, 0);
                self.place_component(zero_name.clone(), "Constant".into(), position)?;
                Ok(zero_name.to_string())
            }
        }
    }

    // Stub methods for wire registration (copied from old implementation)
    fn register_wires_recursive(
        &mut self,
        _statement: &LogicStatement,
    ) -> Result<(), SynthesisError> {
        // TODO: Implement wire registration
        Ok(())
    }

    fn register_let_statements_recursive(
        &mut self,
        _statement: &LogicStatement,
    ) -> Result<(), SynthesisError> {
        // TODO: Implement let statement registration
        Ok(())
    }
}
