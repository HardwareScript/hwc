/// Net classification for physics validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetClassification {
    Power,
    Ground,
    Signal,
    HighVoltage,
    Unclassified,
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
