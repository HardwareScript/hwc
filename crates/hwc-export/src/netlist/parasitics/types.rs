//! Typed domain representations for parasitic extraction and layout connectivity.

use compact_str::CompactString;
use hwc_engine::space::ContactMetadata;

/// Universal vacuum permittivity (F/m)
pub const EPS_0: f64 = 8.854187817e-12;

/// User-configurable rules and thresholds for Physical Parasitic Extraction (PEX).
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Spatial proximity threshold for grouping individual via cuts into a coherent vertical via pillar (in nanometers)
    pub via_cluster_radius_nm: f64,
    /// Proximity threshold for landing a routing trace endpoint onto a via cluster node (in nanometers)
    pub via_landing_radius_nm: f64,
    /// Maximum lateral distance for sidewall coupling capacitance calculation (in nanometers)
    pub max_coupling_distance_nm: f64,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        // Default nominal rules for sub-micron silicon ASIC processes (e.g. SKY130)
        Self {
            via_cluster_radius_nm: 600.0,
            via_landing_radius_nm: 300.0,
            max_coupling_distance_nm: 3000.0,
        }
    }
}

impl ExtractionConfig {
    /// Parse extraction rules from a user-defined profile section `extraction { ... }` or fall back to technology defaults.
    pub fn from_profile(
        profile: Option<&hwc_parser::ProfileDecl>,
        is_asic: bool,
    ) -> Self {
        let mut config = if is_asic {
            Self::default()
        } else {
            // PCB / Discrete packaging defaults
            Self {
                via_cluster_radius_nm: 500_000.0,
                via_landing_radius_nm: 250_000.0,
                max_coupling_distance_nm: 1_000_000.0,
            }
        };

        if let Some(prof) = profile {
            if let Some(ext_sec) = prof.sections.iter().find(|s| s.section_type == "extraction") {
                for (name, expr) in &ext_sec.fields {
                    let val_m = match expr {
                        hwc_parser::ast::Expression::Measurement { value, unit, .. } => {
                            unit.base_si_multiplier().map(|mul| *value * mul)
                        }
                        hwc_parser::ast::Expression::FloatLiteral { value, .. } => Some(*value),
                        hwc_parser::ast::Expression::Literal { value, .. } => Some(*value as f64),
                        _ => None,
                    };
                    if let Some(m) = val_m {
                        let nm = m * 1e9;
                        match name.as_str() {
                            "via_cluster_radius" | "via_cluster_radius_nm" => config.via_cluster_radius_nm = nm,
                            "via_landing_radius" | "via_landing_radius_nm" => config.via_landing_radius_nm = nm,
                            "coupling_distance" | "max_coupling_distance" | "max_coupling_distance_nm" => config.max_coupling_distance_nm = nm,
                            _ => {}
                        }
                    }
                }
            }
        }

        config
    }
}

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
