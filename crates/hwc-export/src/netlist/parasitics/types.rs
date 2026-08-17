//! Typed domain representations for parasitic extraction and layout connectivity.

use compact_str::CompactString;
use hwc_engine::space::ContactMetadata;

/// Physical vacuum permittivity (F/m)
pub const EPS_0: f64 = 8.854187817e-12;

/// Proximity threshold for clustering vertical contact vias into a single column
pub const VIA_CLUSTER_RADIUS_NM: f64 = 2000.0;

/// Maximum lateral distance for sidewall coupling capacitance calculation
pub const MAX_COUPLING_DISTANCE_NM: f64 = 3000.0;

/// Semantic role of a layout pour in the circuit
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PourRole {
    /// External test pad or boundary port carrying the top-level net
    ExternalPad { net: CompactString },
    /// Physical contact landing head bound to a device terminal
    DeviceTerminal {
        device: CompactString,
        terminals: Vec<CompactString>,
    },
    /// Multi-tap power or ground bus plane
    PowerBus { net: CompactString },
    /// Localized interconnect strap (e.g. li1 metal head, contact landing pad)
    InterconnectStrap { net: Option<CompactString> },
}

/// Resolved physical endpoint for a route segment
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteEndpoint {
    /// Connects directly to an external pad (e.g. In, Mid, GND)
    Pad(String),
    /// Connects to a localized spatial via cluster node (e.g. nMid_metal1_0)
    ViaCluster(String),
    /// Internal trace junction node
    TraceJunction(String),
}

impl RouteEndpoint {
    /// Return the string identifier of the node
    pub fn node_name(&self) -> &str {
        match self {
            Self::Pad(name) | Self::ViaCluster(name) | Self::TraceJunction(name) => name,
        }
    }
}

/// An extracted physical node on a specific layer with its physical 2D centroid.
#[derive(Debug, Clone)]
pub struct ExtractedClusterNode {
    pub node: String,
    pub centroid: (f64, f64),
}

/// A spatial cluster of contacts on a specific net forming a coherent vertical via pillar.
pub struct SpatialCluster<'a> {
    pub cluster_idx: usize,
    pub centroid: (f64, f64),
    pub contacts: Vec<&'a ContactMetadata>,
}
