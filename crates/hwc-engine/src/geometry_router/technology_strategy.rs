//! Technology-Specific Routing Strategies
//!
//! Encapsulates the physical differences between PCB and ASIC routing:
//! - PCB: Drilled/plated vias with annular rings (pad overhang)
//! - ASIC: Deposited contacts with flush boundaries (no overhang)
//!
//! This eliminates scattered conditional logic throughout the router.

use crate::constraint_manager::FabricationConstraints;
use hwc_types::Technology;

impl Technology {
    /// Get technology directly from fabrication constraints.
    ///
    /// No inference - the technology field is required in the profile.
    pub fn from_constraints(fab: &FabricationConstraints) -> Self {
        fab.technology
    }

    /// Calculate port escape clearance based on technology.
    ///
    /// # PCB Mode
    /// ```text
    /// Escape clearance = (trace_width / 2) + min_clearance
    /// ```
    /// This accounts for trace width projection beyond the pad edge.
    ///
    /// # ASIC Mode
    /// ```text
    /// Escape clearance = min_clearance
    /// ```
    /// Traces connect flush to contact edges with no offset.
    pub fn calculate_port_escape_clearance(
        &self,
        trace_width_nm: i64,
        min_clearance_nm: i64,
    ) -> i64 {
        match self {
            Self::Pcb => (trace_width_nm / 2) + min_clearance_nm,
            Self::Asic => min_clearance_nm,
        }
    }

    /// Calculate obstacle inflation for navigable space extraction.
    ///
    /// Uses the same formula as port escape clearance to maintain
    /// consistency between escape point placement and obstacle avoidance.
    pub fn calculate_obstacle_inflation(
        &self,
        trace_width_nm: i64,
        min_clearance_nm: i64,
    ) -> i64 {
        self.calculate_port_escape_clearance(trace_width_nm, min_clearance_nm)
    }

    /// Human-readable technology name for logging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Pcb => "PCB",
            Self::Asic => "ASIC",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcb_strategy() {
        let strategy = Technology::Pcb;
        
        // PCB: (trace_width/2) + min_clearance
        assert_eq!(strategy.calculate_port_escape_clearance(200_000, 150_000), 250_000);
        assert_eq!(strategy.calculate_obstacle_inflation(200_000, 150_000), 250_000);
    }

    #[test]
    fn test_asic_strategy() {
        let strategy = Technology::Asic;
        
        // ASIC: min_clearance only (typically 0nm)
        assert_eq!(strategy.calculate_port_escape_clearance(200_000, 0), 0);
        assert_eq!(strategy.calculate_obstacle_inflation(200_000, 0), 0);
        
        // ASIC with non-zero clearance (rare but valid)
        assert_eq!(strategy.calculate_port_escape_clearance(200_000, 50_000), 50_000);
    }

    #[test]
    fn test_from_constraints() {
        let pcb_fab = FabricationConstraints {
            min_trace_width_nm: 200_000,
            min_trace_spacing_nm: 150_000,
            min_via_diameter_nm: 300_000,
            default_via_diameter_nm: 300_000,
            min_enclosure_nm: 150_000, // PCB: has annular ring
            min_spacing_nm: 300_000,
            low_voltage_clearance_nm: 200_000,
            medium_voltage_clearance_nm: 500_000,
            high_voltage_clearance_nm: 1_000_000,
            safety_factor: 2.0,
            stackup: None,
            technology: Technology::Pcb,
            layer_via_enclosures: rustc_hash::FxHashMap::default(),
            max_substrate_tap_distance_nm: None,
            substrate_net: None,
        };
        assert_eq!(Technology::from_constraints(&pcb_fab), Technology::Pcb);

        let asic_fab = FabricationConstraints {
            min_trace_width_nm: 200,
            min_trace_spacing_nm: 0,
            min_via_diameter_nm: 200,
            default_via_diameter_nm: 200,
            min_enclosure_nm: 0, // ASIC: no annular ring
            min_spacing_nm: 300,
            low_voltage_clearance_nm: 500,
            medium_voltage_clearance_nm: 500,
            high_voltage_clearance_nm: 500,
            safety_factor: 2.0,
            stackup: None,
            technology: Technology::Asic,
            layer_via_enclosures: rustc_hash::FxHashMap::default(),
            max_substrate_tap_distance_nm: Some(20_000),
            substrate_net: Some("Bulk".into()),
        };
        assert_eq!(Technology::from_constraints(&asic_fab), Technology::Asic);
    }
}
