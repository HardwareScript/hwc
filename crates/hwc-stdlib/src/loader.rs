//! Standard library file loader

use crate::{stdlib_search_paths, StdlibError};
use hwc_parser::{Lexer, Parser, Program};
use hwc_types::UnitInfo;
use std::fs;
use std::path::PathBuf;

/// Loads the standard library files
pub struct StdlibLoader {
    search_paths: Vec<PathBuf>,
}

impl StdlibLoader {
    pub fn new() -> Self {
        Self {
            search_paths: stdlib_search_paths(),
        }
    }

    /// Load the standard library AST program
    pub fn load_program(&self) -> Result<Program, StdlibError> {
        for path in &self.search_paths {
            if path.exists() {
                return self.load_from_path(path);
            }
        }
        Err(StdlibError::FileNotFound(self.search_paths.clone()))
    }

    /// Load program from a specific path
    pub fn load_from_path(&self, path: &PathBuf) -> Result<Program, StdlibError> {
        let source = fs::read_to_string(path).map_err(|e| {
            StdlibError::ParseError(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let lexer = Lexer::new(&source);
        let tokens = lexer
            .tokenize()
            .map_err(|e| StdlibError::ParseError(format!("Tokenization failed: {:?}", e)))?;

        let collector =
            hwc_parser::DiagnosticCollector::new_with_file(&source, &path.to_string_lossy(), 20);
        let mut parser = Parser::new(tokens);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            return Err(StdlibError::ParseError(format!(
                "Parse failed: {}",
                collector.summary()
            )));
        }

        Ok(program)
    }

    /// Get default unit definitions
    pub fn load(&self) -> Result<Vec<UnitInfo>, StdlibError> {
        Ok(vec![
            UnitInfo {
                symbol: "pm".into(),
                aliases: vec!["picometer".into(), "picometers".into()],
                multiplier: Some(1e-12),
                dimension: "distance".into(),
            },
            UnitInfo {
                symbol: "nm".into(),
                aliases: vec!["nanometer".into(), "nanometers".into()],
                multiplier: Some(1e-9),
                dimension: "distance".into(),
            },
            UnitInfo {
                symbol: "um".into(),
                aliases: vec!["µm".into(), "micrometer".into(), "micrometers".into()],
                multiplier: Some(1e-6),
                dimension: "distance".into(),
            },
            UnitInfo {
                symbol: "mm".into(),
                aliases: vec!["millimeter".into(), "millimeters".into()],
                multiplier: Some(1e-3),
                dimension: "distance".into(),
            },
            UnitInfo {
                symbol: "cm".into(),
                aliases: vec!["centimeter".into(), "centimeters".into()],
                multiplier: Some(1e-2),
                dimension: "distance".into(),
            },
            UnitInfo {
                symbol: "m".into(),
                aliases: vec!["meter".into(), "meters".into()],
                multiplier: Some(1.0),
                dimension: "distance".into(),
            },
            UnitInfo {
                symbol: "V".into(),
                aliases: vec!["volt".into(), "volts".into()],
                multiplier: Some(1.0),
                dimension: "voltage".into(),
            },
            UnitInfo {
                symbol: "mV".into(),
                aliases: vec!["millivolt".into(), "millivolts".into()],
                multiplier: Some(1e-3),
                dimension: "voltage".into(),
            },
            UnitInfo {
                symbol: "kV".into(),
                aliases: vec!["kilovolt".into(), "kilovolts".into()],
                multiplier: Some(1e3),
                dimension: "voltage".into(),
            },
            UnitInfo {
                symbol: "A".into(),
                aliases: vec!["amp".into(), "ampere".into(), "amperes".into()],
                multiplier: Some(1.0),
                dimension: "current".into(),
            },
            UnitInfo {
                symbol: "mA".into(),
                aliases: vec!["milliamp".into(), "milliampere".into()],
                multiplier: Some(1e-3),
                dimension: "current".into(),
            },
            UnitInfo {
                symbol: "uA".into(),
                aliases: vec!["µA".into(), "microamp".into(), "microampere".into()],
                multiplier: Some(1e-6),
                dimension: "current".into(),
            },
        ])
    }

    /// Get the path that would be used (for debugging)
    pub fn find_stdlib_path(&self) -> Option<PathBuf> {
        self.search_paths.iter().find(|p| p.exists()).cloned()
    }
}

impl Default for StdlibLoader {
    fn default() -> Self {
        Self::new()
    }
}
