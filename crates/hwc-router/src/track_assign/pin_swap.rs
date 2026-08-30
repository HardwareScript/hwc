//! Dynamic Pin Swapping via NPN Automorphism Group Symmetries
//!
//! Evaluates input pin symmetries (e.g. A <-> B on NAND/NOR gates) to legally swap
//! net connections in O(1), untangling planar crossings and eliminating vias.

use compact_str::CompactString;

/// Automorphism group symmetry for standard cell inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSymmetryGroup {
    /// No symmetric inputs (e.g. Inverter, Buffer, DFF)
    None,
    /// Symmetric pair (e.g. S_2 on NAND2, NOR2, AND2)
    Pair(CompactString, CompactString),
    /// Symmetric triplet (e.g. S_3 on NAND3, NOR3)
    Triplet(CompactString, CompactString, CompactString),
}

/// Evaluates if two incoming nets can be swapped to prevent a wire crossing.
pub fn try_swap_symmetric_pins(
    symmetry: &InputSymmetryGroup,
    pin_a_y_pm: i64,
    pin_b_y_pm: i64,
    net_a_source_y_pm: i64,
    net_b_source_y_pm: i64,
) -> bool {
    match symmetry {
        InputSymmetryGroup::Pair(_, _) => {
            // Net crossing condition: source_a > source_b but pin_a < pin_b
            let sources_crossed = (net_a_source_y_pm > net_b_source_y_pm) != (pin_a_y_pm > pin_b_y_pm);
            sources_crossed
        }
        InputSymmetryGroup::Triplet(_, _, _) => {
            let sources_crossed = (net_a_source_y_pm > net_b_source_y_pm) != (pin_a_y_pm > pin_b_y_pm);
            sources_crossed
        }
        InputSymmetryGroup::None => false,
    }
}
