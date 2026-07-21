//! Interface capability types and constraint derivation.

use super::types::{DerivedConstraint, RoutingDatabase};

/// Physical capability of an interface, derivable into routing constraints.
///
/// Each capability maps to specific constraint derivation rules that the
/// router uses to enforce physical limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InterfaceCapability {
    /// Maximum current through this interface (in microamps).
    /// Derives: minimum trace width, minimum via count, allowed materials.
    CarryCurrent { max_ua: u32 },

    /// Signal bandwidth constraint (in GHz).
    /// Derives: maximum trace length, impedance matching requirements.
    SignalBandwidth { max_ghz: u32 },

    /// Thermal dissipation capability (in microwatts per kelvin).
    /// Derives: thermal via requirements, keepout zones.
    CarryHeat { max_uw_k: u32 },

    /// Optical coupling (future: photonics).
    OpticalCoupling { wavelength_nm: u32 },
}

impl InterfaceCapability {
    /// Convert capability into a routing constraint.
    ///
    /// The `RoutingDatabase` trait provides technology-specific parameters
    /// (e.g., max current density from material properties).
    pub fn derive_constraint(&self, db: &dyn RoutingDatabase) -> DerivedConstraint {
        match self {
            Self::CarryCurrent { max_ua } => self.derive_current_constraint(*max_ua, db),
            Self::SignalBandwidth { max_ghz } => self.derive_bandwidth_constraint(*max_ghz),
            Self::CarryHeat { .. } => DerivedConstraint::ThermalViaRequired,
            Self::OpticalCoupling { .. } => DerivedConstraint::None,
        }
    }

    fn derive_current_constraint(
        &self,
        max_ua: u32,
        db: &dyn RoutingDatabase,
    ) -> DerivedConstraint {
        let current_density = db.get_max_current_density_ua_per_nm();
        if current_density == 0 {
            return DerivedConstraint::None;
        }
        let min_width_nm = (max_ua as i64) / current_density;
        DerivedConstraint::MinimumTraceWidth(min_width_nm.max(1))
    }

    fn derive_bandwidth_constraint(&self, max_ghz: u32) -> DerivedConstraint {
        if max_ghz == 0 {
            return DerivedConstraint::None;
        }
        // Speed of light approximation: max_length_nm = 3_000_000_000 / max_ghz
        let max_length_nm = 3_000_000_000 / (max_ghz as i64);
        DerivedConstraint::MaximumTraceLength(max_length_nm)
    }
}
