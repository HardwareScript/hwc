use crate::lexer::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Net classification for physics validation (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetDeclaration {
    pub name: CompactString,
    pub classification: NetClassification,
    pub potential_mv: Option<i64>,
    pub current_ma: Option<f64>,
    pub frequency_hz: Option<f64>,
    pub span: Span,
}

/// Net classification types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetClassification {
    Power,
    Ground,
    Signal,
    HighVoltage,
    Unclassified,
}
