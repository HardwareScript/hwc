use crate::symbol_table::SymbolTable;
use hwc_materials::{
    ClearanceConstraints, ConstraintSet, LayerConstraints, RoutableMode, TraceConstraints,
    ViaConstraints,
};
use hwc_parser::ProfileDefinition;

use super::error::ConversionError;
use super::unit_conversion::{measurement_to_celsius, measurement_to_nm, measurement_to_volts};

/// Convert ProfileDefinition from Symbol Table to ConstraintSet
///
/// This implements Phase 6.4: Profile to Constraints Conversion
/// Reference: ROUTING-AND-PHYSICS.md - Translation 1 & 2
pub fn profile_to_constraints(
    profile: &ProfileDefinition,
    _symbol_table: &SymbolTable,
) -> Result<ConstraintSet, ConversionError> {
    let trace = if let Some(trace_def) = &profile.trace {
        TraceConstraints {
            min_width_nm: measurement_to_nm(&trace_def.min_width),
            max_width_nm: 0,
            min_spacing_nm: measurement_to_nm(&trace_def.min_spacing),
            default_width_nm: measurement_to_nm(&trace_def.min_width),
        }
    } else {
        return Err(ConversionError::MissingProfileConstraint(
            "trace (min_width, min_spacing)".into(),
        ));
    };

    let via = if let Some(via_def) = &profile.via {
        ViaConstraints {
            min_diameter_nm: measurement_to_nm(&via_def.min_diameter),
            max_diameter_nm: 0,
            min_annular_ring_nm: measurement_to_nm(&via_def.min_annular_ring),
            min_spacing_nm: via_def
                .min_spacing
                .as_ref()
                .map(measurement_to_nm)
                .unwrap_or_else(|| measurement_to_nm(&via_def.min_diameter) * 2),
            default_diameter_nm: via_def
                .default_diameter
                .as_ref()
                .map(measurement_to_nm)
                .unwrap_or_else(|| measurement_to_nm(&via_def.min_diameter)),
            shape: via_def.shape.as_ref().map(|s| s.name.clone()),
        }
    } else {
        return Err(ConversionError::MissingProfileConstraint(
            "via (min_diameter, min_annular_ring)".into(),
        ));
    };

    let copper_thickness_nm = profile
        .manufacturing
        .as_ref()
        .and_then(|m| m.copper_thickness.as_ref())
        .map(measurement_to_nm)
        .ok_or_else(|| {
            ConversionError::MissingProfileConstraint("manufacturing.copper_thickness".into())
        })?;

    let _ipc2221_k_external = profile
        .manufacturing
        .as_ref()
        .and_then(|m| m.ipc2221_k_external)
        .unwrap_or(0.048);

    let _ipc2221_k_internal = profile
        .manufacturing
        .as_ref()
        .and_then(|m| m.ipc2221_k_internal)
        .unwrap_or(0.024);

    let solder_mask_expansion_nm = profile
        .manufacturing
        .as_ref()
        .and_then(|m| m.solder_mask_expansion.as_ref())
        .map(|m| measurement_to_nm(m));

    let _low_voltage_threshold_v = profile
        .clearance
        .as_ref()
        .and_then(|c| c.low_voltage_threshold.as_ref())
        .map(measurement_to_volts)
        .unwrap_or(50.0);

    let _medium_voltage_threshold_v = profile
        .clearance
        .as_ref()
        .and_then(|c| c.medium_voltage_threshold.as_ref())
        .map(measurement_to_volts)
        .unwrap_or(150.0);

    let clearance = if let Some(clearance_def) = &profile.clearance {
        // v0.1.8: Use trace.min_spacing as the base clearance for all voltage tiers.
        // The profile's trace.min_spacing is the standard net-to-net spacing.
        // High-voltage clearance is declared separately in the clearance block
        // and is used only for HV net pairs.
        ClearanceConstraints {
            low_voltage_nm: trace.min_spacing_nm,
            medium_voltage_nm: trace.min_spacing_nm,
            high_voltage_nm: clearance_def
                .high_voltage
                .as_ref()
                .map(measurement_to_nm)
                .ok_or_else(|| {
                    ConversionError::MissingProfileConstraint("clearance.high_voltage".into())
                })?,
            safety_factor: clearance_def.safety_factor.ok_or_else(|| {
                ConversionError::MissingProfileConstraint("clearance.safety_factor".into())
            })?,
        }
    } else {
        return Err(ConversionError::MissingProfileConstraint(
            "clearance (high_voltage, safety_factor)".into(),
        ));
    };

    let layer = if let Some(layer_def) = &profile.layer {
        LayerConstraints {
            min_thickness_nm: layer_def
                .min_thickness
                .as_ref()
                .map(measurement_to_nm)
                .unwrap_or(copper_thickness_nm),
            max_thickness_nm: 0,
            allowed_conductors: layer_def.allowed_conductors.clone(),
            allowed_dielectrics: layer_def.allowed_dielectrics.clone(),
        }
    } else {
        return Err(ConversionError::MissingProfileConstraint(
            "layer (allowed_conductors, allowed_dielectrics)".into(),
        ));
    };

    let thermal = profile
        .thermal
        .as_ref()
        .map(|thermal_def| hwc_materials::ThermalConstraints {
            ambient_temp_c: measurement_to_celsius(&thermal_def.ambient_temp),
            max_operating_temp_c: measurement_to_celsius(&thermal_def.max_operating_temp),
            max_temp_rise_c: measurement_to_celsius(&thermal_def.max_temp_rise),
            clustering_threshold_nm: thermal_def
                .clustering_threshold
                .as_ref()
                .map(measurement_to_nm),
        });

    let stackup: Option<hwc_materials::StackupConstraints> = None;

    let bridges = profile
        .bridges
        .iter()
        .map(|b| hwc_materials::BridgeRule {
            from_material: b.from.clone(),
            to_material: b.to.clone(),
            interface_material: b.interface_material.clone(),
            fill_material: b
                .fill_material
                .clone()
                .unwrap_or_else(|| b.interface_material.clone()),
        })
        .collect();

    // v0.1.8: Propagate per-layer routability from the stackup to the constraint set.
    // Table-driven: each stackup layer's `routable` field becomes a lookup entry.
    let mut layer_routability = rustc_hash::FxHashMap::default();
    if let Some(stackup) = &profile.stackup {
        for layer in &stackup.layers {
            if let Some(mode) = layer.routable {
                let mode_cm = match mode {
                    hwc_parser::RoutableMode::True => RoutableMode::True,
                    hwc_parser::RoutableMode::False => RoutableMode::False,
                    hwc_parser::RoutableMode::LocalOnly => RoutableMode::LocalOnly,
                };
                layer_routability.insert(layer.name.name.clone().into(), mode_cm);
            }
        }
    }

    // v0.1.8: Propagate max_local_route_length from routing constraints.
    let max_local_route_length_nm = profile
        .routing
        .as_ref()
        .and_then(|r| r.max_local_route_length.as_ref())
        .map(measurement_to_nm);

    Ok(ConstraintSet {
        name: profile.name.to_string().into(),
        description: profile.description.clone().unwrap_or_default(),
        trace,
        via,
        clearance,
        layer,
        thermal,
        stackup,
        bridges,
        solder_mask_expansion_nm,
        technology: profile.technology.clone(),
        layer_routability,
        max_local_route_length_nm,
    })
}
