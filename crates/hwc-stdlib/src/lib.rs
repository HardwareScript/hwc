//! Hardware Script Standard Library
//!
//! This crate provides the standard library for Hardware Script, including:
//! - Unit definitions (capacitance, inductance, frequency, etc.)
//! - Unit registry for validation and conversion
//! - Standard library file loading

mod loader;
mod registry;

pub use loader::StdlibLoader;
pub use registry::UnitRegistry;

use hwc_parser::UnitDefinition;
use std::path::PathBuf;

/// Standard library search paths (in priority order)
pub fn stdlib_search_paths() -> Vec<PathBuf> {
    vec![
        // 1. Project-local override (highest priority)
        PathBuf::from("./stdlib/primitives/units.hw"),
        // 2. User customization
        dirs::home_dir()
            .map(|h| h.join(".hw/stdlib/primitives/units.hw"))
            .unwrap_or_else(|| PathBuf::from("~/.hw/stdlib/primitives/units.hw")),
        // 3. Installed default (ships with compiler)
        // This will be set at compile time based on installation directory
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("stdlib/primitives/units.hw"))
            .unwrap_or_else(|| PathBuf::from("stdlib/primitives/units.hw")),
    ]
}

/// Load the standard library units
pub fn load_stdlib() -> Result<Vec<UnitDefinition>, StdlibError> {
    let loader = StdlibLoader::new();
    loader.load()
}

/// Standard library errors
#[derive(Debug, Clone)]
pub enum StdlibError {
    FileNotFound(Vec<PathBuf>),
    ParseError(String),
    InvalidUnit(String),
}

impl std::fmt::Display for StdlibError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::FileNotFound(paths) => {
                writeln!(
                    f,
                    "Standard library primitives/units.hw not found. Searched:"
                )?;
                for path in paths {
                    writeln!(f, "  - {}", path.display())?;
                }
                Ok(())
            }
            Self::ParseError(msg) => write!(f, "Failed to parse primitives/units.hw: {}", msg),
            Self::InvalidUnit(msg) => write!(f, "Invalid unit definition: {}", msg),
        }
    }
}

impl std::error::Error for StdlibError {}
