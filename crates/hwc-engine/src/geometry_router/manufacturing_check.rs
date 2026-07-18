use std::fmt;

/// Via definition for manufacturing checks.
#[derive(Clone, Debug)]
pub struct ViaDefinition {
    pub drill_diameter_nm: i64,
    pub pad_diameter_nm: i64,
    pub net_id: usize,
    pub location: (i64, i64),
    /// Layer index this via connects from (lower layer).
    pub layer_from: i64,
    /// Layer index this via connects to (upper layer).
    pub layer_to: i64,
}

/// Technology node for manufacturing constraint selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechNode {
    /// IPC Class 3 PCB: stacked microvia limits, aspect ratio ≤ 10:1.
    PcbIpcClass3,
    /// ASIC layer-local: via must stay on a single metal layer pair.
    AsicLayerLocal,
}

impl TechNode {
    /// Map a profile `technology:` string to a `TechNode`.
    ///
    /// "pcb" or "pcb_ipc_class3" → `PcbIpcClass3`
    /// "asic" or "asic_layer_local" → `AsicLayerLocal`
    /// Anything else defaults to `PcbIpcClass3`.
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "asic" | "asic_layer_local" => TechNode::AsicLayerLocal,
            _ => TechNode::PcbIpcClass3,
        }
    }
}

/// Manufacturing violation types.
#[derive(Clone, Debug)]
pub enum MfgViolationType {
    /// Aspect ratio = board_thickness / drill_diameter exceeds limit.
    ViaAspectRatio { ratio: f64, limit: f64 },
    /// Stacked microvia count per column exceeds lamination cycle limit.
    LaminationCycleExceed { count: u32, limit: u32 },
    /// Solder mask expansion out of technology-specific range.
    SolderMaskExpansion {
        actual: i64,
        min_allowed: i64,
        max_allowed: i64,
    },
    /// Via spans more than a single metal layer pair (ASIC layer-local).
    ViaLayerViolation { message: String },
}

impl fmt::Display for MfgViolationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MfgViolationType::ViaAspectRatio { ratio, limit } => {
                write!(
                    f,
                    "Via aspect ratio {:.2}:1 exceeds limit {}:1",
                    ratio, limit
                )
            }
            MfgViolationType::LaminationCycleExceed { count, limit } => {
                write!(
                    f,
                    "Lamination cycle count {} exceeds limit {}",
                    count, limit
                )
            }
            MfgViolationType::SolderMaskExpansion {
                actual,
                min_allowed,
                max_allowed,
            } => {
                write!(
                    f,
                    "Solder mask expansion {}nm outside allowed range [{}nm, {}nm]",
                    actual, min_allowed, max_allowed
                )
            }
            MfgViolationType::ViaLayerViolation { message } => {
                write!(f, "Via layer violation: {}", message)
            }
        }
    }
}

/// A manufacturing violation with location and description.
#[derive(Clone, Debug)]
pub struct ManufacturingViolation {
    pub violation_type: MfgViolationType,
    pub location: (i64, i64),
    pub message: String,
}

/// PCB Class 3 aspect ratio limit.
const IPC_CLASS3_ASPECT_RATIO_LIMIT: f64 = 10.0;

/// Check via aspect ratio: board_thickness / drill_diameter.
/// Returns violation if ratio exceeds limit.
#[inline]
pub fn check_via_aspect_ratio(
    via: &ViaDefinition,
    board_thickness_m: f64,
) -> Option<ManufacturingViolation> {
    if via.drill_diameter_nm <= 0 {
        return None;
    }
    let drill_m = via.drill_diameter_nm as f64 / 1_000_000_000.0;
    if drill_m <= 0.0 {
        return None;
    }
    let ratio = board_thickness_m / drill_m;

    if ratio > IPC_CLASS3_ASPECT_RATIO_LIMIT {
        Some(ManufacturingViolation {
            violation_type: MfgViolationType::ViaAspectRatio {
                ratio,
                limit: IPC_CLASS3_ASPECT_RATIO_LIMIT,
            },
            location: via.location,
            message: format!(
                "Via aspect ratio {:.2}:1 exceeds IPC Class 3 limit {:.0}:1",
                ratio, IPC_CLASS3_ASPECT_RATIO_LIMIT
            ),
        })
    } else {
        None
    }
}

/// Check lamination cycle limits: count stacked microvias per column.
/// vias are grouped by (x, y) location; stacked microvias are vias at the
/// same (x, y) with overlapping layer ranges.
#[inline]
pub fn check_lamination_cycles(
    vias: &[ViaDefinition],
    max_cycles: u32,
) -> Option<ManufacturingViolation> {
    // Count vias at each unique (x, y) location
    let mut location_counts = std::collections::HashMap::<(i64, i64), u32>::new();

    for via in vias {
        *location_counts.entry(via.location).or_insert(0) += 1;
    }

    for (&loc, &count) in &location_counts {
        if count > max_cycles {
            return Some(ManufacturingViolation {
                violation_type: MfgViolationType::LaminationCycleExceed {
                    count,
                    limit: max_cycles,
                },
                location: loc,
                message: format!(
                    "Stacked microvia count {} at ({}, {}) exceeds lamination limit {}",
                    count, loc.0, loc.1, max_cycles
                ),
            });
        }
    }

    None
}

/// Check solder mask expansion rules.
/// Validates that mask_expansion_nm is within [min_expansion_nm, max_expansion_nm].
#[inline]
pub fn check_solder_mask_expansion(
    pad_diameter_nm: i64,
    mask_expansion_nm: i64,
    min_expansion_nm: i64,
    max_expansion_nm: i64,
) -> Option<ManufacturingViolation> {
    if pad_diameter_nm <= 0 {
        return None;
    }

    if mask_expansion_nm < min_expansion_nm || mask_expansion_nm > max_expansion_nm {
        Some(ManufacturingViolation {
            violation_type: MfgViolationType::SolderMaskExpansion {
                actual: mask_expansion_nm,
                min_allowed: min_expansion_nm,
                max_allowed: max_expansion_nm,
            },
            location: (0, 0),
            message: format!(
                "Solder mask expansion {}nm outside range [{}nm, {}nm]",
                mask_expansion_nm, min_expansion_nm, max_expansion_nm
            ),
        })
    } else {
        None
    }
}

/// Check technology-specific via constraints.
#[inline]
pub fn check_via_constraints(
    via: &ViaDefinition,
    tech: TechNode,
    board_thickness_m: f64,
) -> Vec<ManufacturingViolation> {
    let mut violations = Vec::new();

    match tech {
        TechNode::PcbIpcClass3 => {
            // PCB IPC Class 3: aspect ratio ≤ 10:1
            if let Some(v) = check_via_aspect_ratio(via, board_thickness_m) {
                violations.push(v);
            }
        }
        TechNode::AsicLayerLocal => {
            // ASIC: via must stay on a single metal layer pair (span of 1)
            let span = (via.layer_to - via.layer_from).abs();
            if span > 1 {
                violations.push(ManufacturingViolation {
                    violation_type: MfgViolationType::ViaLayerViolation {
                        message: format!(
                            "Via spans {} layers (from {} to {}); ASIC requires single layer pair",
                            span, via.layer_from, via.layer_to
                        ),
                    },
                    location: via.location,
                    message: format!(
                        "Via spans {} layers; ASIC layer-local constraint violated",
                        span
                    ),
                });
            }
        }
    }

    violations
}

/// Batch verify manufacturing constraints across all vias.
pub fn verify_manufacturing(
    vias: &[ViaDefinition],
    tech: TechNode,
    board_thickness_m: f64,
    lamination_limit: u32,
) -> Vec<ManufacturingViolation> {
    let mut violations = Vec::new();

    // Lamination cycle check (batch)
    if let Some(v) = check_lamination_cycles(vias, lamination_limit) {
        violations.push(v);
    }

    // Per-via checks
    for via in vias {
        let via_viols = check_via_constraints(via, tech, board_thickness_m);
        violations.extend(via_viols);
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_via(drill_nm: i64, pad_nm: i64, net_id: usize, location: (i64, i64)) -> ViaDefinition {
        ViaDefinition {
            drill_diameter_nm: drill_nm,
            pad_diameter_nm: pad_nm,
            net_id,
            location,
            layer_from: 0,
            layer_to: 1,
        }
    }

    #[test]
    fn test_aspect_ratio_thin_board_large_drill_pass() {
        // board_thickness=1.6mm, drill=0.2mm → ratio=8:1 → pass
        let via = make_via(200_000, 400_000, 1, (0, 0));
        let v = check_via_aspect_ratio(&via, 1.6e-3);
        assert!(v.is_none());
    }

    #[test]
    fn test_aspect_ratio_thick_board_small_drill_violation() {
        // board_thickness=3.2mm, drill=0.1mm → ratio=32:1 → violation
        let via = make_via(100_000, 300_000, 1, (0, 0));
        let v = check_via_aspect_ratio(&via, 3.2e-3);
        assert!(v.is_some());
        let v = v.expect("violation expected");
        match &v.violation_type {
            MfgViolationType::ViaAspectRatio { ratio, limit } => {
                assert!(*ratio > IPC_CLASS3_ASPECT_RATIO_LIMIT);
                assert_eq!(*limit, IPC_CLASS3_ASPECT_RATIO_LIMIT);
            }
            _ => panic!("Expected ViaAspectRatio violation"),
        }
    }

    #[test]
    fn test_lamination_within_limit() {
        let vias = vec![
            make_via(100_000, 300_000, 1, (0, 0)),
            make_via(100_000, 300_000, 1, (0, 0)),
        ];
        let v = check_lamination_cycles(&vias, 3);
        assert!(v.is_none());
    }

    #[test]
    fn test_lamination_exceeds_limit() {
        let vias = vec![
            make_via(100_000, 300_000, 1, (0, 0)),
            make_via(100_000, 300_000, 1, (0, 0)),
            make_via(100_000, 300_000, 1, (0, 0)),
            make_via(100_000, 300_000, 1, (0, 0)),
        ];
        let v = check_lamination_cycles(&vias, 3);
        assert!(v.is_some());
        let v = v.expect("violation expected");
        match &v.violation_type {
            MfgViolationType::LaminationCycleExceed { count, limit } => {
                assert_eq!(*count, 4);
                assert_eq!(*limit, 3);
            }
            _ => panic!("Expected LaminationCycleExceed violation"),
        }
    }

    #[test]
    fn test_solder_mask_expansion_within_range() {
        let v = check_solder_mask_expansion(300_000, 50_000, 25_000, 75_000);
        assert!(v.is_none());
    }

    #[test]
    fn test_solder_mask_expansion_outside_range() {
        let v = check_solder_mask_expansion(300_000, 100_000, 25_000, 75_000);
        assert!(v.is_some());
        let v = v.expect("violation expected");
        match &v.violation_type {
            MfgViolationType::SolderMaskExpansion {
                actual,
                min_allowed,
                max_allowed,
            } => {
                assert_eq!(*actual, 100_000);
                assert_eq!(*min_allowed, 25_000);
                assert_eq!(*max_allowed, 75_000);
            }
            _ => panic!("Expected SolderMaskExpansion violation"),
        }
    }

    #[test]
    fn test_ipc_class3_vs_asic_constraints() {
        let mut via = make_via(200_000, 400_000, 1, (0, 0));

        // IPC Class 3: thin board → pass
        let viols = check_via_constraints(&via, TechNode::PcbIpcClass3, 1.6e-3);
        assert!(viols.is_empty());

        // ASIC layer-local: single layer pair → pass
        let viols = check_via_constraints(&via, TechNode::AsicLayerLocal, 1.6e-3);
        assert!(viols.is_empty());

        // ASIC layer-local: multi-layer span → violation
        via.layer_from = 0;
        via.layer_to = 3;
        let viols = check_via_constraints(&via, TechNode::AsicLayerLocal, 1.6e-3);
        assert_eq!(viols.len(), 1);
        match &viols[0].violation_type {
            MfgViolationType::ViaLayerViolation { .. } => {}
            _ => panic!("Expected ViaLayerViolation"),
        }
    }

    #[test]
    fn test_batch_verify_manufacturing() {
        let vias = vec![
            make_via(200_000, 400_000, 1, (0, 0)),
            make_via(200_000, 400_000, 1, (0, 0)),
            make_via(200_000, 400_000, 1, (0, 0)),
        ];
        // IPC Class 3, thin board, lamination limit 2 → violation (3 > 2)
        let violations = verify_manufacturing(&vias, TechNode::PcbIpcClass3, 1.6e-3, 2);
        assert!(!violations.is_empty());
        let has_lam = violations.iter().any(|v| {
            matches!(
                v.violation_type,
                MfgViolationType::LaminationCycleExceed { .. }
            )
        });
        assert!(has_lam);
    }
}
