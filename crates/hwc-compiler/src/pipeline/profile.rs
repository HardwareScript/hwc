//! `profile` declaration → [`hwc_materials::ConstraintSet`] bridge.
//!
//! Walks the `technology`/`via`/`trace`/`manufacturing`/`clearance`/`thermal`
//! sections of a resolved `profile` and lowers them into a fabrication
//! constraint set consumed by the routing/verification stages.

use compact_str::CompactString;

use crate::pipeline::extract_numeric_value;
use crate::symbol_table::SymbolTable;
use hwc_materials::{ClearanceConstraints, ConstraintSet, LayerConstraints, ThermalConstraints, TraceConstraints, ViaConstraints};
use hwc_parser::ast::Expression;
use hwc_parser::SpaceDecl;

/// Parse a `profile` referenced by `space_decl` into a [`ConstraintSet`].
///
/// Returns `None` when the space has no profile or the named profile cannot be
/// resolved from the symbol table.
pub fn build_fabrication_constraints(
    space_decl: &SpaceDecl,
    symbol_table: &SymbolTable,
) -> Option<ConstraintSet> {
    let prof_ident = space_decl.profile.as_ref()?;
    let prof_decl = symbol_table.get_profile(prof_ident.as_str()).ok()?;

    let mut via_shape: Option<CompactString> = None;
    let mut min_via_dia_nm = 170i64;
    let mut min_via_encl_nm = 40i64;
    let mut min_via_spc_nm = 200i64;
    let via_contact_depth_nm = 0i64;
    let mut min_trace_w_nm = 300i64;
    let mut min_trace_spc_nm = 300i64;
    let mut circle_segments = 64u32;
    let mut mfg_grid_nm = 10i64;
    let mut substrate_net_name: Option<CompactString> = None;
    let mut thermal_constraints: Option<ThermalConstraints> = None;
    let mut clearance_high_v_nm = 1000i64;
    let mut clearance_safety_factor = 2.0f64;

    for sec in &prof_decl.sections {
        match sec.section_type.as_str() {
            "technology" => {
                for (field_name, field_expr) in &sec.fields {
                    if field_name == "substrate_net" {
                        if let Expression::StringLiteral { value, .. } = field_expr {
                            substrate_net_name = Some(value.as_str().into());
                        } else if let Expression::Variable { name, .. } = field_expr {
                            substrate_net_name = Some(name.as_str().into());
                        }
                    }
                }
            }
            "via" => {
                for (field_name, field_expr) in &sec.fields {
                    match field_name.as_str() {
                        "shape" => {
                            if let Expression::StringLiteral { value, .. } = field_expr {
                                via_shape = Some(value.as_str().into());
                            } else if let Expression::Variable { name, .. } = field_expr {
                                via_shape = Some(name.as_str().into());
                            }
                        }
                        "min_diameter" => {
                            if let Expression::Measurement { value, unit, .. } = field_expr {
                                if let Ok(nm) = unit.to_nanometers(*value) {
                                    min_via_dia_nm = nm as i64;
                                }
                            }
                        }
                        "min_enclosure" => {
                            if let Expression::Measurement { value, unit, .. } = field_expr {
                                if let Ok(nm) = unit.to_nanometers(*value) {
                                    min_via_encl_nm = nm as i64;
                                }
                            }
                        }
                        "min_spacing" => {
                            if let Expression::Measurement { value, unit, .. } = field_expr {
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
                            if let Expression::Measurement { value, unit, .. } = field_expr {
                                if let Ok(nm) = unit.to_nanometers(*value) {
                                    min_trace_w_nm = nm as i64;
                                }
                            }
                        }
                        "min_spacing" => {
                            if let Expression::Measurement { value, unit, .. } = field_expr {
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
                            if let Expression::Literal { value, .. } = field_expr {
                                circle_segments = *value as u32;
                            }
                        }
                        "track_pitch" => {
                            if let Expression::Measurement { value, unit, .. } = field_expr {
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
                            if let Expression::Measurement { value, unit, .. } = field_expr {
                                if let Ok(nm) = unit.to_nanometers(*value) {
                                    clearance_high_v_nm = nm as i64;
                                }
                            }
                        }
                        "safety_factor" => {
                            if let Expression::Literal { value, .. } = field_expr {
                                clearance_safety_factor = *value as f64;
                            } else if let Expression::FloatLiteral { value, .. } = field_expr {
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
                            if let Expression::Measurement { value, unit, .. } = field_expr {
                                if let Ok(nm) = unit.to_nanometers(*value) {
                                    clustering_thresh_nm = Some(nm as i64);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                thermal_constraints = Some(ThermalConstraints {
                    ambient_temp_c: ambient,
                    max_operating_temp_c: max_op,
                    max_temp_rise_c: max_rise,
                    clustering_threshold_nm: clustering_thresh_nm,
                });
            }
            _ => {}
        }
    }

    let fab_constraints = ConstraintSet {
        name: prof_ident.name.clone(),
        description: "".into(),
        trace: TraceConstraints {
            min_width_nm: min_trace_w_nm,
            max_width_nm: 0,
            min_spacing_nm: min_trace_spc_nm,
            default_width_nm: min_trace_w_nm,
        },
        via: ViaConstraints {
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
        clearance: ClearanceConstraints {
            low_voltage_nm: 300,
            medium_voltage_nm: 600,
            high_voltage_nm: clearance_high_v_nm,
            safety_factor: clearance_safety_factor,
            max_substrate_tap_distance_nm: None,
        },
        layer: LayerConstraints {
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
    Some(fab_constraints)
}
