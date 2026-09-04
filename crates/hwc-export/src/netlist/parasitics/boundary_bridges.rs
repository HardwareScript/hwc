//! Boundary net bridging: connects top-level stimulus nets to physical pad landing nodes.
//!
//! ## Zero String-Scraping Mandate
//!
//! This module must NOT inspect the string contents of generated node names
//! (e.g. looking for "_start", "_end" suffixes, foundry-specific prefixes). Classification
//! flows exclusively through `classify_pour` which uses typed `PourMetadata` fields.
//!
//! ## Canonical Circuit Topology
//!
//!   `In` (Stimulus Source)
//!     ──► Rpad_stimulus_bridge_In (1e-4 Ω)
//!     ──► nIn_pad    ← Cgnd_pour_pad attaches HERE (emitted by pours.rs)
//!     ──► Rtr_In     (physical trace resistance, already emitted by routes.rs)
//!     ──► nIn_metal1_0
//!     ──► Rvia_In    (already emitted by contacts.rs)
//!     ──► nIn_li1_0  ──► DUT Terminal A

use crate::netlist::types::{ParasiticElement, PhysicalNetlistGraph};
use hwc_engine::HardwareSpace;

pub fn emit_boundary_pad_bridges(
    space: &HardwareSpace,
    graph: &mut PhysicalNetlistGraph,
) {
    // Collect every net that has an ExternalPad pour.
    // This is the TYPED oracle: classify_pour returns ExternalPad iff the pour
    // sits on the dedicated pad mask layer (thickness=0, is_mask=true, name="pad").
    // No string heuristics. No name inspection.
    let mut pad_nets: Vec<String> = Vec::new();
    for pour in &space.pours {
        if let Some(ref net) = pour.net {
            if matches!(
                super::geometry::classify_pour(space, pour),
                super::types::PourRole::ExternalPad { .. }
            ) {
                let s = net.as_str().to_owned();
                if !pad_nets.contains(&s) {
                    pad_nets.push(s);
                }
            }
        }
    }

    // For each pad net, verify a Cgnd_pour_pad capacitance node already exists
    // (emitted by pours.rs) and emit the stimulus bridge.
    for net_name in &pad_nets {
        let pad_node = format!("n{}_pad", net_name);

        // Check if the pad node is already referenced anywhere in the graph.
        // pours.rs emits GroundCapacitance to n{Net}_pad for metal pad pours.
        let pad_node_exists = graph.parasitics.iter().any(|p| match p {
            ParasiticElement::GroundCapacitance { node, .. } => node == &pad_node,
            ParasiticElement::TraceResistor { node_a, node_b, .. } => {
                node_a == &pad_node || node_b == &pad_node
            }
            _ => false,
        });

        // Also skip if a stimulus bridge already exists for this net.
        let already_bridged = graph.parasitics.iter().any(|p| match p {
            ParasiticElement::TraceResistor { node_a, node_b, .. } => {
                node_a == net_name || node_b == net_name
            }
            _ => false,
        });

        if already_bridged {
            continue;
        }

        if pad_node_exists {
            // Standard case: pad capacitance was emitted → bridge stimulus to pad node.
            graph.parasitics.push(ParasiticElement::TraceResistor {
                name: format!("Rpad_stimulus_bridge_{}", net_name),
                node_a: net_name.clone(),
                node_b: pad_node,
                value_ohms: 1.0e-4,
            });
        } else {
            // The pad mask pour exists but no metal capacitance was emitted (edge case:
            // tiny pad area below 1e-17 F threshold). Still emit the bridge so the
            // stimulus node connects to at least the trace resistor chain.
            // Create the pad node as a placeholder — downstream trace resistors will
            // connect from n{Net}_pad automatically via routes.rs.
            graph.parasitics.push(ParasiticElement::TraceResistor {
                name: format!("Rpad_stimulus_bridge_{}", net_name),
                node_a: net_name.clone(),
                node_b: pad_node,
                value_ohms: 1.0e-4,
            });
        }
    }
}
