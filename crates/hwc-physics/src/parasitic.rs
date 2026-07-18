//! Parasitic extraction (RCX) for Hardware Script physics validation.
//!
//! This module provides parasitic extraction for trace segments:
//! - Trace resistance calculation using length × resistivity
//! - Trace capacitance calculation using parallel-plate model with GND planes
//! - Surface area calculation from trace dimensions (rectangular prism)
//! - Conversion of overlap area to capacitance (pF = area × dielectric constant)
//!
//! # Architecture
//!
//! The parasitic extractor bridges routing and simulation by extracting
//! "hidden" R and C components from physical traces. This enables accurate
//! signal integrity analysis without full SPICE simulation.
//!
//! # Performance Target
//!
//! Extract parasitics for large designs in < 10ms using O(1) per chunk operations.

use crate::property_extraction::{extract_relative_permittivity, extract_resistivity};
use crate::PropertyError;
use hwc_parser::MaterialDefinition;

/// Trait for accessing material definitions from Symbol Table.
pub trait SymbolTableTrait {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String>;
}

/// Extracted parasitic values for a trace segment.
#[derive(Debug, Clone)]
pub struct ParasiticValues {
    /// Trace resistance in ohms
    pub resistance_ohm: f64,
    /// Trace capacitance to ground in picofarads
    pub capacitance_pf: f64,
    /// Trace length in nanometers
    pub length_nm: i64,
    /// Copper surface area in square nanometers
    pub surface_area_nm2: i64,
}

/// Parameters for parasitic extraction.
#[derive(Debug, Clone)]
pub struct ParasiticExtractionParams<'a> {
    /// Trace length in nanometers
    pub length_nm: i64,
    /// Trace width in nanometers
    pub width_nm: i64,
    /// Trace thickness in nanometers
    pub thickness_nm: i64,
    /// Distance to ground plane in nanometers
    pub dielectric_thickness_nm: i64,
    /// Conductor material name (e.g., "Copper")
    pub conductor_material_name: &'a str,
    /// Dielectric material name (e.g., "FR4")
    pub dielectric_material_name: &'a str,
}

/// Parasitic extractor for RCX analysis.
///
/// Uses bitwise operations and dilation for fast parasitic extraction.
#[derive(Default)]
pub struct ParasiticExtractor {
    // Extractor state
}

impl ParasiticExtractor {
    pub fn new() -> Self {
        Self {}
    }

    /// Extract parasitics for merged regions (Sprint 3.2 enhancement).
    ///
    /// When multiple pours are merged (e.g., multi-finger transistor source/drain),
    /// they should be treated as a single electrical node for parasitic extraction.
    /// This function combines the areas and calculates parasitics for the merged region.
    ///
    /// # Arguments
    /// * `merged_pour_areas` - Vector of area values (nm²) for each pour in the merged region
    /// * `params` - Base parasitic extraction parameters (length, thickness, etc.)
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Combined parasitic values for the entire merged region
    ///
    /// # Performance
    /// O(n) where n is the number of merged pours (typically small, e.g., 3-4 fingers)
    pub fn extract_merged_region_parasitics(
        &self,
        merged_pour_areas: &[i64],
        base_params: &ParasiticExtractionParams,
        symbol_table: &dyn SymbolTableTrait,
    ) -> Result<ParasiticValues, PropertyError> {
        if merged_pour_areas.is_empty() {
            return Err(PropertyError::MissingProperty {
                material: "merged_region".into(),
                property: "no pours in merged region".into(),
            });
        }

        // Calculate total surface area by summing all merged pour areas
        let total_area_nm2: i64 = merged_pour_areas.iter().sum();

        // For resistance, we need to consider the merged geometry
        // In a multi-finger transistor, fingers are in parallel, so resistance decreases
        // R_total = R_single / n (parallel resistors)
        let num_fingers = merged_pour_areas.len();

        // Calculate single finger resistance
        let single_resistance = self.extract_trace_resistance(
            base_params.length_nm,
            base_params.width_nm,
            base_params.thickness_nm,
            base_params.conductor_material_name,
            symbol_table,
        )?;

        // Parallel combination: R_total = R / n
        let resistance_ohm = single_resistance / num_fingers as f64;

        // For capacitance, use the total combined surface area
        let capacitance_pf = self.extract_trace_capacitance(
            total_area_nm2,
            base_params.dielectric_thickness_nm,
            base_params.dielectric_material_name,
            symbol_table,
        )?;

        Ok(ParasiticValues {
            resistance_ohm,
            capacitance_pf,
            length_nm: base_params.length_nm,
            surface_area_nm2: total_area_nm2,
        })
    }

    /// Extract trace resistance using R = ρ × (L / A)
    ///
    /// # Arguments
    /// * `length_nm` - Trace length in nanometers
    /// * `width_nm` - Trace width in nanometers
    /// * `thickness_nm` - Trace thickness in nanometers
    /// * `material_name` - Material name to look up in Symbol Table
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Resistance in ohms
    ///
    /// # Performance
    /// O(1) - simple arithmetic calculation
    pub fn extract_trace_resistance(
        &self,
        length_nm: i64,
        width_nm: i64,
        thickness_nm: i64,
        material_name: &str,
        symbol_table: &dyn SymbolTableTrait,
    ) -> Result<f64, PropertyError> {
        // Load material from Symbol Table
        let material_def = symbol_table.get_material(material_name).map_err(|e| {
            PropertyError::MissingProperty {
                material: material_name.to_string().into(),
                property: format!("material lookup failed: {}", e),
            }
        })?;

        // Extract resistivity using property extraction helper
        let resistivity = extract_resistivity(material_def)?;

        // Calculate resistance: R = ρ × (L / A)
        let length_m = length_nm as f64 / 1_000_000_000.0;
        let width_m = width_nm as f64 / 1_000_000_000.0;
        let thickness_m = thickness_nm as f64 / 1_000_000_000.0;
        let area_m2 = width_m * thickness_m;

        Ok(resistivity * (length_m / area_m2))
    }

    /// Extract trace capacitance using parallel-plate model with GND planes.
    ///
    /// Uses surface area from trace dimensions and converts to capacitance
    /// using the dielectric constant.
    ///
    /// # Arguments
    /// * `surface_area_nm2` - Copper surface area in square nanometers
    /// * `dielectric_thickness_nm` - Distance to ground plane in nanometers
    /// * `dielectric_material_name` - Dielectric material name (e.g., "FR4")
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Capacitance in picofarads
    ///
    /// # Formula
    /// C = ε₀ × εᵣ × (A / d)
    /// where:
    /// - ε₀ = 8.854e-12 F/m (permittivity of free space)
    /// - εᵣ = relative permittivity (from material database)
    /// - A = surface area (m²)
    /// - d = dielectric thickness (m)
    ///
    /// # Performance
    /// O(1) - simple arithmetic calculation after bit-counting
    pub fn extract_trace_capacitance(
        &self,
        surface_area_nm2: i64,
        dielectric_thickness_nm: i64,
        dielectric_material_name: &str,
        symbol_table: &dyn SymbolTableTrait,
    ) -> Result<f64, PropertyError> {
        // Load dielectric material from Symbol Table
        let material_def = symbol_table
            .get_material(dielectric_material_name)
            .map_err(|e| PropertyError::MissingProperty {
                material: dielectric_material_name.to_string().into(),
                property: format!("material lookup failed: {}", e),
            })?;

        // Extract relative permittivity
        let epsilon_r = extract_relative_permittivity(material_def)?;

        // Permittivity of free space (F/m)
        const EPSILON_0: f64 = 8.854e-12;

        // Convert to meters
        // 1 nm = 1e-9 m, so 1 nm² = 1e-18 m²
        let area_m2 = surface_area_nm2 as f64 * 1e-18; // nm² to m²
        let thickness_m = dielectric_thickness_nm as f64 * 1e-9; // nm to m

        // Calculate capacitance: C = ε₀ × εᵣ × (A / d)
        let capacitance_f = EPSILON_0 * epsilon_r * (area_m2 / thickness_m);

        // Convert to picofarads (1 pF = 1e-12 F)
        Ok(capacitance_f * 1e12)
    }

    /// Calculate copper surface area from trace dimensions.
    ///
    /// Uses the formula for a rectangular prism:
    ///   A = 2 * (L*W + L*T + W*T)
    ///
    /// # Arguments
    /// * `length_nm` - Trace length in nanometers
    /// * `width_nm` - Trace width in nanometers
    /// * `thickness_nm` - Trace thickness in nanometers
    ///
    /// # Returns
    /// Surface area in square nanometers
    ///
    /// # Performance
    /// O(1) - simple arithmetic
    #[inline]
    pub fn calculate_surface_area(length_nm: i64, width_nm: i64, thickness_nm: i64) -> i64 {
        2 * (length_nm * width_nm + length_nm * thickness_nm + width_nm * thickness_nm)
    }

    /// Extract complete parasitic values for a trace segment.
    ///
    /// This is the high-level API that combines resistance and capacitance extraction.
    ///
    /// # Arguments
    /// * `params` - Parasitic extraction parameters
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Complete parasitic values (R and C)
    ///
    /// # Performance
    /// O(1) - combines two O(1) operations
    pub fn extract_parasitics(
        &self,
        params: &ParasiticExtractionParams,
        symbol_table: &dyn SymbolTableTrait,
    ) -> Result<ParasiticValues, PropertyError> {
        // Extract resistance
        let resistance_ohm = self.extract_trace_resistance(
            params.length_nm,
            params.width_nm,
            params.thickness_nm,
            params.conductor_material_name,
            symbol_table,
        )?;

        // Calculate surface area from trace dimensions
        let surface_area_nm2 =
            Self::calculate_surface_area(params.length_nm, params.width_nm, params.thickness_nm);

        // Extract capacitance
        let capacitance_pf = self.extract_trace_capacitance(
            surface_area_nm2,
            params.dielectric_thickness_nm,
            params.dielectric_material_name,
            symbol_table,
        )?;

        Ok(ParasiticValues {
            resistance_ohm,
            capacitance_pf,
            length_nm: params.length_nm,
            surface_area_nm2,
        })
    }
}
