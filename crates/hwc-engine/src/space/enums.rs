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
/// Stores voltage, current budget, and frequency constraints declared in the nets: section.
/// Used by DRC engines for junction breakdown, electromigration, and crosstalk validation.
///
/// **CRITICAL SEMANTIC CLARIFICATION (v0.2.2+):**
/// The `current_ma` field stores the user's DECLARED BUDGET from `nets: { current: X }`,
/// NOT a simulated operating current. This is a design constraint, not a computed value.
///
/// **What `current_ma` Represents:**
/// - ✅ User's declared budget: "This net must safely carry up to X mA"
/// - ✅ Static DRC constraint for trace width sizing
/// - ✅ Capability validation input (budget vs. wire ampacity)
///
/// **What `current_ma` Does NOT Represent:**
/// - ❌ Simulated DC operating current (requires SPICE matrix solver)
/// - ❌ Measured branch current (requires .op/.tran simulation)
/// - ❌ Actual power-on current draw (requires solving I = V/R for circuit)
#[derive(Debug, Clone)]
pub struct NetElectricalProperties {
    /// Net classification (power, ground, signal, etc.)
    pub classification: NetClassification,
    /// Voltage/potential in volts (V). None if not declared.
    pub potential_v: Option<f64>,
    /// Current budget in milliamps (mA) - user's declared capability constraint.
    /// IMPORTANT: This is NOT simulated operating current, it's a design budget.
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
