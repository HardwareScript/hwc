//! EM/Thermal verification and design rule checking.
//!
//! Phase 3 of the routing pipeline: validates physics compliance
//! and checks clearance violations against component boundaries.

use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::netlist::NetId;
use hwc_engine::{HardwareSpace, LineSegment};

/// Parameters for EM (Electromigration) verification.
pub struct EmVerificationParams<'a> {
    pub space: &'a HardwareSpace,
    pub net_id: NetId,
    pub net_name: &'a CompactString,
    pub segments: &'a [LineSegment],
    pub trace_width_nm: i64,
    pub trace_thickness_nm: i64,
    pub current_ma: f64,
    pub profile: Option<&'a hwc_parser::ProfileDefinition>,
}

/// Verify EM (Electromigration) and thermal constraints.
///
/// Checks current density limits and temperature rise against PDK constraints.
pub fn verify_em_thermal(params: &EmVerificationParams) -> Result<(), IrError> {
    let current_decl = if let Some(route) = params
        .space
        .netlist
        .get_net(params.net_id)
        .and_then(|n| n.current_ma)
    {
        hwc_engine::CurrentDeclaration::Dc(route / 1000.0)
    } else {
        hwc_engine::CurrentDeclaration::Dc(params.current_ma / 1000.0)
    };

    let em_segments: Vec<hwc_engine::IndexedSegment> = params
        .segments
        .iter()
        .enumerate()
        .map(|(i, seg)| hwc_engine::IndexedSegment {
            source: hwc_engine::geometry_router::spatial_index::SpatialEntitySource::RouteSegment {
                net_idx: params.net_id.raw() as usize,
                seg_idx: i,
            },
            segment_id: i,
            net_id: params.net_id.raw() as usize,
            width_nm: params.trace_width_nm,
            thickness_nm: params.trace_thickness_nm,
            start: seg.start,
            end: seg.end,
            layer: 0,
        })
        .collect();

    let em_params = hwc_engine::EmParams {
        j_limit: params
            .profile
            .and_then(|p| p.other.get("em_current_density_limit"))
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message:
                    "PDK missing required 'em_current_density_limit' in profile 'other' block."
                        .into(),
                hint: "Add 'other: em_current_density_limit: <value>' to your profile.".into(),
            })?,
        i_peak: current_decl.peak(),
    };

    let thermal_params = hwc_engine::ThermalParams {
        ambient_temp_c: params
            .profile
            .and_then(|p| p.thermal.as_ref())
            .map(|t| t.ambient_temp.value)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "PDK missing required 'thermal.ambient_temp' constraint.".into(),
                hint: "Add a 'thermal:' block to your profile with 'ambient_temp: <value>'.".into(),
            })?,
        max_temp_rise_c: params
            .profile
            .and_then(|p| p.thermal.as_ref())
            .map(|t| t.max_temp_rise.value)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "PDK missing required 'thermal.max_temp_rise' constraint.".into(),
                hint: "Add a 'thermal:' block to your profile with 'max_temp_rise: <value>'."
                    .into(),
            })?,
        copper_thickness_m: params.trace_thickness_nm as f64 * 1e-9,
        substrate_er: params
            .profile
            .and_then(|p| p.stackup.as_ref())
            .and_then(|s| {
                s.layers.iter().find(|l| {
                    l.material.to_lowercase().contains("fr4")
                        || l.material.to_lowercase().contains("dielectric")
                })
            })
            .and_then(|l| {
                let er_key: CompactString = format!("substrate_er_{}", l.name.name).into();
                params
                    .profile
                    .and_then(|p| p.other.get(&er_key))
                    .and_then(|s| s.parse::<f64>().ok())
            })
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "PDK missing required substrate dielectric constant (substrate_er)."
                    .into(),
                hint: "Add 'other: substrate_er_<LayerName>: <value>' to your profile.".into(),
            })?,
    };

    let violations =
        hwc_engine::verify_em_thermal(&em_segments, &current_decl, &em_params, &thermal_params);

    if !violations.is_empty() {
        let msg = violations
            .iter()
            .map(|v| match v {
                hwc_engine::EmThermalViolation::Em(em) => {
                    format!(
                        "EM violation: current density {:.2} A/m² exceeds limit {:.2} A/m² at ({}, {}), width {}nm, min {}nm",
                        em.current_density, em.limit, em.location.0, em.location.1, em.width_nm, em.min_width_nm
                    )
                }
                hwc_engine::EmThermalViolation::Thermal(th) => {
                    format!(
                        "Thermal violation: {:.1}°C rise exceeds {:.1}°C limit at ({}, {})",
                        th.temp_rise_c, th.max_allowed_c, th.location.0, th.location.1
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n  ");
        eprintln!(
            "[ROUTER] ⚠ EM/Thermal violations for route {}:\n  {}",
            params.net_name, msg
        );
    }

    Ok(())
}

/// Design Rule Check parameters.
pub struct DrcParams<'a> {
    pub space: &'a HardwareSpace,
    pub net_name: CompactString,
    pub from_component: CompactString,
    pub to_component: CompactString,
    pub min_clearance_nm: i64,
    pub route_from: &'a hwc_parser::RouteEndpointSpec,
    pub route_to: &'a hwc_parser::RouteEndpointSpec,
}

/// Run design rule checks on the routed trace.
///
/// Validates clearance violations against component boundaries,
/// excluding the source and destination components.
pub fn run_drc(params: &DrcParams) -> Result<(), IrError> {
    let current_route = params
        .space
        .analytic_routes
        .last()
        .ok_or_else(|| IrError::EmptyRoute {
            net: params.net_name.clone(),
        })?;

    let mut violations = Vec::new();
    for (comp_name, comp_bbox) in &params.space.component_bboxes {
        if comp_name == params.from_component || comp_name == params.to_component {
            continue;
        }

        if !current_route.check_clearance(comp_bbox, params.min_clearance_nm) {
            let half_w = current_route.cross_section.width_nm / 2;
            let mut min_dist = i64::MAX;

            for seg in &current_route.segments {
                let dist = seg.distance_to_bbox(comp_bbox);
                min_dist = min_dist.min(dist);
            }

            let actual_clearance = min_dist - half_w;
            violations.push((
                current_route.net_name.clone(),
                comp_name.clone(),
                actual_clearance,
            ));
        }
    }

    if !violations.is_empty() {
        return Err(IrError::NoPathFound {
            net: params.net_name.clone(),
            from_pin: crate::ir::routing::helpers::endpoint_label(params.route_from).into(),
            to_pin: crate::ir::routing::helpers::endpoint_label(params.route_to).into(),
        });
    }

    Ok(())
}
