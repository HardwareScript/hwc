//! Core type definitions shared across Hardware Script compiler crates.
//!
//! This crate contains fundamental types that need to be shared across
//! multiple crates without creating circular dependencies.

/// Strongly-typed net ID (newtype wrapper around u32).
///
/// Zero memory overhead - compiles to a raw u32.
/// Provides compile-time safety for net identification across the codebase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct NetId(pub u32);

impl NetId {
    /// Semantic constant for unconnected/keepout zones.
    /// Components and pours with this net ID block all routing.
    pub const UNCONNECTED: NetId = NetId(0);

    /// Create a new net ID.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Check if this is an unconnected/keepout zone.
    #[inline]
    pub const fn is_unconnected(self) -> bool {
        self.0 == 0
    }
}

/// Technology strategy for PCB vs ASIC design rules.
///
/// This enum represents the two supported technology families and encapsulates
/// the differing design-rule calculations for each. It serves as the single
/// source of truth for technology-specific logic across the entire codebase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TechnologyStrategy {
    /// Printed circuit board technology.
    Pcb,
    /// Application-specific integrated circuit technology.
    Asic,
}

impl Default for TechnologyStrategy {
    /// Default to ASIC technology strategy.
    fn default() -> Self {
        Self::Asic
    }
}

impl TechnologyStrategy {
    /// Determine the technology strategy from the minimum annular ring value.
    ///
    /// If `min_annular_ring_nm` is greater than zero the technology is PCB,
    /// otherwise it is ASIC.
    #[inline]
    pub const fn from_annular_ring(min_annular_ring_nm: i64) -> Self {
        if min_annular_ring_nm > 0 {
            Self::Pcb
        } else {
            Self::Asic
        }
    }

    /// Compute the contact expansion for this technology.
    ///
    /// For PCB the expansion equals `min_annular_ring_nm`.
    /// For ASIC the expansion is always zero.
    #[inline]
    pub const fn contact_expansion(&self, min_annular_ring_nm: i64) -> i64 {
        match self {
            Self::Pcb => min_annular_ring_nm,
            Self::Asic => 0,
        }
    }

    /// Compute the port escape clearance for this technology.
    ///
    /// For PCB: `(trace_width_nm / 2) + min_clearance_nm`.
    /// For ASIC: `min_clearance_nm`.
    #[inline]
    pub const fn port_escape_clearance(&self, trace_width_nm: i64, min_clearance_nm: i64) -> i64 {
        match self {
            Self::Pcb => (trace_width_nm / 2) + min_clearance_nm,
            Self::Asic => min_clearance_nm,
        }
    }

    /// Compute the obstacle inflation for this technology.
    ///
    /// Delegates directly to [`port_escape_clearance`](Self::port_escape_clearance).
    #[inline]
    pub const fn obstacle_inflation(&self, trace_width_nm: i64, min_clearance_nm: i64) -> i64 {
        self.port_escape_clearance(trace_width_nm, min_clearance_nm)
    }

    /// Return a human-readable name for this technology strategy.
    #[inline]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Pcb => "PCB",
            Self::Asic => "ASIC",
        }
    }

    /// Returns `true` if this is the PCB technology strategy.
    #[inline]
    pub const fn is_pcb(&self) -> bool {
        matches!(self, Self::Pcb)
    }

    /// Returns `true` if this is the ASIC technology strategy.
    #[inline]
    pub const fn is_asic(&self) -> bool {
        matches!(self, Self::Asic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_asic() {
        assert_eq!(TechnologyStrategy::default(), TechnologyStrategy::Asic);
    }

    #[test]
    fn from_annular_ring_positive_is_pcb() {
        assert_eq!(TechnologyStrategy::from_annular_ring(1), TechnologyStrategy::Pcb);
        assert_eq!(TechnologyStrategy::from_annular_ring(100), TechnologyStrategy::Pcb);
    }

    #[test]
    fn from_annular_ring_zero_or_negative_is_asic() {
        assert_eq!(TechnologyStrategy::from_annular_ring(0), TechnologyStrategy::Asic);
        assert_eq!(TechnologyStrategy::from_annular_ring(-5), TechnologyStrategy::Asic);
    }

    #[test]
    fn contact_expansion_pcb() {
        let tech = TechnologyStrategy::Pcb;
        assert_eq!(tech.contact_expansion(10), 10);
        assert_eq!(tech.contact_expansion(0), 0);
        assert_eq!(tech.contact_expansion(-3), -3);
    }

    #[test]
    fn contact_expansion_asic() {
        let tech = TechnologyStrategy::Asic;
        assert_eq!(tech.contact_expansion(10), 0);
        assert_eq!(tech.contact_expansion(0), 0);
    }

    #[test]
    fn port_escape_clearance_pcb() {
        let tech = TechnologyStrategy::Pcb;
        assert_eq!(tech.port_escape_clearance(100, 50), 100);
        assert_eq!(tech.port_escape_clearance(0, 50), 50);
        assert_eq!(tech.port_escape_clearance(200, 30), 130);
    }

    #[test]
    fn port_escape_clearance_asic() {
        let tech = TechnologyStrategy::Asic;
        assert_eq!(tech.port_escape_clearance(100, 50), 50);
        assert_eq!(tech.port_escape_clearance(0, 50), 50);
    }

    #[test]
    fn obstacle_inflation_delegates_to_port_escape_clearance() {
        let pcb = TechnologyStrategy::Pcb;
        let asic = TechnologyStrategy::Asic;
        assert_eq!(pcb.obstacle_inflation(100, 50), pcb.port_escape_clearance(100, 50));
        assert_eq!(asic.obstacle_inflation(100, 50), asic.port_escape_clearance(100, 50));
    }

    #[test]
    fn name_pcb() {
        assert_eq!(TechnologyStrategy::Pcb.name(), "PCB");
    }

    #[test]
    fn name_asic() {
        assert_eq!(TechnologyStrategy::Asic.name(), "ASIC");
    }

    #[test]
    fn is_pcb() {
        assert!(TechnologyStrategy::Pcb.is_pcb());
        assert!(!TechnologyStrategy::Asic.is_pcb());
    }

    #[test]
    fn is_asic() {
        assert!(TechnologyStrategy::Asic.is_asic());
        assert!(!TechnologyStrategy::Pcb.is_asic());
    }
}
