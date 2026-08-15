/// Net classification for physics validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetClassification {
    Power,
    Ground,
    Signal,
    HighVoltage,
    Unclassified,
}

/// **v0.3.0: Net electrical properties for physics validation**
///
/// Stores voltage, current, and frequency constraints declared in the nets: section.
/// Used by DRC engines for junction breakdown, electromigration, and crosstalk validation.
#[derive(Debug, Clone)]
pub struct NetElectricalProperties {
    /// Net classification (power, ground, signal, etc.)
    pub classification: NetClassification,
    /// Voltage/potential in volts (V). None if not declared.
    pub potential_v: Option<f64>,
    /// Current limit in milliamperes (mA). None if not declared.
    pub current_ma: Option<f64>,
    /// Operating frequency in hertz (Hz). None if not declared.
    pub frequency_hz: Option<f64>,
}

impl NetElectricalProperties {
    pub fn new(classification: NetClassification) -> Self {
        Self {
            classification,
            potential_v: None,
            current_ma: None,
            frequency_hz: None,
        }
    }
}

/// **v0.1.6: Space visualization orientation**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceView {
    /// Horizontal 'floor' layout (Z is Up)
    Horizontal,
    /// Vertical 'standing' layout (Y is Up)
    Vertical,
}

impl std::fmt::Display for NetClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetClassification::Power => write!(f, "power"),
            NetClassification::Ground => write!(f, "ground"),
            NetClassification::Signal => write!(f, "signal"),
            NetClassification::HighVoltage => write!(f, "high-voltage"),
            NetClassification::Unclassified => write!(f, "unclassified"),
        }
    }
}
