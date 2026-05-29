//! Hardware Script Lexer (v0.1.4)
//!
//! This module implements the lexer (tokenizer) for Hardware Script using the Logos crate.
//!
//! **v0.1.4 Changes**:
//! - Native SI unit parsing: `254µm` tokenizes as `Measurement(254.0, Micrometers)`
//! - Added `define` block keywords: material, profile, component, mechanical, interface, test
//! - Removed separate unit tokens - measurements are now atomic
//!
//! ## Features
//!
//! - Logos-based token generation (fast, compile-time regex)
//! - Native SI unit measurements (e.g., `4.7kΩ`, `100nF`, `254µm`)
//! - Indentation tracking (INDENT/DEDENT tokens like Python)
//! - Unicode support (Ω, µ, °)
//! - Span tracking for error reporting

mod error;
mod parsers;
mod span;
mod token;
mod tokenizer;
pub mod units;

#[cfg(test)]
mod tests;

// Re-export public API
pub use error::{span_to_source_span, LexError};
pub use span::{Span, SpannedToken};
pub use token::Token;
pub use tokenizer::Lexer;
pub use units::*;
