//! Trait implementations for external crate integration

use super::layer::{SymbolLayer, SymbolTable};
use hwc_parser::{MaterialDefinition, ProfileDefinition};

// ========== Internal layered lookup trait ==========

/// Trait for extracting values from a SymbolLayer
///
/// This enables clean, type-safe lookups across the layer hierarchy
/// without repetitive or_else chains.
pub(super) trait _LayerLookup<T> {
    fn get_from_layer(&self, layer: &SymbolLayer) -> Option<&T>;
}

// ========== hwc-engine trait implementations ==========

impl hwc_engine::SymbolTableTrait for SymbolTable<'_> {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String> {
        self.get_material(name).map_err(|e| e.to_string())
    }

    fn get_profile(&self, name: &str) -> Result<&ProfileDefinition, String> {
        self.get_profile(name).map_err(|e| e.to_string())
    }

    fn measurement_to_nm(&self, measurement: &hwc_parser::Measurement) -> Result<i64, String> {
        SymbolTable::measurement_to_nm(self, measurement)
    }
}

// ========== hwc-physics trait implementations ==========

impl hwc_physics::electrical::SymbolTableTrait for SymbolTable<'_> {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String> {
        self.get_material(name).map_err(|e| e.to_string())
    }

    fn get_profile_constraints(
        &self,
        profile_name: &str,
    ) -> Result<hwc_physics::electrical::ProfileConstraints, String> {
        let profile = self.get_profile(profile_name).map_err(|e| e.to_string())?;
        Ok(extract_electrical_constraints(profile))
    }
}

impl hwc_physics::thermal::SymbolTableTrait for SymbolTable<'_> {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String> {
        self.get_material(name).map_err(|e| e.to_string())
    }

    fn get_profile_constraints(
        &self,
        profile_name: &str,
    ) -> Result<hwc_physics::thermal::ProfileConstraints, String> {
        let profile = self.get_profile(profile_name).map_err(|e| e.to_string())?;
        Ok(extract_thermal_constraints(profile))
    }
}

impl hwc_physics::electromagnetic::SymbolTableTrait for SymbolTable<'_> {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String> {
        self.get_material(name).map_err(|e| e.to_string())
    }
}

impl hwc_physics::clearance::SymbolTableTrait for SymbolTable<'_> {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String> {
        self.get_material(name).map_err(|e| e.to_string())
    }
}

// ========== Helper functions for constraint extraction ==========

/// Extract electrical constraints from a profile definition
fn extract_electrical_constraints(
    profile: &ProfileDefinition,
) -> hwc_physics::electrical::ProfileConstraints {
    let mut constraints = hwc_physics::electrical::ProfileConstraints::default();

    // Extract thermal constraints (used for ambient temp)
    if let Some(ref thermal) = profile.thermal {
        constraints.ambient_temp_c = measurement_to_celsius(&thermal.ambient_temp);
        constraints.max_operating_temp_c =
            Some(measurement_to_celsius(&thermal.max_operating_temp));
        constraints.max_temp_rise_c = Some(measurement_to_celsius(&thermal.max_temp_rise));
    }

    // TODO: Extract max_voltage_drop from profile when electrical section is added
    // For now, use default value

    constraints
}

/// Extract thermal constraints from a profile definition
fn extract_thermal_constraints(
    profile: &ProfileDefinition,
) -> hwc_physics::thermal::ProfileConstraints {
    let mut constraints = hwc_physics::thermal::ProfileConstraints::default();

    if let Some(ref thermal) = profile.thermal {
        constraints.ambient_temp_c = measurement_to_celsius(&thermal.ambient_temp);
        constraints.max_operating_temp_c =
            Some(measurement_to_celsius(&thermal.max_operating_temp));
        constraints.max_temp_rise_c = Some(measurement_to_celsius(&thermal.max_temp_rise));

        // Note: clustering_threshold conversion requires symbol table access
        // This is a limitation of the current architecture where we extract constraints
        // without symbol table context. For now, we skip this field.
        // TODO: Refactor to pass symbol table to constraint extraction functions
    }

    constraints
}

/// Convert a Measurement to Celsius
fn measurement_to_celsius(measurement: &hwc_parser::Measurement) -> f64 {
    // Measurement value is already in the base unit (Celsius for temperature)
    measurement.value
}
