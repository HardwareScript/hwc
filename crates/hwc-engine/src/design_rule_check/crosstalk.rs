//! Crosstalk and Signal Integrity Validation Engine (v0.2.1)
//!
//! Evaluates capacitive coupling between parallel traces using:
//! 1. Segment-by-segment integer coordinate overlap projection
//! 2. Stackup-driven dielectric permittivity & ground-plane distance extraction
//! 3. Wheeler effective permittivity + Sakurai 2.5D empirical microstrip equations
//! 4. Intent-driven budget enforcement (Zero string-matching heuristics)
//!
//! **HardwareScript Architectural Compliance:**
//! - Law of Zero Magic: No string matching, all intent from PDK declarations
//! - Law of Stackup Truth: Dielectric properties queried from stackup
//! - Law of Physical Reality: Geometry computed dynamically from layer Z-coordinates
//! - Subsystem 21 (BEM Standard): Wheeler + Sakurai 2.5D fringing physics

use super::types::DrcViolation;
use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::Point3D;
use crate::space::{AnalyticTrace, HardwareSpace, LineSegment};

use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Vacuum permittivity constant: ε₀ = 8.8541878128e-12 F/m
const EPSILON_0: f64 = 8.854_187_812_8e-12;

/// Geometric & material context extracted from the layer stackup for a routing plane.
#[derive(Debug, Clone, Copy)]
pub struct DielectricContext {
    /// Relative permittivity of the surrounding dielectric (from stackup ILD)
    pub epsilon_r: f64,
    /// Distance from the trace centerline to the underlying reference/ground plane (in meters)
    pub height_to_ground_m: f64,
}

/// A resolved parallel coupling interaction between two discrete trace segments.
#[derive(Debug, Clone)]
pub struct SegmentCoupling {
    pub parallel_length_nm: i64,
    pub edge_to_edge_spacing_nm: i64,
    pub center_point: Point3D,
    pub coupling_ratio_db: f64,
}

/// Primary entrypoint: validates all analytic routes against their declared PDK budgets.
pub fn validate_crosstalk(
    space: &HardwareSpace,
    _constraints: &ConstraintRulebook,
) -> Result<Vec<DrcViolation>, String> {
    let mut violations = Vec::new();
    let traces = &space.analytic_routes;

    if traces.len() < 2 {
        return Ok(violations);
    }

    // 1. Build Net-to-Budget map from declared net intents (No hardcoded strings!)
    let intent_budgets = extract_net_crosstalk_budgets(space);

    // 2. Pairwise segment evaluation across all routes
    for i in 0..traces.len() {
        for j in (i + 1)..traces.len() {
            let trace_a = &traces[i];
            let trace_b = &traces[j];

            // Same net cannot crosstalk with itself
            if trace_a.net_id == trace_b.net_id {
                continue;
            }

            // Only evaluate traces on the same physical routing layer
            if trace_a.layer_name != trace_b.layer_name {
                continue;
            }

            // 3. Resolve stackup physics (dielectric properties & ground reference distance)
            let dielectric = resolve_dielectric_context(space, &trace_a.layer_name)?;

            // 4. Evaluate segment-by-segment coupling
            for seg_a in &trace_a.segments {
                for seg_b in &trace_b.segments {
                    if let Some(coupling) = evaluate_segment_pair(
                        trace_a,
                        seg_a,
                        trace_b,
                        seg_b,
                        &dielectric,
                    ) {
                        // Check victim budget (Trace A)
                        if let Some(&budget_a) = intent_budgets.get(&trace_a.net_name) {
                            if coupling.coupling_ratio_db > budget_a {
                                violations.push(DrcViolation::CrosstalkViolation {
                                    aggressor_net: trace_b.net_name.clone(),
                                    victim_net: trace_a.net_name.clone(),
                                    crosstalk_db: coupling.coupling_ratio_db,
                                    max_crosstalk_db: budget_a,
                                    parallel_length_nm: coupling.parallel_length_nm,
                                    spacing_nm: coupling.edge_to_edge_spacing_nm,
                                    location: coupling.center_point,
                                });
                            }
                        }

                        // Check victim budget (Trace B)
                        if let Some(&budget_b) = intent_budgets.get(&trace_b.net_name) {
                            if coupling.coupling_ratio_db > budget_b {
                                violations.push(DrcViolation::CrosstalkViolation {
                                    aggressor_net: trace_a.net_name.clone(),
                                    victim_net: trace_b.net_name.clone(),
                                    crosstalk_db: coupling.coupling_ratio_db,
                                    max_crosstalk_db: budget_b,
                                    parallel_length_nm: coupling.parallel_length_nm,
                                    spacing_nm: coupling.edge_to_edge_spacing_nm,
                                    location: coupling.center_point,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(violations)
}

/// Evaluates coupling between two discrete line segments using exact coordinate projections.
fn evaluate_segment_pair(
    trace_a: &AnalyticTrace,
    seg_a: &LineSegment,
    trace_b: &AnalyticTrace,
    seg_b: &LineSegment,
    dielectric: &DielectricContext,
) -> Option<SegmentCoupling> {
    let dx_a = (seg_a.end.x - seg_a.start.x).abs();
    let dy_a = (seg_a.end.y - seg_a.start.y).abs();
    let dx_b = (seg_b.end.x - seg_b.start.x).abs();
    let dy_b = (seg_b.end.y - seg_b.start.y).abs();

    let a_is_horizontal = dy_a == 0 && dx_a > 0;
    let b_is_horizontal = dy_b == 0 && dx_b > 0;
    let a_is_vertical = dx_a == 0 && dy_a > 0;
    let b_is_vertical = dx_b == 0 && dy_b > 0;

    let (parallel_len_nm, centerline_spacing_nm, center_pt) = if a_is_horizontal && b_is_horizontal
    {
        // Both horizontal: check X interval overlap
        let a_min_x = seg_a.start.x.min(seg_a.end.x);
        let a_max_x = seg_a.start.x.max(seg_a.end.x);
        let b_min_x = seg_b.start.x.min(seg_b.end.x);
        let b_max_x = seg_b.start.x.max(seg_b.end.x);

        let overlap_start = a_min_x.max(b_min_x);
        let overlap_end = a_max_x.min(b_max_x);

        if overlap_end <= overlap_start {
            return None; // No parallel overlap
        }

        let len = overlap_end - overlap_start;
        let spacing = (seg_a.start.y - seg_b.start.y).abs();
        let mid_x = (overlap_start + overlap_end) / 2;
        let mid_y = (seg_a.start.y + seg_b.start.y) / 2;

        (len, spacing, Point3D::new(mid_x, mid_y, seg_a.start.z))
    } else if a_is_vertical && b_is_vertical {
        // Both vertical: check Y interval overlap
        let a_min_y = seg_a.start.y.min(seg_a.end.y);
        let a_max_y = seg_a.start.y.max(seg_a.end.y);
        let b_min_y = seg_b.start.y.min(seg_b.end.y);
        let b_max_y = seg_b.start.y.max(seg_b.end.y);

        let overlap_start = a_min_y.max(b_min_y);
        let overlap_end = a_max_y.min(b_max_y);

        if overlap_end <= overlap_start {
            return None; // No parallel overlap
        }

        let len = overlap_end - overlap_start;
        let spacing = (seg_a.start.x - seg_b.start.x).abs();
        let mid_x = (seg_a.start.x + seg_b.start.x) / 2;
        let mid_y = (overlap_start + overlap_end) / 2;

        (len, spacing, Point3D::new(mid_x, mid_y, seg_a.start.z))
    } else {
        // Orthogonal segments do not exhibit parallel lateral crosstalk
        return None;
    };

    // Calculate edge-to-edge physical clearance D = Centerline_Spacing - (W_a/2 + W_b/2)
    let w_a_nm = trace_a.cross_section.width_nm;
    let w_b_nm = trace_b.cross_section.width_nm;
    let edge_spacing_nm = centerline_spacing_nm - (w_a_nm / 2 + w_b_nm / 2);

    if edge_spacing_nm <= 0 {
        return None; // Touching or shorted (handled by clearance DRC, not crosstalk)
    }

    // Physics Engine: Wheeler + Sakurai 2.5D Microstrip Equations
    let t_m = trace_a.cross_section.thickness_nm as f64 * 1e-9;
    let w_m = w_a_nm.max(w_b_nm) as f64 * 1e-9;
    let d_m = edge_spacing_nm as f64 * 1e-9;
    let l_m = parallel_len_nm as f64 * 1e-9;
    let h_m = dielectric.height_to_ground_m;

    // Wheeler Effective Relative Permittivity
    let eps_eff = compute_wheeler_effective_permittivity(dielectric.epsilon_r, w_m, h_m);

    // Sakurai 2.5D Empirical Coupling Capacitance (C₁₂)
    let term_w = 0.03 * (w_m / h_m);
    let term_t = 0.08 * (t_m / h_m);
    let term_fringe = 0.07 * (w_m / h_m).powf(0.25) * (t_m / h_m).powf(0.5) * (h_m / d_m).powf(1.34);
    let c_12 = EPSILON_0 * eps_eff * l_m * (term_w + term_t + term_fringe);

    // Sakurai Ground Capacitance (C₁g)
    let c_gnd = EPSILON_0 * eps_eff * l_m * (1.15 * (w_m / h_m) + 2.80 * (t_m / h_m).powf(0.222));

    // Voltage Transfer Ratio (Capacitive Divider): V_victim / V_aggressor = C12 / (C12 + Cgnd)
    let ratio = c_12 / (c_12 + c_gnd);
    let coupling_db = if ratio > 1e-12 {
        20.0 * ratio.log10()
    } else {
        -240.0
    };

    Some(SegmentCoupling {
        parallel_length_nm: parallel_len_nm,
        edge_to_edge_spacing_nm: edge_spacing_nm,
        center_point: center_pt,
        coupling_ratio_db: coupling_db,
    })
}

/// Wheeler's closed-form equation for effective relative permittivity.
#[inline]
fn compute_wheeler_effective_permittivity(eps_r: f64, w: f64, h: f64) -> f64 {
    let term = (1.0 + 12.0 * (h / w)).powf(-0.5);
    ((eps_r + 1.0) / 2.0) + ((eps_r - 1.0) / 2.0) * term
}

/// Dynamically resolves dielectric permittivity and height to ground plane from the StackupManager.
fn resolve_dielectric_context(
    space: &HardwareSpace,
    layer_name: &str,
) -> Result<DielectricContext, String> {
    // 1. Query the layer's physical Z-centerline
    let routing_z_nm = space
        .get_layer_routing_z(layer_name)
        .ok_or_else(|| format!("Layer '{}' has no registered routing elevation", layer_name))?;

    // 2. Query the active substrate dielectric material
    let (epsilon_r, substrate_z_nm) = space
        .get_stackup_dielectric_context(layer_name)
        .unwrap_or((3.9, 0)); // Defaults to standard SiO₂ at z=0 if unshielded

    let height_nm = (routing_z_nm - substrate_z_nm).max(100); // Minimum 100nm to avoid div-by-zero
    let height_m = height_nm as f64 * 1e-9;

    Ok(DielectricContext {
        epsilon_r,
        height_to_ground_m: height_m,
    })
}

/// Extracts declared crosstalk budgets from the space's routing intents (Zero String Magic).
fn extract_net_crosstalk_budgets(space: &HardwareSpace) -> FxHashMap<CompactString, f64> {
    let mut budgets = FxHashMap::default();

    for route in &space.analytic_routes {
        // Query the profile for the net's assigned routing intent
        if let Some(budget_db) = space.get_net_max_crosstalk_db(&route.net_name) {
            budgets.insert(route.net_name.clone(), budget_db);
        }
    }

    budgets
}
