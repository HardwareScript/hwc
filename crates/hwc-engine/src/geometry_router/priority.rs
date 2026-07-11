//! Net priority system for routing order.
//!
//! v0.1.8 ZERO-MAGIC: No heuristics. Net priority must be explicitly declared
//! in the PDK profile via `net_priority(name: "...", level: 5)`.
//! The compiler must NOT guess priority from net names.
//!
//! Priority is a simple `u8` where higher values are routed first.
//! The PDK profile declares priorities; the router reads them.

use crate::netlist::NetId;
use rustc_hash::FxHashMap;

/// Lookup table of net priorities from the PDK profile.
///
/// Populated from `routing.net_priorities` in the profile definition.
/// If a net is not in this map, it gets priority 0 (lowest).
pub type NetPriorityMap = FxHashMap<NetId, u8>;

/// Get the routing priority for a net. Returns 0 if not declared in the profile.
#[inline]
pub fn get_net_priority(net_id: NetId, priorities: &NetPriorityMap) -> u8 {
    priorities.get(&net_id).copied().unwrap_or(0)
}
