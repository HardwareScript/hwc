//! HardwareScript v0.3.0 Compiler Core (`hwc-compiler`)
//!
//! Provides the Compile-Time Evaluation Engine (`hwc-eval`), Module Resolver,
//! Symbol Table, and Native Emitter bridges.

use compact_str::CompactString;
use rustc_hash::FxHashMap;
use hwc_engine::material::MaterialPhysicalProps;

pub mod embedded_stdlib;
pub mod error_codes;
pub mod eval;
pub mod ir;
pub mod module_resolver;
pub mod prelude;
pub mod span_utils;
pub mod symbol_table;

pub use eval::{
    eval_expression_bytecode, eval_expression_str, run_script, ControlFlow, EscapeEnvelope,
    EvalError, EvaluationContext, Evaluator, MemoryEmitter, MeasurementValue, PhysicalDimension,
    PhysicalValue, SandboxGuard, SpaceEmitter, UnitDimension, Value,
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

/// Helper function to extract numeric value from Expression for material properties
fn extract_numeric_value(expr: &hwc_parser::Expression) -> Option<f64> {
    match expr {
        hwc_parser::Expression::Literal { value, .. } => Some(*value as f64),
        hwc_parser::Expression::FloatLiteral { value, .. } => Some(*value),
        hwc_parser::Expression::Measurement { value, .. } => Some(*value),
        _ => None,
    }
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

    // Populate ctx.functions, ctx.structs, and enums from the symbol table arena
    // This includes all imported symbols that were registered by the module resolver
    eprintln!("[PIPELINE DEBUG] Loading {} functions, {} structs, {} enums from symbol table arena", 
        _symbol_table.arena().function_defs.len(),
        _symbol_table.arena().struct_defs.len(),
        _symbol_table.arena().enum_defs.len()
    );
    
    for func_def in _symbol_table.arena().function_defs.iter() {
        ctx.functions.insert(func_def.name.name.clone(), func_def.clone());
    }
    
    for struct_def in _symbol_table.arena().struct_defs.iter() {
        ctx.structs.insert(struct_def.name.name.clone(), struct_def.clone());
    }
    
    eprintln!("[PIPELINE DEBUG] Registered Structs in Context: {:?}", ctx.structs.keys().collect::<Vec<_>>());
    eprintln!("[PIPELINE DEBUG] Registered Functions in Context: {:?}", ctx.functions.keys().collect::<Vec<_>>());
    
    // Add enum variant bindings to the evaluation context
    // For enums, we store the enum type (as a namespace) in ctx.enum_types
    for enum_def in _symbol_table.arena().enum_defs.iter() {
        // Create a map of variant names to their values for this enum
        let mut variants_map = FxHashMap::default();
        for variant in &enum_def.variants {
            let variant_value = eval::Value::EnumVariant {
                enum_name: enum_def.name.name.clone(),
                variant_name: variant.name.clone(),
                payload: None,
            };
            variants_map.insert(variant.name.clone(), variant_value);
        }
        
        // Store the enum type in ctx.enum_types for the bytecode compiler
        let enum_namespace = eval::Value::EnumType {
            name: enum_def.name.name.clone(),
            variants: std::sync::Arc::new(variants_map),
        };
        ctx.enum_types.insert(enum_def.name.name.clone(), enum_namespace);
    }

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

    // 1. Build universal MaterialRegistry from symbol table
    let mut base_material_registry = hwc_engine::MaterialRegistry::new();
    for mat_def in _symbol_table.arena().material_defs.iter() {
        let category = mat_def.category();
        let parser_process = mat_def.get_process();
        let process = match parser_process {
            hwc_parser::ManufacturingProcess::DrilledPlated => hwc_engine::ManufacturingProcess::DrilledPlated,
            hwc_parser::ManufacturingProcess::Deposited => hwc_engine::ManufacturingProcess::Deposited,
            hwc_parser::ManufacturingProcess::Etched => hwc_engine::ManufacturingProcess::Etched,
        };
        let mat_id = base_material_registry.register_with_properties(
            mat_def.name.as_str(),
            category,
            process,
        );
        
        // Extract and register physical properties
        let mut physical_props = MaterialPhysicalProps::new();
        for (prop_name, prop_value) in &mat_def.properties {
            // Extract numeric value from the property expression
            if let Some(value) = extract_numeric_value(prop_value) {
                physical_props.set(prop_name.as_str(), value);
            }
        }
        if !physical_props.properties.is_empty() {
            base_material_registry.set_physical_props(mat_id, physical_props);
        }
    }

    // Ensure common PDK materials are registered
    base_material_registry.register_with_properties("Polysilicon", hwc_parser::MaterialCategory::Conductor, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("Aluminum", hwc_parser::MaterialCategory::Conductor, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("Tungsten", hwc_parser::MaterialCategory::Conductor, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("Titanium_Silicide", hwc_parser::MaterialCategory::Conductor, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("Silicon_Dioxide", hwc_parser::MaterialCategory::Insulator, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("Resistor_Poly_Mask", hwc_parser::MaterialCategory::Mask, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("P_Plus_Diffusion", hwc_parser::MaterialCategory::Semiconductor, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("P_Plus_Implant_Mask", hwc_parser::MaterialCategory::Mask, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("N_Plus_Implant_Mask", hwc_parser::MaterialCategory::Mask, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("Tap_Mask", hwc_parser::MaterialCategory::Mask, hwc_engine::ManufacturingProcess::Deposited);
    base_material_registry.register_with_properties("Pad_Mask", hwc_parser::MaterialCategory::Mask, hwc_engine::ManufacturingProcess::Deposited);

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
                material_registry: base_material_registry.clone(),
                view: hwc_engine::space::SpaceView::Horizontal,
                manufacturing_grid_nm: 10,
                technology_strategy: hwc_types::Technology::Asic,
            };

            let mut hw_space = hwc_engine::HardwareSpace::new(params);

            // 2. Resolve stackup layers from profile
            if let Some(prof_ident) = &space_decl.profile {
                if let Ok(prof_decl) = _symbol_table.get_profile(prof_ident.as_str()) {
                    let mut current_z = 0i64;
                    for sec in &prof_decl.sections {
                        if sec.section_type == "stackup" {
                            for (layer_name, expr) in &sec.fields {
                                let mut mat_name: CompactString = "Polysilicon".into();
                                let mut thickness_nm = 100i64;
                                let mut routable = true;

                                if let hwc_parser::ast::Expression::StructInstance { fields, .. } = expr {
                                    for fi in fields {
                                        let fexpr = match &fi.value { Some(e) => e, None => continue };
                                        match fi.name.as_str() {
                                            "material" => {
                                                if let hwc_parser::ast::Expression::StringLiteral { value, .. } = fexpr {
                                                    mat_name = value.as_str().into();
                                                } else if let hwc_parser::ast::Expression::Variable { name, .. } = fexpr {
                                                    mat_name = name.clone();
                                                }
                                            }
                                            "thickness" => {
                                                if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = fexpr {
                                                    if let Ok(nm) = unit.to_nanometers(*value) {
                                                        thickness_nm = nm as i64;
                                                    }
                                                }
                                            }
                                            "routable" => {
                                                if let hwc_parser::ast::Expression::Literal { value, .. } = fexpr {
                                                    routable = *value != 0;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }

                                let is_mask = thickness_nm == 0;
                                let category = hw_space.material_registry.get_category_by_name(&mat_name)
                                    .unwrap_or(if is_mask { hwc_parser::MaterialCategory::Mask } else { hwc_parser::MaterialCategory::Conductor });
                                let kind = hwc_engine::stackup::LayerKind::from_material_category(category);
                                let z_bottom = current_z;
                                let z_top = current_z + thickness_nm;
                                current_z = z_top;

                                hw_space.stackup_layers.push(hwc_engine::space::StackupLayer::new(
                                    layer_name.clone(),
                                    z_bottom,
                                    z_top,
                                    thickness_nm,
                                    mat_name,
                                    routable,
                                    is_mask,
                                    kind,
                                ));
                            }
                        }
                    }
                    let mut via_shape: Option<CompactString> = None;
                    let mut min_via_dia_nm = 170i64;
                    let mut min_via_encl_nm = 40i64;
                    let mut min_via_spc_nm = 200i64;
                    let mut via_contact_depth_nm = 0i64;
                    let mut min_trace_w_nm = 300i64;
                    let mut min_trace_spc_nm = 300i64;
                    let mut circle_segments = 64u32;
                    let mut mfg_grid_nm = 10i64;
                    let mut substrate_net_name: Option<CompactString> = None;
                    let mut thermal_constraints: Option<hwc_materials::ThermalConstraints> = None;
                    let mut clearance_high_v_nm = 1000i64;
                    let mut clearance_safety_factor = 2.0f64;

                    for sec in &prof_decl.sections {
                        match sec.section_type.as_str() {
                            "technology" => {
                                for (field_name, field_expr) in &sec.fields {
                                    if field_name == "substrate_net" {
                                        if let hwc_parser::ast::Expression::StringLiteral { value, .. } = field_expr {
                                            substrate_net_name = Some(value.as_str().into());
                                        } else if let hwc_parser::ast::Expression::Variable { name, .. } = field_expr {
                                            substrate_net_name = Some(name.as_str().into());
                                        }
                                    }
                                }
                            }
                            "via" => {
                                for (field_name, field_expr) in &sec.fields {
                                    match field_name.as_str() {
                                        "shape" => {
                                            if let hwc_parser::ast::Expression::StringLiteral { value, .. } = field_expr {
                                                via_shape = Some(value.as_str().into());
                                            } else if let hwc_parser::ast::Expression::Variable { name, .. } = field_expr {
                                                via_shape = Some(name.as_str().into());
                                            }
                                        }
                                        "min_diameter" => {
                                            if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                                if let Ok(nm) = unit.to_nanometers(*value) {
                                                    min_via_dia_nm = nm as i64;
                                                }
                                            }
                                        }
                                        "min_enclosure" => {
                                            if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                                if let Ok(nm) = unit.to_nanometers(*value) {
                                                    min_via_encl_nm = nm as i64;
                                                }
                                            }
                                        }
                                        "min_spacing" => {
                                            if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                                if let Ok(nm) = unit.to_nanometers(*value) {
                                                    min_via_spc_nm = nm as i64;
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "trace" => {
                                for (field_name, field_expr) in &sec.fields {
                                    match field_name.as_str() {
                                        "min_width" => {
                                            if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                                if let Ok(nm) = unit.to_nanometers(*value) {
                                                    min_trace_w_nm = nm as i64;
                                                }
                                            }
                                        }
                                        "min_spacing" => {
                                            if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                                if let Ok(nm) = unit.to_nanometers(*value) {
                                                    min_trace_spc_nm = nm as i64;
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "manufacturing" => {
                                for (field_name, field_expr) in &sec.fields {
                                    match field_name.as_str() {
                                        "circle_segments" => {
                                            if let hwc_parser::ast::Expression::Literal { value, .. } = field_expr {
                                                circle_segments = *value as u32;
                                            }
                                        }
                                        "track_pitch" => {
                                            if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                                if let Ok(nm) = unit.to_nanometers(*value) {
                                                    mfg_grid_nm = nm as i64;
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "clearance" => {
                                for (field_name, field_expr) in &sec.fields {
                                    match field_name.as_str() {
                                        "high_voltage" => {
                                            if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                                if let Ok(nm) = unit.to_nanometers(*value) {
                                                    clearance_high_v_nm = nm as i64;
                                                }
                                            }
                                        }
                                        "safety_factor" => {
                                            if let hwc_parser::ast::Expression::Literal { value, .. } = field_expr {
                                                clearance_safety_factor = *value as f64;
                                            } else if let hwc_parser::ast::Expression::FloatLiteral { value, .. } = field_expr {
                                                clearance_safety_factor = *value;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "thermal" => {
                                let mut ambient = 25.0f64;
                                let mut max_op = 125.0f64;
                                let mut max_rise = 50.0f64;
                                let mut clustering_thresh_nm: Option<i64> = None;

                                for (field_name, field_expr) in &sec.fields {
                                    match field_name.as_str() {
                                        "ambient_temp" => {
                                            if let Some(v) = extract_numeric_value(field_expr) {
                                                ambient = v;
                                            }
                                        }
                                        "max_operating_temp" => {
                                            if let Some(v) = extract_numeric_value(field_expr) {
                                                max_op = v;
                                            }
                                        }
                                        "max_temp_rise" => {
                                            if let Some(v) = extract_numeric_value(field_expr) {
                                                max_rise = v;
                                            }
                                        }
                                        "clustering_threshold" => {
                                            if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                                if let Ok(nm) = unit.to_nanometers(*value) {
                                                    clustering_thresh_nm = Some(nm as i64);
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                thermal_constraints = Some(hwc_materials::ThermalConstraints {
                                    ambient_temp_c: ambient,
                                    max_operating_temp_c: max_op,
                                    max_temp_rise_c: max_rise,
                                    clustering_threshold_nm: clustering_thresh_nm,
                                });
                            }
                            _ => {}
                        }
                    }

                    let fab_constraints = hwc_materials::ConstraintSet {
                        name: prof_ident.name.clone(),
                        description: "".into(),
                        trace: hwc_materials::TraceConstraints {
                            min_width_nm: min_trace_w_nm,
                            max_width_nm: 0,
                            min_spacing_nm: min_trace_spc_nm,
                            default_width_nm: min_trace_w_nm,
                        },
                        via: hwc_materials::ViaConstraints {
                            min_diameter_nm: min_via_dia_nm,
                            max_diameter_nm: 0,
                            min_enclosure_nm: min_via_encl_nm,
                            min_spacing_nm: min_via_spc_nm,
                            default_diameter_nm: min_via_dia_nm,
                            contact_depth_nm: via_contact_depth_nm,
                            material_contact_depths_nm: rustc_hash::FxHashMap::default(),
                            min_contact_depth_nm: None,
                            max_contact_depth_nm: None,
                            shape: via_shape,
                            layer_enclosures_nm: rustc_hash::FxHashMap::default(),
                        },
                        clearance: hwc_materials::ClearanceConstraints {
                            low_voltage_nm: 300,
                            medium_voltage_nm: 600,
                            high_voltage_nm: clearance_high_v_nm,
                            safety_factor: clearance_safety_factor,
                            max_substrate_tap_distance_nm: None,
                        },
                        layer: hwc_materials::LayerConstraints {
                            min_thickness_nm: 50,
                            max_thickness_nm: 0,
                            allowed_conductors: Vec::new(),
                            allowed_dielectrics: Vec::new(),
                        },
                        thermal: thermal_constraints,
                        stackup: None,
                        bridges: Vec::new(),
                        circle_segments,
                        technology: hwc_types::Technology::Asic,
                        layer_routability: rustc_hash::FxHashMap::default(),
                        max_local_route_length_nm: None,
                        intents: Vec::new(),
                        manufacturing_grid_nm: mfg_grid_nm,
                        substrate_net: substrate_net_name,
                    };
                    hw_space.fabrication_constraints = Some(fab_constraints);
                }
            }

            // Require explicit profile stackup (no fallbacks)
            if hw_space.stackup_layers.is_empty() {
                return Err(PipelineError {
                    message: format!("Space '{}' requires a valid profile with a 'stackup' section", space_name),
                });
            }

            // Update space dimensions depth with true total stackup thickness
            let total_depth_nm = hw_space.stackup_layers.iter().map(|l| l.z_top).max().unwrap_or(0);
            hw_space.dimensions.depth_nm = total_depth_nm;

            // Inject dielectric substrate slabs (die boundary) into EntityGraph for 2D/3D CAD & DXF
            for st in &hw_space.stackup_layers {
                if !st.is_mask {
                    let mat_id = hw_space.material_registry.get_id(&st.material_name).unwrap_or(0);
                    if hw_space.material_registry.is_insulator(mat_id) || st.material_name == "Silicon_Dioxide" {
                        let die_bbox = hwc_engine::BoundingBox::new(
                            hwc_engine::Point3D::new(0, 0, st.z_bottom),
                            hwc_engine::Point3D::new(width_nm, height_nm, st.z_top),
                        );
                        let substrate_slab = hwc_engine::geometry_router::substrate_types::SubstrateLayer::new(
                            mat_id,
                            hwc_engine::netlist::NetId::UNCONNECTED,
                            die_bbox,
                            hwc_physics::connectivity::SubstrateLayerType::Substrate,
                        );
                        hw_space.entity_graph.substrate_layers.push(substrate_slab);
                    }
                }
            }

            // Register nets in hw_space.netlist
            let default_route_mat_id = hw_space.material_registry.get_id("Aluminum").unwrap_or(0);
            
            for net_decl in &space_decl.nets {
                let net_name: CompactString = net_decl.name.as_str().into();
                let mut net_mat_id = default_route_mat_id;
                let mut net_width_nm = 300i64;

                if let Some(mat_expr) = net_decl.get_property("material") {
                    let mat_name = match mat_expr {
                        hwc_parser::ast::Expression::StringLiteral { value, .. } => Some(value.as_str()),
                        hwc_parser::ast::Expression::Variable { name, .. } => Some(name.as_str()),
                        _ => None,
                    };
                    if let Some(name) = mat_name {
                        if let Some(id) = hw_space.material_registry.get_id(name) {
                            net_mat_id = id;
                        }
                    }
                }

                if let Some(w_expr) = net_decl.get_property("width") {
                    if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = w_expr {
                        if let Ok(nm) = unit.to_nanometers(*value) {
                            net_width_nm = nm as i64;
                        }
                    }
                }

                hw_space.netlist.add_net(net_name, net_width_nm, net_mat_id);
            }

            // Build reverse NetId -> NetName mapping for this space
            let mut net_id_to_name: FxHashMap<hwc_types::NetId, CompactString> = FxHashMap::default();
            for (name, id) in &mem.nets {
                net_id_to_name.insert(*id, name.clone());
            }

            // 3. Populate pours from polygons & inject into EntityGraph
            let mut pour_counters: FxHashMap<(CompactString, Option<CompactString>), usize> = FxHashMap::default();
            
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

                // Resolve layer Z elevations and material
                let st = hw_space.stackup_layers.iter().find(|l| l.name == poly.layer)
                    .ok_or_else(|| PipelineError {
                        message: format!(
                            "Polygon '{}' references layer '{}' which is not defined in profile '{}'. Available layers: {}",
                            poly.semantic_name.as_deref().unwrap_or("unnamed"),
                            poly.layer,
                            space_decl.profile.as_ref().map(|p| p.as_str()).unwrap_or("None"),
                            hw_space.stackup_layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                    })?;
                let (z_bottom, z_top, mat_name) = (st.z_bottom, st.z_top, st.material_name.clone());

                let mat_id = hw_space.material_registry.get_id(&mat_name)
                    .ok_or_else(|| PipelineError {
                        message: format!(
                            "Polygon '{}' on layer '{}' references material '{}' which is not defined. Available materials: {}",
                            poly.semantic_name.as_deref().unwrap_or("unnamed"),
                            poly.layer,
                            mat_name,
                            hw_space.material_registry.all_materials()
                                .iter()
                                .map(|(_, name)| *name)
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    })?;

                let bbox = if min_x <= max_x && min_y <= max_y {
                    Some(hwc_engine::BoundingBox::new(
                        hwc_engine::Point3D::new(min_x, min_y, z_bottom),
                        hwc_engine::Point3D::new(max_x, max_y, z_top),
                    ))
                } else {
                    None
                };

                let net_name = poly.net.and_then(|id| net_id_to_name.get(&id).cloned());

                let w = (max_x - min_x).max(0);
                let h = (max_y - min_y).max(0);

                if let Some(b) = bbox {
                    let engine_net = poly.net.map(|id| hwc_engine::netlist::NetId::new(id.0)).unwrap_or(hwc_engine::netlist::NetId::UNCONNECTED);
                    let substrate_layer = hwc_engine::geometry_router::substrate_types::SubstrateLayer::new(
                        mat_id,
                        engine_net,
                        b,
                        hwc_physics::connectivity::SubstrateLayerType::Pour,
                    );
                    hw_space.entity_graph.substrate_layers.push(substrate_layer);
                }

                // v0.3.0: Generate semantic pour names based on net and layer
                // Format: <Net>_<Material>_<Layer> or just <Material>_<Layer> if no net
                let pour_name = if let Some(semantic_name) = &poly.semantic_name {
                    semantic_name.clone()
                } else {
                    // Generate semantic name: NetName_Layer or just Layer_N if no net
                    let counter_key = (poly.layer.clone(), net_name.clone());
                    let counter = pour_counters.entry(counter_key).or_insert(0);
                    *counter += 1;
                    
                    if let Some(ref net) = net_name {
                        if *counter == 1 {
                            // First pour on this net+layer: use simpler name
                            CompactString::new(format!("{}_{}", net, poly.layer))
                        } else {
                            // Multiple pours: add counter
                            CompactString::new(format!("{}_{}_{}", net, poly.layer, *counter - 1))
                        }
                    } else {
                        // No net: use layer name with counter
                        CompactString::new(format!("{}_{}", poly.layer, idx))
                    }
                };

                if let Some(ref net) = net_name {
                    let engine_net_id = hw_space.netlist.get_or_create_net(net.as_str());
                    let comp_id = hw_space.netlist.add_component(pour_name.clone(), poly.layer.clone(), (min_x, min_y, z_bottom));
                    let pin_anchor = hw_space.netlist.add_pin(comp_id, "anchor".into(), (0, 0, 0), None);
                    hw_space.netlist.connect_pin(pin_anchor, engine_net_id);
                    let pin_virt = hw_space.netlist.add_pin(comp_id, format!("__virtual_{}", pour_name).into(), (0, 0, 0), None);
                    hw_space.netlist.connect_pin(pin_virt, engine_net_id);
                }

                hw_space.pours.push(hwc_engine::PourMetadata {
                    name: pour_name,
                    material_name: mat_name,
                    layer_name: poly.layer.clone(),
                    z_bottom_nm: z_bottom,
                    net: net_name,
                    area_nm2: w * h,
                    bbox,
                    device_binding: None,
                    merged_region_id: None,
                    waivers: hwc_parser::Waivers::default(),
                });
            }

            // 4. Populate contacts & inject into EntityGraph
            let mut via_counters: FxHashMap<(CompactString, CompactString, Option<CompactString>), usize> = FxHashMap::default();
            
            for (idx, contact) in mem.contacts.iter().enumerate() {
                let x_nm = contact.at.0 / 1000;
                let y_nm = contact.at.1 / 1000;
                let dia_nm = contact.diameter_pm / 1000;
                let r_nm = dia_nm / 2;

                let from_st = hw_space.stackup_layers.iter().find(|l| l.name == contact.from_layer)
                    .ok_or_else(|| PipelineError {
                        message: format!(
                            "Contact '{}' references from_layer '{}' which is not defined in profile '{}'. Available layers: {}",
                            contact.semantic_name.as_deref().unwrap_or(&format!("contact_{}", idx)),
                            contact.from_layer,
                            space_decl.profile.as_ref().map(|p| p.as_str()).unwrap_or("None"),
                            hw_space.stackup_layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                    })?;
                let to_st = hw_space.stackup_layers.iter().find(|l| l.name == contact.to_layer)
                    .ok_or_else(|| PipelineError {
                        message: format!(
                            "Contact '{}' references to_layer '{}' which is not defined in profile '{}'. Available layers: {}",
                            contact.semantic_name.as_deref().unwrap_or(&format!("contact_{}", idx)),
                            contact.to_layer,
                            space_decl.profile.as_ref().map(|p| p.as_str()).unwrap_or("None"),
                            hw_space.stackup_layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                    })?;
                
                eprintln!("[VIA Z-RANGE DEBUG] Contact from='{}' to='{}'", contact.from_layer, contact.to_layer);
                eprintln!("[VIA Z-RANGE DEBUG]   FROM layer: name='{}', z_bottom={}nm, z_top={}nm, thickness={}nm", 
                    from_st.name, from_st.z_bottom, from_st.z_top, from_st.z_top - from_st.z_bottom);
                eprintln!("[VIA Z-RANGE DEBUG]   TO layer: name='{}', z_bottom={}nm, z_top={}nm, thickness={}nm", 
                    to_st.name, to_st.z_bottom, to_st.z_top, to_st.z_top - to_st.z_bottom);
                
                // Read contact_depth and min_enclosure from profile
                let (contact_depth_pct, min_enclosure_nm) = if let Some(prof_ident) = &space_decl.profile {
                    if let Ok(prof_decl) = _symbol_table.get_profile(prof_ident.as_str()) {
                        let mut found_depth = None;
                        let mut found_enclosure = None;
                        for sec in &prof_decl.sections {
                            if sec.section_type == "via" {
                                for (field_name, field_expr) in &sec.fields {
                                    if field_name == "contact_depth" {
                                        match field_expr {
                                            hwc_parser::ast::Expression::StringLiteral { value, .. } => {
                                                if value.ends_with('%') {
                                                    if let Ok(pct) = value.trim_end_matches('%').parse::<f64>() {
                                                        found_depth = Some(pct / 100.0);
                                                    }
                                                }
                                            }
                                            hwc_parser::ast::Expression::Literal { value, .. } => {
                                                found_depth = Some(*value as f64 / 100.0);
                                            }
                                            _ => {}
                                        }
                                    } else if field_name == "min_enclosure" {
                                        if let hwc_parser::ast::Expression::Measurement { value, unit, .. } = field_expr {
                                            if let Ok(nm) = unit.to_nanometers(*value) {
                                                found_enclosure = Some(nm as i64);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        (found_depth.unwrap_or(0.30), found_enclosure.unwrap_or(0))
                    } else {
                        (0.30, 0)
                    }
                } else {
                    (0.30, 0)
                };
                
                // Calculate penetration depths
                let from_thickness = from_st.z_top - from_st.z_bottom;
                let to_thickness = to_st.z_top - to_st.z_bottom;
                
                let from_penetration = (from_thickness as f64 * contact_depth_pct) as i64;
                let via_z_start = from_st.z_top - from_penetration;
                
                let to_penetration = (to_thickness as f64 * contact_depth_pct) as i64;
                let via_z_end = to_st.z_bottom + to_penetration;
                
                let (z_start_nm, z_end_nm) = (via_z_start, via_z_end);

                let footprint_r_nm = r_nm + min_enclosure_nm;
                let bbox = Some(hwc_engine::BoundingBox::new(
                    hwc_engine::Point3D::new(x_nm - footprint_r_nm, y_nm - footprint_r_nm, z_start_nm),
                    hwc_engine::Point3D::new(x_nm + footprint_r_nm, y_nm + footprint_r_nm, z_end_nm),
                ));

                let mat_name: CompactString = "Tungsten".into();
                let mat_id = hw_space.material_registry.get_id("Tungsten")
                    .ok_or_else(|| PipelineError {
                        message: format!(
                            "Via/contact material 'Tungsten' is not defined in the material registry. \
                             Vias require Tungsten to be declared in the material definitions. \
                             Available materials: {}",
                            hw_space.material_registry.all_materials()
                                .iter()
                                .map(|(_, name)| *name)
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    })?;

                // Check profile for via shape specification
                let via_shape: Option<String> = if let Some(prof_ident) = &space_decl.profile {
                    if let Ok(prof_decl) = _symbol_table.get_profile(prof_ident.as_str()) {
                        let mut found_shape = None;
                        for sec in &prof_decl.sections {
                            if sec.section_type == "via" {
                                for (field_name, field_expr) in &sec.fields {
                                    if field_name == "shape" {
                                        if let hwc_parser::ast::Expression::StringLiteral { value, .. } = field_expr {
                                            found_shape = Some(value.to_string());
                                            break;
                                        } else if let hwc_parser::ast::Expression::Variable { name, .. } = field_expr {
                                            found_shape = Some(name.to_string());
                                            break;
                                        }
                                    }
                                }
                                if found_shape.is_some() {
                                    break;
                                }
                            }
                        }
                        found_shape
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(b) = bbox {
                    let engine_net = contact.net.map(|id| hwc_engine::netlist::NetId::new(id.0)).unwrap_or(hwc_engine::netlist::NetId::UNCONNECTED);
                    
                    let substrate_contact = if via_shape.as_deref() == Some("square") {
                        hwc_engine::geometry_router::substrate_types::SubstrateLayer::new_square_via(
                            mat_id,
                            engine_net,
                            b,
                            dia_nm + (2 * min_enclosure_nm), // side length with enclosure
                        )
                    } else {
                        hwc_engine::geometry_router::substrate_types::SubstrateLayer::new_contact_circle(
                            mat_id,
                            engine_net,
                            b,
                            footprint_r_nm,
                        )
                    };
                    hw_space.entity_graph.substrate_layers.push(substrate_contact);
                }

                let net_name = contact.net.and_then(|id| net_id_to_name.get(&id).cloned());

                let contact_name = if let Some(semantic_name) = &contact.semantic_name {
                    semantic_name.clone()
                } else {
                    let counter_key = (contact.from_layer.clone(), contact.to_layer.clone(), net_name.clone());
                    let counter = via_counters.entry(counter_key).or_insert(0);
                    *counter += 1;
                    
                    if let Some(ref net) = net_name {
                        if *counter == 1 {
                            CompactString::new(format!("Via_{}_{}_{}", net, contact.from_layer, contact.to_layer))
                        } else {
                            CompactString::new(format!("Via_{}_{}_{}_{}", net, contact.from_layer, contact.to_layer, *counter - 1))
                        }
                    } else {
                        CompactString::new(format!("Via_{}_{}_{}", contact.from_layer, contact.to_layer, idx))
                    }
                };

                if let Some(ref net) = net_name {
                    let engine_net_id = hw_space.netlist.get_or_create_net(net.as_str());
                    let comp_id = hw_space.netlist.add_component(contact_name.clone(), "via".into(), (x_nm, y_nm, z_start_nm));
                    let pin_virt = hw_space.netlist.add_pin(comp_id, format!("__virtual_{}", contact_name).into(), (0, 0, 0), None);
                    hw_space.netlist.connect_pin(pin_virt, engine_net_id);
                }

                let from_layer_id = hw_space.get_layer_id(&contact.from_layer);
                let to_layer_id = hw_space.get_layer_id(&contact.to_layer);
                let engine_net_id = contact.net.map(|id| hwc_types::NetId::new(id.0));

                let aperture = if via_shape.as_deref() == Some("square") {
                    hwc_types::ViaApertureShape::Square
                } else if via_shape.as_deref() == Some("polygon") {
                    hwc_types::ViaApertureShape::Polygon
                } else {
                    hwc_types::ViaApertureShape::Circular
                };

                let is_internal_head_tail = contact.from_layer == "polyres" || (contact.from_layer == "poly" && contact.to_layer == "li1");
                let exemption = if is_internal_head_tail {
                    hwc_types::ContactExemption::SubcircuitInternal { device_id: 0 }
                } else {
                    hwc_types::ContactExemption::Interconnect
                };

                hw_space.contacts.push(hwc_engine::ContactMetadata {
                    name: contact_name,
                    material_name: mat_name,
                    material_id: Some(mat_id),
                    z_start_nm,
                    z_end_nm,
                    net: net_name,
                    net_id: engine_net_id,
                    bridge: None,
                    bbox,
                    drill_diameter_nm: Some(dia_nm),
                    is_tented: false,
                    mask_clearance_diameter_nm: None,
                    from_layer: Some(contact.from_layer.clone()),
                    from_layer_id,
                    to_layer: Some(contact.to_layer.clone()),
                    to_layer_id,
                    aperture,
                    exemption,
                });
            }

            // 5. Populate devices
            for dev in &mem.devices {
                let mut terms = Vec::new();
                let mut term_nets = FxHashMap::default();
                for (term_name, net_id) in &dev.terminals {
                    terms.push(term_name.clone());
                    let resolved_net = net_id_to_name
                        .get(net_id)
                        .cloned()
                        .unwrap_or_else(|| CompactString::new(format!("NET_{}", net_id.0)));
                    term_nets.insert(term_name.clone(), resolved_net);
                }

                let mut params_map = FxHashMap::default();
                for (p_name, m_val) in &dev.params {
                    params_map.insert(p_name.clone(), (m_val.raw as f64) * 1e-12);
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

            // 6. Populate routes into analytic_routes and entity_graph
            for route in &mem.routes {
                if let (Ok(p1), Ok(p2)) = (route.from.coerce_to_point2d(), route.to.coerce_to_point2d()) {
                    if let (eval::Value::Point2D { x: x1, y: y1 }, eval::Value::Point2D { x: x2, y: y2 }) = (p1, p2) {
                        let pt1_nm = (x1 / 1000, y1 / 1000);
                        let pt2_nm = (x2 / 1000, y2 / 1000);
                        let mut resolved_net_name: Option<CompactString> = None;
                        let mut resolved_net_id = hwc_engine::netlist::NetId::UNCONNECTED;

                        for pour in &hw_space.pours {
                            if let (Some(ref bbox), Some(ref net_name)) = (&pour.bbox, &pour.net) {
                                if (pt1_nm.0 >= bbox.min.x && pt1_nm.0 <= bbox.max.x && pt1_nm.1 >= bbox.min.y && pt1_nm.1 <= bbox.max.y)
                                    || (pt2_nm.0 >= bbox.min.x && pt2_nm.0 <= bbox.max.x && pt2_nm.1 >= bbox.min.y && pt2_nm.1 <= bbox.max.y)
                                {
                                    resolved_net_name = Some(net_name.clone());
                                    if let Some(id) = mem.nets.get(net_name) {
                                        resolved_net_id = hwc_engine::netlist::NetId::new(id.0);
                                    }
                                    break;
                                }
                            }
                        }

                        // v0.3.0 FIX: Extract layer from route properties, default to metal1
                        let layer_name = route.properties.get("layer")
                            .and_then(|v| match v {
                                eval::Value::String(s) => Some(s.as_str()),
                                _ => None
                            })
                            .unwrap_or("metal1");

                        let net_label = resolved_net_name.clone().unwrap_or_else(|| "ROUTE".into());

                        let layer_st = hw_space.stackup_layers.iter().find(|l| l.name == layer_name);
                        let z_min = layer_st.map(|l| l.z_bottom).unwrap_or(630);
                        let z_max = layer_st.map(|l| l.z_top).unwrap_or(990);
                        let z_center = (z_min + z_max) / 2;

                        // v0.3.0 FIX: Use material from the routing layer's stackup definition
                        // NO FALLBACK - fail if layer or material is missing
                        let trace_mat_name = layer_st
                            .ok_or_else(|| PipelineError {
                                message: format!(
                                    "Route on net '{}' references unknown layer '{}'. Available layers: {}",
                                    net_label,
                                    layer_name,
                                    hw_space.stackup_layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", ")
                                ),
                            })?
                            .material_name.clone();
                        
                        let trace_mat_id = hw_space.material_registry.get_id(&trace_mat_name)
                            .ok_or_else(|| PipelineError {
                                message: format!(
                                    "Route on layer '{}' requires material '{}' which is not registered. Define this material in your .hw file.",
                                    layer_name, trace_mat_name
                                ),
                            })?;

                        let trace_params = hwc_engine::space::AnalyticTraceParams {
                            net_id: resolved_net_id,
                            cross_section: hwc_engine::space::CrossSection::new(300, (z_max - z_min).max(100)),
                            segments: vec![hwc_engine::space::LineSegment::new(
                                hwc_engine::Point3D::new(pt1_nm.0, pt1_nm.1, z_center),
                                hwc_engine::Point3D::new(pt2_nm.0, pt2_nm.1, z_center),
                            )],
                            material: trace_mat_id,
                            net_name: net_label,
                            current: hwc_engine::space::CurrentRating::new(0.0, 0.0),
                            layer_z_range: Some((z_min, z_max)),
                            layer_name: layer_name.into(),
                        };
                        hw_space.analytic_routes.push(hwc_engine::space::AnalyticTrace::with_layer_z_range(trace_params));

                        let trace_min_x = pt1_nm.0.min(pt2_nm.0) - 150;
                        let trace_max_x = pt1_nm.0.max(pt2_nm.0) + 150;
                        let trace_min_y = pt1_nm.1.min(pt2_nm.1) - 150;
                        let trace_max_y = pt1_nm.1.max(pt2_nm.1) + 150;
                        let trace_bbox = hwc_engine::BoundingBox::new(
                            hwc_engine::Point3D::new(trace_min_x, trace_min_y, z_min),
                            hwc_engine::Point3D::new(trace_max_x, trace_max_y, z_max),
                        );
                        let substrate_trace = hwc_engine::geometry_router::substrate_types::SubstrateLayer::new(
                            trace_mat_id,
                            resolved_net_id,
                            trace_bbox,
                            hwc_physics::connectivity::SubstrateLayerType::Pour,
                        );
                        hw_space.entity_graph.substrate_layers.push(substrate_trace);
                    }
                }
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
