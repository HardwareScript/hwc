//! Token types for Hardware Script language
//!
//! Based on v0.1.6 syntax unification specification.
//! See `grammar/hardware.grammar` for complete syntax rules.
//!
//! # Module Organization
//!
//! - `token_types`: Core Token enum with all variants
//! - `interpolation`: Interpolated identifier parsing (e.g., `L1_R{row}_C{col}`)
//! - `number_parsers`: Integer literal parsing (decimal, hex, binary, octal)
//! - `display`: Human-friendly token display for error messages
//!
//! # Example
//!
//! ```rust
//! use hwc_parser::lexer::Token;
//! use logos::Logos;
//!
//! let mut lexer = Token::lexer("add component Resistor");
//! assert_eq!(lexer.next(), Some(Ok(Token::Add)));
//! assert_eq!(lexer.next(), Some(Ok(Token::Component)));
//! ```

mod display;
mod interpolation;
mod number_parsers;
mod token_types;

// Re-export all public types
pub use interpolation::InterpolatedPart;

pub use token_types::Token;
