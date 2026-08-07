//! Provenance types: where a route segment came from.

use super::ids::RouteId;
use crate::geometry::TraceSegment;
use crate::netlist::NetId;
use compact_str::CompactString;
use std::fmt;

/// Source of a route segment (child instance or parent level)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteSource {
    /// Route originated from a child space instance
    ChildInstance {
        /// Instance name (e.g., "PMOS_Inst")
        instance: CompactString,
        /// Original net name in child space
        original_net: CompactString,
    },

    /// Route created at parent level
    ParentLevel {
        /// Source entity name (e.g., "PMOS_Inst.Out_Pad")
        from_entity: CompactString,
        /// Destination entity name (e.g., "NMOS_Inst.Out_Pad")
        to_entity: CompactString,
    },
}

impl fmt::Display for RouteSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouteSource::ChildInstance {
                instance,
                original_net,
            } => {
                write!(
                    f,
                    "child instance '{}' (original net: {})",
                    instance, original_net
                )
            }
            RouteSource::ParentLevel {
                from_entity,
                to_entity,
            } => {
                write!(f, "parent-level route: {} → {}", from_entity, to_entity)
            }
        }
    }
}

/// A route segment with full provenance information
#[derive(Debug, Clone)]
pub struct ProvenanceSegment {
    /// Network this segment belongs to
    pub net_id: NetId,

    /// Network name (for debugging)
    pub net_name: Option<CompactString>,

    /// The actual geometric segment
    pub segment: TraceSegment,

    /// Where this segment came from
    pub source: RouteSource,

    /// Unique identifier for this segment
    pub route_id: RouteId,
}
