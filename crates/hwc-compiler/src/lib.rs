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
                }
            }

            // Require explicit profile stackup (no fallbacks)
            if hw_space.stackup_layers.is_empty() {
                return Err(PipelineError {
                    message: format!("Space '{}' requires a valid profile with a 'stackup' section", space_name),
                });
            }

            // Register nets in hw_space.netlist
            // v0.3.0: Use Aluminum as default routing material (most common top metal)
            let default_route_mat_id = hw_space.material_registry.get_id("Aluminum").unwrap_or(0);
            
            for net_decl in &space_decl.nets {
                let net_name: CompactString = net_decl.name.as_str().into();
                hw_space.netlist.add_net(net_name, 300, default_route_mat_id);
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
                let (z_bottom, z_top, mat_name) = if let Some(st) = hw_space.stackup_layers.iter().find(|l| l.name == poly.layer) {
                    (st.z_bottom, st.z_top, st.material_name.clone())
                } else {
                    (0, 100, poly.layer.clone())
                };

                let mat_id = hw_space.material_registry.get_id(&mat_name).unwrap_or_else(|| {
                    hw_space.material_registry.register_with_properties(
                        &mat_name,
                        hwc_parser::MaterialCategory::Conductor,
                        hwc_engine::ManufacturingProcess::Deposited,
                    )
                });

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

                let from_st = hw_space.stackup_layers.iter().find(|l| l.name == contact.from_layer);
                let to_st = hw_space.stackup_layers.iter().find(|l| l.name == contact.to_layer);
                
                eprintln!("[VIA Z-RANGE DEBUG] Contact from='{}' to='{}'", contact.from_layer, contact.to_layer);
                if let Some(from) = from_st {
                    eprintln!("[VIA Z-RANGE DEBUG]   FROM layer: name='{}', z_bottom={}nm, z_top={}nm, thickness={}nm", 
                        from.name, from.z_bottom, from.z_top, from.z_top - from.z_bottom);
                } else {
                    eprintln!("[VIA Z-RANGE DEBUG]   FROM layer: NOT FOUND in stackup!");
                }
                if let Some(to) = to_st {
                    eprintln!("[VIA Z-RANGE DEBUG]   TO layer: name='{}', z_bottom={}nm, z_top={}nm, thickness={}nm", 
                        to.name, to.z_bottom, to.z_top, to.z_top - to.z_bottom);
                } else {
                    eprintln!("[VIA Z-RANGE DEBUG]   TO layer: NOT FOUND in stackup!");
                }
                
                // v0.3.0: Read contact_depth from profile (default to 30% if not specified)
                let contact_depth_pct = if let Some(prof_ident) = &space_decl.profile {
                    if let Ok(prof_decl) = _symbol_table.get_profile(prof_ident.as_str()) {
                        let mut found_depth = None;
                        for sec in &prof_decl.sections {
                            if sec.section_type == "via" {
                                for (field_name, field_expr) in &sec.fields {
                                    if field_name == "contact_depth" {
                                        // Extract percentage value (e.g., "30%" -> 0.30)
                                        if let hwc_parser::ast::Expression::StringLiteral { value, .. } = field_expr {
                                            if value.ends_with('%') {
                                                if let Ok(pct) = value.trim_end_matches('%').parse::<f64>() {
                                                    eprintln!("[VIA Z-RANGE DEBUG] Profile specifies contact_depth: {}% -> {}", pct, pct / 100.0);
                                                    found_depth = Some(pct / 100.0);
                                                    break;
                                                } else {
                                                    eprintln!("[VIA Z-RANGE DEBUG] Failed to parse contact_depth percentage: '{}'", value);
                                                }
                                            } else {
                                                eprintln!("[VIA Z-RANGE DEBUG] contact_depth is not a percentage: '{}'", value);
                                            }
                                        } else {
                                            eprintln!("[VIA Z-RANGE DEBUG] contact_depth has unexpected expression type");
                                        }
                                    }
                                }
                                if found_depth.is_some() {
                                    break;
                                }
                            }
                        }
                        found_depth
                    } else {
                        None
                    }
                } else {
                    None
                }.unwrap_or(0.30); // Default to 30% if not specified
                
                eprintln!("[VIA Z-RANGE DEBUG]   Using contact_depth: {}% ({} as fraction)", contact_depth_pct * 100.0, contact_depth_pct);
                
                // Calculate penetration depths
                let (z_start_nm, z_end_nm) = if let (Some(from), Some(to)) = (from_st, to_st) {
                    let from_thickness = from.z_top - from.z_bottom;
                    let to_thickness = to.z_top - to.z_bottom;
                    
                    // Via penetrates contact_depth% into FROM layer (from top surface downward)
                    let from_penetration = (from_thickness as f64 * contact_depth_pct) as i64;
                    let via_z_start = from.z_top - from_penetration;
                    
                    // Via penetrates contact_depth% into TO layer (from bottom surface upward)
                    let to_penetration = (to_thickness as f64 * contact_depth_pct) as i64;
                    let via_z_end = to.z_bottom + to_penetration;
                    
                    eprintln!("[VIA Z-RANGE DEBUG]   FROM penetration: {}nm * {} = {}nm", from_thickness, contact_depth_pct, from_penetration);
                    eprintln!("[VIA Z-RANGE DEBUG]   TO penetration: {}nm * {} = {}nm", to_thickness, contact_depth_pct, to_penetration);
                    eprintln!("[VIA Z-RANGE DEBUG]   CALCULATED: z_start={}nm (from.z_top {} - penetration {})", via_z_start, from.z_top, from_penetration);
                    eprintln!("[VIA Z-RANGE DEBUG]   CALCULATED: z_end={}nm (to.z_bottom {} + penetration {})", via_z_end, to.z_bottom, to_penetration);
                    eprintln!("[VIA Z-RANGE DEBUG]   NEW via span: {}nm (vs OLD full-span: {}nm)", via_z_end - via_z_start, to.z_top - from.z_bottom);
                    
                    (via_z_start, via_z_end)
                } else {
                    // Fallback to old behavior if layers not found
                    let z_start = from_st.map(|l| l.z_bottom).unwrap_or(0);
                    let z_end = to_st.map(|l| l.z_top).unwrap_or(1000);
                    eprintln!("[VIA Z-RANGE DEBUG]   FALLBACK: Using old full-span behavior");
                    (z_start, z_end)
                };


                let bbox = Some(hwc_engine::BoundingBox::new(
                    hwc_engine::Point3D::new(x_nm - r_nm, y_nm - r_nm, z_start_nm),
                    hwc_engine::Point3D::new(x_nm + r_nm, y_nm + r_nm, z_end_nm),
                ));

                let mat_name: CompactString = "Tungsten".into();
                let mat_id = hw_space.material_registry.get_id("Tungsten").unwrap_or_else(|| {
                    hw_space.material_registry.register_with_properties(
                        "Tungsten",
                        hwc_parser::MaterialCategory::Conductor,
                        hwc_engine::ManufacturingProcess::Deposited,
                    )
                });

                // v0.3.0 DEBUG: Check profile for via shape specification
                let via_shape: Option<String> = if let Some(prof_ident) = &space_decl.profile {
                    eprintln!("[VIA SHAPE DEBUG] Space has profile: '{}'", prof_ident);
                    if let Ok(prof_decl) = _symbol_table.get_profile(prof_ident.as_str()) {
                        eprintln!("[VIA SHAPE DEBUG] Profile found with {} sections", prof_decl.sections.len());
                        let mut found_shape = None;
                        for sec in &prof_decl.sections {
                            eprintln!("[VIA SHAPE DEBUG] Checking section type: '{}'", sec.section_type);
                            if sec.section_type == "via" {
                                eprintln!("[VIA SHAPE DEBUG] Found 'via' section with {} fields", sec.fields.len());
                                for (field_name, field_expr) in &sec.fields {
                                    eprintln!("[VIA SHAPE DEBUG] Field '{}' found", field_name);
                                    if field_name == "shape" {
                                        if let hwc_parser::ast::Expression::StringLiteral { value, .. } = field_expr {
                                            eprintln!("[VIA SHAPE DEBUG] Profile specifies via shape (StringLiteral): '{}'", value);
                                            found_shape = Some(value.to_string());
                                            break;
                                        } else if let hwc_parser::ast::Expression::Variable { name, .. } = field_expr {
                                            eprintln!("[VIA SHAPE DEBUG] Profile specifies via shape (Variable): '{}'", name);
                                            found_shape = Some(name.to_string());
                                            break;
                                        } else {
                                            eprintln!("[VIA SHAPE DEBUG] Shape field has unexpected expression type");
                                        }
                                    }
                                }
                                if found_shape.is_some() {
                                    break;
                                }
                            }
                        }
                        if found_shape.is_none() {
                            eprintln!("[VIA SHAPE DEBUG] No shape field found in via section, defaulting to None");
                        }
                        found_shape
                    } else {
                        eprintln!("[VIA SHAPE DEBUG] Profile '{}' not found in symbol table", prof_ident);
                        None
                    }
                } else {
                    eprintln!("[VIA SHAPE DEBUG] Space has no profile specified");
                    None
                };

                eprintln!("[VIA DEBUG] Creating contact #{}: from='{}' to='{}', diameter={}nm, position=({}, {}), z_range=[{}, {}], shape={:?}",
                    idx, contact.from_layer, contact.to_layer, dia_nm, x_nm, y_nm, z_start_nm, z_end_nm, via_shape);

                if let Some(b) = bbox {
                    let engine_net = contact.net.map(|id| hwc_engine::netlist::NetId::new(id.0)).unwrap_or(hwc_engine::netlist::NetId::UNCONNECTED);
                    
                    // v0.3.0 FIX: Use square contacts for IC (shape="square"), circular for PCB
                    let substrate_contact = if via_shape.as_deref() == Some("square") {
                        eprintln!("[VIA DEBUG] ✓ Using SQUARE IC-etched contact ({}nm x {}nm) - etched through silicon layers", dia_nm, dia_nm);
                        hwc_engine::geometry_router::substrate_types::SubstrateLayer::new_square_via(
                            mat_id,
                            engine_net,
                            b,
                            dia_nm, // side length for square
                        )
                    } else {
                        eprintln!("[VIA DEBUG] ✓ Using CIRCULAR PCB-drilled via (radius={}nm, area={}nm²) - mechanically drilled", r_nm, (r_nm * r_nm * 314) / 100);
                        hwc_engine::geometry_router::substrate_types::SubstrateLayer::new_contact_circle(
                            mat_id,
                            engine_net,
                            b,
                            r_nm,
                        )
                    };
                    hw_space.entity_graph.substrate_layers.push(substrate_contact);
                }

                let net_name = contact.net.and_then(|id| net_id_to_name.get(&id).cloned());

                // v0.3.0: Generate semantic contact names based on transition and net
                // Format: <Net>_<FromLayer>_<ToLayer> or Via_<FromLayer>_<ToLayer>_N if no net
                let contact_name = if let Some(semantic_name) = &contact.semantic_name {
                    semantic_name.clone()
                } else {
                    // Generate semantic name: NetName_FromLayer_ToLayer or Via_FromLayer_ToLayer_N
                    let counter_key = (contact.from_layer.clone(), contact.to_layer.clone(), net_name.clone());
                    let counter = via_counters.entry(counter_key).or_insert(0);
                    *counter += 1;
                    
                    if let Some(ref net) = net_name {
                        if *counter == 1 {
                            // First via on this transition+net: use simpler name
                            CompactString::new(format!("Via_{}_{}_{}", net, contact.from_layer, contact.to_layer))
                        } else {
                            // Multiple vias: add counter
                            CompactString::new(format!("Via_{}_{}_{}_{}", net, contact.from_layer, contact.to_layer, *counter - 1))
                        }
                    } else {
                        // No net: use transition with counter
                        CompactString::new(format!("Via_{}_{}_{}", contact.from_layer, contact.to_layer, idx))
                    }
                };

                hw_space.contacts.push(hwc_engine::ContactMetadata {
                    name: contact_name,
                    material_name: mat_name,
                    z_start_nm,
                    z_end_nm,
                    net: net_name,
                    bridge: None,
                    bbox,
                    drill_diameter_nm: Some(dia_nm),
                    is_tented: false,
                    mask_clearance_diameter_nm: None,
                    from_layer: Some(contact.from_layer.clone()),
                    to_layer: Some(contact.to_layer.clone()),
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
