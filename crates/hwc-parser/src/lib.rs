// SPDX-License-Identifier: AGPL-3.0-or-later WITH HardwareScript-Compiler-Output-Exception
// Copyright (C) 2024-2026 Olowookere Olamide and HardwareScript Contributors
//
// This file is part of the Hardware Script compiler (hwc).
//
// hwc is free software: you can redistribute it and/or modify it under the terms
// of the GNU Affero General Public License as published by the Free Software Foundation,
// either version 3 of the License, or (at your option) any later version, WITH the
// HardwareScript Compiler Output Exception.
//
// See LICENSE.md and COMPILER-OUTPUT-EXCEPTION.md in the repository root for details.

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
