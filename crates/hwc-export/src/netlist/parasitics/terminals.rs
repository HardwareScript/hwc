//! Stage 5: Deterministic device terminal → physical layer node binding.
//!
//! ## Zero-Heuristic Terminal Binding (v0.3.0)
//!
//! The canonical binding law:
//!
//! * **BULK / SUB terminals** → the extracted node on the layer with the **lowest `z_bottom`**
//!   in the stackup for the terminal's net.  This is always the deepest semiconductor layer
//!   (e.g. `pdiff`, `ndiff`) where the compact subcircuit model's bulk pin physically sits.
//!
//! * **All other terminals** → the extracted node on the **lowest-Z routable** (`is_routable`)
//!   layer for the terminal's net.  On most planar CMOS processes this is `li1` (Local
//!   Interconnect Layer 1), the first metal layer above the active device surface.
//!
//! No string matching.  No fallback priority lists.  No pour scanning.  The only source of
//! truth is the set of `(net, layer)` → `[ExtractedClusterNode]` entries built by the
//! via-stack and trace extractors in earlier stages, combined with the ordered physical
//! stackup (`space.stackup_layers`) that declares the Z coordinate of every layer.

use rustc_hash::FxHashMap;

use super::types::ExtractedClusterNode;
use crate::netlist::types::{PhysicalNetlist, PhysicalNetlistGraph};
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;

/// Stage 5: Bind each device terminal to the correct extracted physical layer node.
///
/// ## Contract
///
/// * Exactly one node is chosen per terminal.  If no extracted node exists for the terminal's
///   net on any layer, the terminal is silently skipped (it will be absent from
///   `graph.device_nodes` and therefore absent from the SPICE subcircuit line).
/// * The chosen layer is determined purely by Z ordering from `space.stackup_layers`; there
///   is no string-based heuristic.
pub fn map_device_terminals(
    space: &HardwareSpace,
    _symbol_table: &SymbolTable,
    physical_netlist: Option<&PhysicalNetlist>,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
) {
    let netlist = match physical_netlist {
        Some(nl) => nl,
        None => return,
    };

    // Build a name → z_bottom lookup from the authoritative stackup.
    // This is the only spatial ordering used below — no layer name matching.
    let z_bottom_of: FxHashMap<&str, i64> = space
        .stackup_layers
        .iter()
        .map(|sl| (sl.name.as_str(), sl.z_bottom))
        .collect();

    // Build a name → is_routable lookup from the authoritative stackup.
    let is_routable_of: FxHashMap<&str, bool> = space
        .stackup_layers
        .iter()
        .map(|sl| (sl.name.as_str(), sl.is_routable))
        .collect();

    for device in &netlist.devices {
        for (term_name, term_net) in &device.terminals {
            // Collect every (layer_name, nodes) pair that the extractors recorded for this net.
            let net_layers: Vec<(&str, &Vec<ExtractedClusterNode>)> = extracted_layer_nodes
                .iter()
                .filter(|((net, _layer), _nodes)| net == term_net.as_str())
                .map(|((_, layer), nodes)| (layer.as_str(), nodes))
                .collect();

            if net_layers.is_empty() {
                                continue;
            }

            let is_bulk =
                term_name.eq_ignore_ascii_case("bulk") || term_name.eq_ignore_ascii_case("sub");

            // Select the winning layer according to the binding law above.
            let winning_layer: Option<&str> = if is_bulk {
                // BULK → lowest-Z layer (deepest semiconductor interface)
                net_layers
                    .iter()
                    .min_by_key(|(layer, _nodes)| z_bottom_of.get(layer).copied().unwrap_or(i64::MAX))
                    .map(|(layer, _)| *layer)
            } else {
                // Signal → lowest-Z routable layer (first metal / LI above the device surface)
                net_layers
                    .iter()
                    .filter(|(layer, _nodes)| is_routable_of.get(layer).copied().unwrap_or(false))
                    .min_by_key(|(layer, _nodes)| z_bottom_of.get(layer).copied().unwrap_or(i64::MAX))
                    .map(|(layer, _)| *layer)
                    // If no routable layer exists, fall through to overall lowest-Z
                    .or_else(|| {
                        net_layers
                            .iter()
                            .min_by_key(|(layer, _nodes)| z_bottom_of.get(layer).copied().unwrap_or(i64::MAX))
                            .map(|(layer, _)| *layer)
                    })
            };

            let Some(layer) = winning_layer else {
                                continue;
            };

            // From the winning layer's node list pick the single node (clusters produce exactly
            // one node per (net, layer) in the current extraction model; if multiple exist, take
            // the first — spatial tie-breaking is not needed here because via clusters are
            // de-duplicated by the via-stack extractor).
            let node = extracted_layer_nodes
                .get(&(term_net.to_string(), layer.to_string()))
                .and_then(|nodes| nodes.first())
                .map(|n| n.node.clone());

            if let Some(node) = node {
                                graph
                    .device_nodes
                    .insert((device.name.to_string(), term_name.to_string()), node);
            }
        }
    }
}
