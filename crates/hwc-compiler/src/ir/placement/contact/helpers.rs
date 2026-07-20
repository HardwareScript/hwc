use compact_str::CompactString;

use hwc_engine::geometry_router::entity_graph::CapType;
use hwc_engine::HardwareSpace;

use crate::ir::errors::IrError;

/// Resolve the PDK-declared circle fidelity (`manufacturing.circle_segments`).
///
/// Circular geometry (vias, pads, tubes, TSVs) must agree between geometry
/// generation and mesh export, so the segment count is sourced from the
/// profile rather than hardcoded per call site. Errors if the PDK profile has
/// not declared it — there is no silent default.
pub(super) fn resolve_circle_segments(space: &HardwareSpace) -> Result<u32, IrError> {
    space
        .fabrication_constraints
        .as_ref()
        .map(|c| c.circle_segments)
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "PDK profile is missing 'manufacturing.circle_segments'".into(),
            hint: "Add 'circle_segments: <n>' to the manufacturing block of your profile \
                   (e.g. circle_segments: 64)."
                .into(),
        })
}

pub(super) fn get_prop_nm(
    contact: &hwc_parser::ContactPlacement,
    name: &str,
    _symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Option<i64> {
    contact.properties.get(name).and_then(|expr| {
        expr.evaluate(eval_context)
            .ok()
            .and_then(|val| val.to_nanometers().ok())
    })
}

pub(super) fn get_prop_bool(
    contact: &hwc_parser::ContactPlacement,
    name: &str,
    eval_context: &hwc_parser::EvaluationContext,
) -> Option<bool> {
    contact.properties.get(name).and_then(|expr| {
        if let hwc_parser::Expression::Variable { name, .. } = expr {
            match name.as_str() {
                "true" => return Some(true),
                "false" => return Some(false),
                _ => {}
            }
        }
        expr.evaluate(eval_context)
            .ok()
            .and_then(|val| val.as_integer().ok().map(|i| i != 0))
    })
}

pub(super) fn get_prop_string(
    contact: &hwc_parser::ContactPlacement,
    name: &str,
    _eval_context: &hwc_parser::EvaluationContext,
) -> Option<CompactString> {
    contact.properties.get(name).and_then(|expr| match expr {
        hwc_parser::Expression::Variable { name, .. } => Some(name.clone()),
        _ => None,
    })
}

pub(super) fn get_prop_cap_type(
    contact: &hwc_parser::ContactPlacement,
    name: &str,
    _eval_context: &hwc_parser::EvaluationContext,
) -> Option<CapType> {
    contact.properties.get(name).and_then(|expr| match expr {
        hwc_parser::Expression::Variable { name, .. } => match name.as_str() {
            "none" => Some(CapType::None),
            "annular" => Some(CapType::Annular),
            "solid" => Some(CapType::Solid),
            _ => None,
        },
        _ => None,
    })
}
