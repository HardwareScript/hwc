pub mod ast;
pub mod error_codes;
pub mod lexer;
pub mod parser;

// Re-export DiagnosticCollector from hwc-diagnostics
pub use hwc_diagnostics::DiagnosticCollector;

// Re-export AST types (ast::Measurement and ast::Unit)
pub use ast::*;

// Re-export lexer types, but be specific about units to avoid ambiguity
pub use lexer::{LexError, Lexer, Span, SpannedToken, Token};

// Re-export core unit types (only the 4 essential ones)
pub use lexer::units::{CurrentUnit, DistanceUnit, TemperatureUnit, VoltageUnit};

pub use parser::{ParseError, Parser};
