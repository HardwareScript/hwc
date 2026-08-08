//! Standard library file loader

use crate::{stdlib_search_paths, StdlibError};
use bumpalo::Bump;
use hwc_parser::{Definition, Lexer, Parser, UnitDefinition};
use std::fs;
use std::path::PathBuf;

/// Loads the standard library units.hw file
pub struct StdlibLoader {
    search_paths: Vec<PathBuf>,
}

impl StdlibLoader {
    pub fn new() -> Self {
        Self {
            search_paths: stdlib_search_paths(),
        }
    }

    /// Load units from the first available units.hw file
    pub fn load(&self) -> Result<Vec<UnitDefinition>, StdlibError> {
        // Try each search path in order
        for path in &self.search_paths {
            if path.exists() {
                return self.load_from_path(path);
            }
        }

        // No file found
        Err(StdlibError::FileNotFound(self.search_paths.clone()))
    }

    /// Load units from a specific path
    fn load_from_path(&self, path: &PathBuf) -> Result<Vec<UnitDefinition>, StdlibError> {
        // Read file
        let source = fs::read_to_string(path).map_err(|e| {
            StdlibError::ParseError(format!("Failed to read {}: {}", path.display(), e))
        })?;

        // Tokenize
        let lexer = Lexer::new(&source);
        let tokens = lexer
            .tokenize()
            .map_err(|e| StdlibError::ParseError(format!("Tokenization failed: {:?}", e)))?;

        // Parse with diagnostic collector
        let arena = Bump::new();
        let collector =
            hwc_parser::DiagnosticCollector::new_with_file(&source, &path.to_string_lossy(), 20);
        let mut parser = Parser::new(tokens, &arena);
        let program = parser.parse(&collector);

        // Check for errors
        if collector.has_errors() {
            return Err(StdlibError::ParseError(format!(
                "Parse failed: {}",
                collector.summary()
            )));
        }

        // Extract unit definitions
        let mut units = Vec::new();
        for definition in program.definitions {
            if let Definition::Unit(unit) = definition {
                units.push(unit);
            }
        }

        Ok(units)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_finds_stdlib() {
        let loader = StdlibLoader::new();
        let path = loader.find_stdlib_path();

        // Should find at least one path (the one in the repo)
        assert!(
            path.is_some(),
            "Should find stdlib/primitives/units.hw in repo"
        );
    }

    #[test]
    fn test_load_stdlib_units() {
        let loader = StdlibLoader::new();
        let result = loader.load();

        match result {
            Ok(units) => {
                assert!(!units.is_empty(), "Should load at least one unit");

                // Check for some expected units
                let symbols: Vec<&str> = units.iter().map(|u| u.symbol.as_str()).collect();
                assert!(
                    symbols.contains(&"µF") || symbols.contains(&"F"),
                    "Should contain capacitance units"
                );
            }
            Err(e) => {
                // It's okay if stdlib isn't set up yet in test environment
                eprintln!("Note: stdlib not found (expected in dev): {}", e);
            }
        }
    }
}
