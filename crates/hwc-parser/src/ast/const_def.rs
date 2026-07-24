//! Constant definitions for math.hw primitives (v0.1.6)

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::lexer::Span;

/// Constant definition: `const NAME: value`
///
/// v0.2.0: Supports optional `export` keyword for visibility control
///
/// Used in primitives/math.hw for mathematical and physical constants.
/// These are resolved at parse time and enable compile-time constant folding.
///
/// Example:
/// ```hw
/// const PI: 3.14159265358979323846
/// const SPEED_OF_LIGHT: 299792458
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstDefinition {
    pub name: CompactString,
    pub is_exported: bool, // v0.2.0: Access control
    pub value: f64,
    pub span: Span,
}
