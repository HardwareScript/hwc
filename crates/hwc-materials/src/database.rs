//! Material database for PCB and ASIC design
//!
//! This module provides a database of material properties for conductors,
//! insulators, and semiconductors.
//!
//! # v0.1.4 Architecture
//!
//! Materials are now defined in .hw files using `define material` blocks:
//! 1. Parser creates `MaterialDefinition` AST nodes
//! 2. Symbol Table registers materials during Pass 1
//! 3. Compiler populates `MaterialDatabase` via `populate_material_database()`
//! 4. Engine uses material properties for physics calculations
//!
//! See: `hwc-compiler/src/conversions.rs::populate_material_database()`

use crate::material::{
    ConductorProperties, InsulatorProperties, MaterialMetadata, SemiconductorProperties,
};
use compact_str::CompactString;
use miette::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum MaterialError {
    #[error("Material not found: {0}")]
    #[diagnostic(
        code(M21),
        url("https://docs.hw-script.org/errors/M21"),
        help("Define the material in your .hw file using 'define material' syntax")
    )]
    NotFound(String),

    #[error("Failed to parse material definition: {0}")]
    #[diagnostic(
        code(M31),
        url("https://docs.hw-script.org/errors/M31"),
        help("Verify material syntax matches LANGUAGE-SPEC.md")
    )]
    ParseError(String),

    #[error("Failed to read file: {0}")]
    #[diagnostic(
        code(M32),
        url("https://docs.hw-script.org/errors/M32"),
        help("Verify file path exists and has read permissions")
    )]
    IoError(#[from] std::io::Error),

    #[error("Invalid material type for operation: {0}")]
    #[diagnostic(
        code(M11),
        url("https://docs.hw-script.org/errors/M11"),
        help("Material type must match the operation (conductor, insulator, or semiconductor)")
    )]
    InvalidType(String),
}

/// Complete material database with 3-tier hierarchy
#[derive(Debug, Clone, Default)]
pub struct MaterialDatabase {
    pub conductors: FxHashMap<CompactString, ConductorProperties>,
    pub insulators: FxHashMap<CompactString, InsulatorProperties>,
    pub semiconductors: FxHashMap<CompactString, SemiconductorProperties>,
    pub metadata: MaterialMetadata,
}

impl MaterialDatabase {
    /// Create an empty material database
    ///
    /// Used for testing or when no materials are defined.
    /// In production, use `populate_material_database()` from the compiler.
    pub fn empty() -> Self {
        Self {
            conductors: FxHashMap::default(),
            insulators: FxHashMap::default(),
            semiconductors: FxHashMap::default(),
            metadata: MaterialMetadata::default(),
        }
    }

    /// Create database from material definitions (v0.1.4)
    ///
    /// This is the primary way to create a material database in v0.1.4.
    /// Use `hwc_compiler::populate_material_database()` instead of calling this directly.
    ///
    /// # Example
    /// Use the compiler's populate_material_database function with the Symbol Table.
    pub fn from_definitions() -> Result<Self, MaterialError> {
        Err(MaterialError::ParseError(
            "Use hwc_compiler::populate_material_database() with Symbol Table.".into(),
        ))
    }

    /// Get conductor by name
    pub fn get_conductor(&self, name: &str) -> Result<&ConductorProperties, MaterialError> {
        self.conductors
            .get(name)
            .ok_or_else(|| MaterialError::NotFound(format!("Conductor '{}' not found", name)))
    }

    /// Get insulator by name
    pub fn get_insulator(&self, name: &str) -> Result<&InsulatorProperties, MaterialError> {
        self.insulators
            .get(name)
            .ok_or_else(|| MaterialError::NotFound(format!("Insulator '{}' not found", name)))
    }

    /// Get semiconductor by name
    pub fn get_semiconductor(&self, name: &str) -> Result<&SemiconductorProperties, MaterialError> {
        self.semiconductors
            .get(name)
            .ok_or_else(|| MaterialError::NotFound(format!("Semiconductor '{}' not found", name)))
    }

    /// Check if material exists (any type)
    pub fn has_material(&self, name: &str) -> bool {
        self.conductors.contains_key(name)
            || self.insulators.contains_key(name)
            || self.semiconductors.contains_key(name)
    }

    /// Check if conductor exists
    pub fn has_conductor(&self, name: &str) -> bool {
        self.conductors.contains_key(name)
    }

    /// Check if insulator exists
    pub fn has_insulator(&self, name: &str) -> bool {
        self.insulators.contains_key(name)
    }

    /// Check if semiconductor exists
    pub fn has_semiconductor(&self, name: &str) -> bool {
        self.semiconductors.contains_key(name)
    }

    /// Merge another database into this one (override priority)
    pub fn merge(&mut self, other: MaterialDatabase) {
        self.conductors.extend(other.conductors);
        self.insulators.extend(other.insulators);
        self.semiconductors.extend(other.semiconductors);
    }

    /// Export database to .hw file format (future feature)
    ///
    /// Currently not implemented. Materials are defined directly in .hw files.
    pub fn export_to_file(&self, _path: &Path) -> Result<(), MaterialError> {
        Err(MaterialError::ParseError(
            "Material export not yet implemented in v0.1.4 - define materials directly in .hw files".into(),
        ))
    }

    /// Calculate clearance between two nets based on voltage and material
    pub fn calculate_clearance_nm(
        &self,
        voltage_diff_mv: i64,
        material_name: &str,
        safety_factor: i64,
    ) -> Result<i64, MaterialError> {
        let insulator = self.get_insulator(material_name)?;
        Ok(insulator.calculate_clearance_nm(voltage_diff_mv, safety_factor))
    }

    /// Calculate trace width for given current
    pub fn calculate_trace_width_nm(
        &self,
        conductor_name: &str,
        current_ma: i64,
        temp_rise_c: i64,
        is_external: bool,
    ) -> Result<i64, MaterialError> {
        let conductor = self.get_conductor(conductor_name)?;
        Ok(conductor.calculate_trace_width_nm(current_ma, temp_rise_c, is_external))
    }
}
