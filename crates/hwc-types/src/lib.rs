//! Core type definitions shared across Hardware Script compiler crates.
//!
//! This crate contains fundamental types that need to be shared across
//! multiple crates without creating circular dependencies.

mod def_path;
mod unit_registry;

pub use def_path::DefPath;
pub use unit_registry::{UnitInfo, UnitRegistry};

/// 7-Base SI Dimensional Exponent Vector `[L, M, T, I, Theta, N, J]`
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SiDimension {
    pub length: i8,      // L (meters)
    pub mass: i8,        // M (kilograms)
    pub time: i8,        // T (seconds)
    pub current: i8,     // I (amperes)
    pub temp: i8,        // Theta (kelvin)
    pub amount: i8,      // N (moles)
    pub luminosity: i8,  // J (candelas)
}

impl SiDimension {
    pub const DIMENSIONLESS: Self        = Self { length: 0, mass: 0, time: 0, current: 0, temp: 0, amount: 0, luminosity: 0 };
    pub const LENGTH: Self               = Self { length: 1, mass: 0, time: 0, current: 0, temp: 0, amount: 0, luminosity: 0 };
    pub const AREA: Self                 = Self { length: 2, mass: 0, time: 0, current: 0, temp: 0, amount: 0, luminosity: 0 };
    pub const VOLUME: Self               = Self { length: 3, mass: 0, time: 0, current: 0, temp: 0, amount: 0, luminosity: 0 };
    pub const TIME: Self                 = Self { length: 0, mass: 0, time: 1, current: 0, temp: 0, amount: 0, luminosity: 0 };
    pub const CURRENT: Self              = Self { length: 0, mass: 0, time: 0, current: 1, temp: 0, amount: 0, luminosity: 0 };
    pub const VOLTAGE: Self              = Self { length: 2, mass: 1, time: -3, current: -1, temp: 0, amount: 0, luminosity: 0 };
    pub const RESISTANCE: Self           = Self { length: 2, mass: 1, time: -3, current: -2, temp: 0, amount: 0, luminosity: 0 };
    pub const SHEET_RES: Self            = Self { length: 0, mass: 1, time: -3, current: -2, temp: 0, amount: 0, luminosity: 0 };
    pub const CAPACITANCE: Self          = Self { length: -2, mass: -1, time: 4, current: 2, temp: 0, amount: 0, luminosity: 0 };
    pub const CAPACITANCE_DENSITY: Self  = Self { length: -4, mass: -1, time: 4, current: 2, temp: 0, amount: 0, luminosity: 0 };
    pub const INDUCTANCE: Self           = Self { length: 2, mass: 1, time: -2, current: -2, temp: 0, amount: 0, luminosity: 0 };
    pub const TEMPERATURE: Self          = Self { length: 0, mass: 0, time: 0, current: 0, temp: 1, amount: 0, luminosity: 0 };
    pub const POWER: Self                = Self { length: 2, mass: 1, time: -3, current: 0, temp: 0, amount: 0, luminosity: 0 };
    pub const FREQUENCY: Self            = Self { length: 0, mass: 0, time: -1, current: 0, temp: 0, amount: 0, luminosity: 0 };
    pub const ANGLE: Self                = Self { length: 0, mass: 0, time: 0, current: 0, temp: 0, amount: 0, luminosity: 0 };

    #[inline(always)]
    pub const fn mul(self, rhs: Self) -> Self {
        Self {
            length: self.length + rhs.length,
            mass: self.mass + rhs.mass,
            time: self.time + rhs.time,
            current: self.current + rhs.current,
            temp: self.temp + rhs.temp,
            amount: self.amount + rhs.amount,
            luminosity: self.luminosity + rhs.luminosity,
        }
    }

    #[inline(always)]
    pub const fn div(self, rhs: Self) -> Self {
        Self {
            length: self.length - rhs.length,
            mass: self.mass - rhs.mass,
            time: self.time - rhs.time,
            current: self.current - rhs.current,
            temp: self.temp - rhs.temp,
            amount: self.amount - rhs.amount,
            luminosity: self.luminosity - rhs.luminosity,
        }
    }

    #[inline(always)]
    pub const fn is_dimensionless(&self) -> bool {
        self.length == 0
            && self.mass == 0
            && self.time == 0
            && self.current == 0
            && self.temp == 0
            && self.amount == 0
            && self.luminosity == 0
    }

    #[inline(always)]
    pub fn si_to_internal_scale(&self) -> f64 {
        if self.length == 1 && self.mass == 0 && self.time == 0 && self.current == 0 && self.temp == 0 && self.amount == 0 && self.luminosity == 0 {
            1e12
        } else if self.length == 2 && self.mass == 0 && self.time == 0 && self.current == 0 && self.temp == 0 && self.amount == 0 && self.luminosity == 0 {
            1e24
        } else if self.length == 3 && self.mass == 0 && self.time == 0 && self.current == 0 && self.temp == 0 && self.amount == 0 && self.luminosity == 0 {
            1e36
        } else if self.length == 2 && self.mass == 1 && self.time == -3 && self.current == -1 && self.temp == 0 && self.amount == 0 && self.luminosity == 0 {
            1e6
        } else if self.length == 0 && self.mass == 0 && self.time == 0 && self.current == 1 && self.temp == 0 && self.amount == 0 && self.luminosity == 0 {
            1e6
        } else if self.length == 2 && self.mass == 1 && self.time == -3 && self.current == -2 && self.temp == 0 && self.amount == 0 && self.luminosity == 0 {
            1e3
        } else if self.length == -2 && self.mass == -1 && self.time == 4 && self.current == 2 && self.temp == 0 && self.amount == 0 && self.luminosity == 0 {
            1e15
        } else if self.length == 2 && self.mass == 1 && self.time == -2 && self.current == -2 && self.temp == 0 && self.amount == 0 && self.luminosity == 0 {
            1e12
        } else {
            1.0
        }
    }
}

/// Strongly-typed net ID (newtype wrapper around u32).
///
/// Zero memory overhead - compiles to a raw u32.
/// Provides compile-time safety for net identification across the codebase.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
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

/// Strongly-typed physical stackup layer ID (newtype wrapper around u8).
///
/// Enables O(1) direct indexing into stackup arrays with zero string hashing.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct LayerId(pub u8);

impl LayerId {
    #[inline]
    pub const fn new(id: u8) -> Self {
        Self(id)
    }

    #[inline]
    pub const fn raw(self) -> u8 {
        self.0
    }

    #[inline]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Explicit geometrical cross-section of via holes / contact cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ViaApertureShape {
    /// Circular cylinder (PCB mechanical/laser drill) -> Area = π(d/2)²
    Circular,
    /// Square / rectangular aperture (ASIC photolithography cut) -> Area = w * h
    Square,
    /// Custom polygon boundary
    Polygon,
}

impl ViaApertureShape {
    #[inline]
    pub fn calculate_area_cm2(&self, diameter_nm: i64) -> f64 {
        let dim_cm = (diameter_nm as f64) * 1e-7;
        match self {
            Self::Square => dim_cm * dim_cm,
            Self::Circular => std::f64::consts::PI * (dim_cm * 0.5) * (dim_cm * 0.5),
            Self::Polygon => dim_cm * dim_cm,
        }
    }
}

/// Declares whether a contact is part of a subcircuit compact model
/// or represents routing interconnect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ContactExemption {
    /// Standard routing interconnect: extract lumped via resistor (Rvia)
    Interconnect,
    /// Device-internal interface (already embedded in subcircuit model, e.g., RR_head/RR_tail)
    /// Excludes from lumped extraction to prevent double-counting.
    SubcircuitInternal {
        device_id: u32,
    },
}


/// Technology strategy for PCB vs ASIC design rules.
///
/// This enum represents the two supported technology families and encapsulates
/// Technology type distinguishing between PCB and ASIC design rules.
///
/// This enum determines manufacturing-specific behavior throughout the compiler:
/// - Via geometry (drilled holes with enclosure pads vs photolithographic contacts)
/// - Clearance rules (IPC standards vs process design rules)
/// - Layer stack assumptions (copper + FR4 vs deposited metal + oxide)
///
/// **IMPORTANT**: This must be explicitly declared in every profile. No defaults
/// are permitted to prevent accidental mismatches between design intent and
/// manufacturing constraints.
///
/// This type serves as the single source of truth for technology-specific logic
/// across the entire codebase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Technology {
    /// Printed Circuit Board technology (drilled vias, copper traces on FR4)
    Pcb,
    /// Application-Specific Integrated Circuit technology (photolithography)
    Asic,
}

impl std::str::FromStr for Technology {
    type Err = TechnologyParseError;

    /// Parse a technology string from profile definition.
    ///
    /// Case-insensitive matching for user convenience.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pcb" => Ok(Self::Pcb),
            "asic" => Ok(Self::Asic),
            _ => Err(TechnologyParseError(s.to_string())),
        }
    }
}

/// Error returned when a technology string cannot be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechnologyParseError(pub String);

impl std::fmt::Display for TechnologyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid technology '{}'", self.0)
    }
}

impl std::error::Error for TechnologyParseError {}

impl Technology {
    /// Parse a technology string from profile definition.
    ///
    /// Case-insensitive matching for user convenience.
    pub fn parse(s: &str) -> Option<Self> {
        s.parse::<Self>().ok()
    }

    /// Convert to canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pcb => "PCB",
            Self::Asic => "ASIC",
        }
    }
}

/// Legacy alias for Technology enum.
///
impl Technology {
    /// Compute the contact expansion for this technology.
    ///
    /// For PCB the expansion equals `min_enclosure_nm`.
    /// For ASIC the expansion is always zero.
    #[inline]
    pub const fn contact_expansion(&self, min_enclosure_nm: i64) -> i64 {
        match self {
            Self::Pcb => min_enclosure_nm,
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
    fn technology_from_str() {
        assert_eq!("PCB".parse::<Technology>(), Ok(Technology::Pcb));
        assert_eq!("pcb".parse::<Technology>(), Ok(Technology::Pcb));
        assert_eq!("ASIC".parse::<Technology>(), Ok(Technology::Asic));
        assert_eq!("asic".parse::<Technology>(), Ok(Technology::Asic));
        assert!("invalid".parse::<Technology>().is_err());
    }

    #[test]
    fn technology_as_str() {
        assert_eq!(Technology::Pcb.as_str(), "PCB");
        assert_eq!(Technology::Asic.as_str(), "ASIC");
    }

    #[test]
    fn technology_name() {
        assert_eq!(Technology::Pcb.name(), "PCB");
        assert_eq!(Technology::Asic.name(), "ASIC");
    }

    #[test]
    fn technology_is_pcb() {
        assert!(Technology::Pcb.is_pcb());
        assert!(!Technology::Asic.is_pcb());
    }

    #[test]
    fn technology_is_asic() {
        assert!(Technology::Asic.is_asic());
        assert!(!Technology::Pcb.is_asic());
    }

    #[test]
    fn contact_expansion_pcb() {
        let tech = Technology::Pcb;
        assert_eq!(tech.contact_expansion(10), 10);
        assert_eq!(tech.contact_expansion(0), 0);
        assert_eq!(tech.contact_expansion(-3), -3);
    }

    #[test]
    fn contact_expansion_asic() {
        let tech = Technology::Asic;
        assert_eq!(tech.contact_expansion(10), 0);
        assert_eq!(tech.contact_expansion(0), 0);
    }

    #[test]
    fn port_escape_clearance_pcb() {
        let tech = Technology::Pcb;
        assert_eq!(tech.port_escape_clearance(100, 50), 100);
        assert_eq!(tech.port_escape_clearance(0, 50), 50);
        assert_eq!(tech.port_escape_clearance(200, 30), 130);
    }

    #[test]
    fn port_escape_clearance_asic() {
        let tech = Technology::Asic;
        assert_eq!(tech.port_escape_clearance(100, 50), 50);
        assert_eq!(tech.port_escape_clearance(0, 50), 50);
    }

    #[test]
    fn obstacle_inflation_delegates_to_port_escape_clearance() {
        let pcb = Technology::Pcb;
        let asic = Technology::Asic;
        assert_eq!(
            pcb.obstacle_inflation(100, 50),
            pcb.port_escape_clearance(100, 50)
        );
        assert_eq!(
            asic.obstacle_inflation(100, 50),
            asic.port_escape_clearance(100, 50)
        );
    }
}
