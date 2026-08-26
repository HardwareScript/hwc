//! HardwareScript v0.3.0 Compiler Core (`hwc-compiler`)
//!
//! Provides the Compile-Time Evaluation Engine (`hwc-eval`), Module Resolver,
//! Symbol Table, and Native Emitter bridges.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

pub mod embedded_stdlib;
pub mod error_codes;
pub mod eval;
pub mod ir;
pub mod module_resolver;
pub mod prelude;
pub mod span_utils;
pub mod symbol_table;

pub use eval::{
    ControlFlow, EscapeEnvelope, EvalError, EvaluationContext, Evaluator, MemoryEmitter,
    MeasurementValue, PhysicalDimension, PhysicalValue, SandboxGuard, SpaceEmitter,
    UnitDimension, Value,
};
pub use hwc_diagnostics::{DiagnosticCollector, ErrorFingerprint};
pub use module_resolver::{ModuleResolver, ResolverError};
pub use prelude::{Prelude, PreludeError};
pub use symbol_table::{Definition, SymbolError, SymbolTable};

#[derive(Debug, Clone)]
pub struct CompilationMetadata {
    pub source_file: CompactString,
    pub space_count: usize,
    pub warnings: Vec<CompactString>,
}

/// Helper function to evaluate an entire parsed program with an evaluation context
pub fn evaluate_program(
    program: &hwc_parser::Program,
    ctx: &mut EvaluationContext,
) -> Result<(), EvalError> {
    let mut evaluator = Evaluator::new(ctx);
    evaluator.eval_program(program)
}

// ── v0.3.0 Pipeline Stubs ─────────────────────────────────────────────────────
//
// `program_to_space` and `program_to_spaces_with_lockfile` were v0.2.x pipeline
// entry points that lowered AST → HardwareSpace in a single pass with an embedded
// constraint solver.
//
// In v0.3.0 this is replaced by:
//   1. `evaluate_program` + `SpaceEmitter` → emits geometry into EntityGraph
//   2. DOPHR 3-Stage Guided Router → fills the routing database
//   3. `hwc-export` consumers read the final EntityGraph
//
// These stubs compile existing `hwc-cli` call sites and return a descriptive error
// at runtime until the full v0.3.0 pipeline is wired into the build command.
// ─────────────────────────────────────────────────────────────────────────────

/// Error returned by v0.3.0 pipeline stub functions.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("v0.3.0 pipeline: {message}")]
#[diagnostic(
    code(P00),
    help(
        "The v0.2.x `program_to_space` pipeline has been replaced by `evaluate_program` + \
         SpaceEmitter in v0.3.0. Wire up the comptime evaluator in `hwc-cli/build_cmd` to \
         use the new API."
    )
)]
pub struct PipelineError {
    pub message: String,
}

/// Transform a parsed program into one [`hwc_engine::HardwareSpace`] per `space` declaration.
pub fn program_to_spaces_with_lockfile(
    program: &hwc_parser::Program,
    _symbol_table: &SymbolTable,
    _collector: &DiagnosticCollector,
    _lockfile_path: Option<&std::path::Path>,
    _source_content: Option<&str>,
    _force_reroute: bool,
    unit_registry: &hwc_types::UnitRegistry,
) -> Result<FxHashMap<CompactString, hwc_engine::HardwareSpace>, PipelineError> {
    let memory_emitter = eval::MemoryEmitter::new();
    let mut ctx = eval::EvaluationContext::with_emitter(Box::new(memory_emitter));
    ctx.unit_registry = Some(std::sync::Arc::new(unit_registry.clone()));

    let mut evaluator = eval::Evaluator::new(&mut ctx);
    if let Err(e) = evaluator.eval_program(program) {
        return Err(PipelineError {
            message: format!("Comptime evaluation error: {}", e),
        });
    }

    // Pre-evaluate space dimensions while evaluator is available
    let mut space_dims = FxHashMap::default();
    for item in &program.items {
        if let hwc_parser::TopLevelItem::Space(space_decl) = item {
            let (width_nm, height_nm) = if let Some((w_expr, h_expr)) = &space_decl.dimensions {
                let w = evaluator
                    .eval_expression(w_expr)
                    .ok()
                    .and_then(|v| match v {
                        eval::Value::Measurement(m) => Some((m.raw / 1000) as i64),
                        eval::Value::Int(i) => Some(i),
                        _ => None,
                    })
                    .unwrap_or(20_000);
                let h = evaluator
                    .eval_expression(h_expr)
                    .ok()
                    .and_then(|v| match v {
                        eval::Value::Measurement(m) => Some((m.raw / 1000) as i64),
                        eval::Value::Int(i) => Some(i),
                        _ => None,
                    })
                    .unwrap_or(10_000);
                (w, h)
            } else {
                (20_000, 10_000)
            };
            space_dims.insert(space_decl.name.name.clone(), (width_nm, height_nm));
        }
    }
    drop(evaluator);

    let mut result_spaces = FxHashMap::default();
    let mem = ctx
        .emitter
        .as_any()
        .downcast_ref::<eval::MemoryEmitter>()
        .unwrap();

    for item in &program.items {
        if let hwc_parser::TopLevelItem::Space(space_decl) = item {
            let space_name = space_decl.name.name.clone();
            let (width_nm, height_nm) = space_dims.get(&space_name).copied().unwrap_or((20_000, 10_000));

            let params = hwc_engine::space::HardwareSpaceParams {
                name: space_name.clone(),
                dimensions: hwc_engine::Dimensions {
                    width_nm,
                    height_nm,
                    depth_nm: 10_000,
                },
                substrate_material_id: 0,
                material_registry: hwc_engine::MaterialRegistry::new(),
                view: hwc_engine::space::SpaceView::Horizontal,
                manufacturing_grid_nm: 10,
                technology_strategy: hwc_types::Technology::Asic,
            };

            let mut hw_space = hwc_engine::HardwareSpace::new(params);

            // Populate pours from polygons
            for (idx, poly) in mem.polygons.iter().enumerate() {
                let mut min_x = i64::MAX;
                let mut min_y = i64::MAX;
                let mut max_x = i64::MIN;
                let mut max_y = i64::MIN;
                for (x_pm, y_pm) in &poly.points {
                    let x_nm = x_pm / 1000;
                    let y_nm = y_pm / 1000;
                    min_x = min_x.min(x_nm);
                    min_y = min_y.min(y_nm);
                    max_x = max_x.max(x_nm);
                    max_y = max_y.max(y_nm);
                }

                let bbox = if min_x <= max_x && min_y <= max_y {
                    Some(hwc_engine::BoundingBox::new(
                        hwc_engine::Point3D::new(min_x, min_y, 0),
                        hwc_engine::Point3D::new(max_x, max_y, 0),
                    ))
                } else {
                    None
                };

                let w = (max_x - min_x).max(0);
                let h = (max_y - min_y).max(0);

                hw_space.pours.push(hwc_engine::PourMetadata {
                    name: CompactString::new(format!("POUR_{}_{}", poly.layer, idx)),
                    material_name: poly.layer.clone(),
                    layer_name: poly.layer.clone(),
                    z_bottom_nm: 0,
                    net: poly.net.map(|id| CompactString::new(format!("NET_{}", id.0))),
                    area_nm2: w * h,
                    bbox,
                    device_binding: None,
                    merged_region_id: None,
                    waivers: hwc_parser::Waivers::default(),
                });
            }

            // Populate contacts
            for (idx, contact) in mem.contacts.iter().enumerate() {
                let x_nm = contact.at.0 / 1000;
                let y_nm = contact.at.1 / 1000;
                let dia_nm = contact.diameter_pm / 1000;
                let r_nm = dia_nm / 2;

                hw_space.contacts.push(hwc_engine::ContactMetadata {
                    name: CompactString::new(format!("VIA_{}", idx)),
                    material_name: contact.from_layer.clone(),
                    z_start_nm: 0,
                    z_end_nm: 0,
                    net: contact.net.map(|id| CompactString::new(format!("NET_{}", id.0))),
                    bridge: None,
                    bbox: Some(hwc_engine::BoundingBox::new(
                        hwc_engine::Point3D::new(x_nm - r_nm, y_nm - r_nm, 0),
                        hwc_engine::Point3D::new(x_nm + r_nm, y_nm + r_nm, 0),
                    )),
                    drill_diameter_nm: Some(dia_nm),
                    is_tented: false,
                    mask_clearance_diameter_nm: None,
                    from_layer: Some(contact.from_layer.clone()),
                    to_layer: Some(contact.to_layer.clone()),
                });
            }

            // Populate devices
            for dev in &mem.devices {
                let mut terms = Vec::new();
                let mut term_nets = FxHashMap::default();
                for (term_name, net_id) in &dev.terminals {
                    terms.push(term_name.clone());
                    term_nets.insert(term_name.clone(), CompactString::new(format!("NET_{}", net_id.0)));
                }

                let mut params_map = FxHashMap::default();
                for (p_name, m_val) in &dev.params {
                    params_map.insert(p_name.clone(), m_val.raw as f64 / 1_000_000.0);
                }

                hw_space.device_instances.push(hwc_engine::space::DeviceInstance {
                    name: dev.name.clone(),
                    def_path: None,
                    device_type: dev.device_type.clone(),
                    terminals: terms,
                    terminal_nets: term_nets,
                    parameters: params_map,
                });
            }

            result_spaces.insert(space_name, hw_space);
        }
    }

    Ok(result_spaces)
}

/// Transform a parsed program into a single flattened [`hwc_engine::HardwareSpace`].
pub fn program_to_space(
    program: &hwc_parser::Program,
    symbol_table: &SymbolTable,
    collector: &DiagnosticCollector,
    unit_registry: &hwc_types::UnitRegistry,
) -> Result<hwc_engine::HardwareSpace, PipelineError> {
    let spaces = program_to_spaces_with_lockfile(
        program,
        symbol_table,
        collector,
        None,
        None,
        false,
        unit_registry,
    )?;

    spaces.into_values().next().ok_or_else(|| PipelineError {
        message: "No spaces defined in program".to_string(),
    })
}
