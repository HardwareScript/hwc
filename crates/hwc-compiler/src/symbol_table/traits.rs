//! Trait implementations for external crate integration

use super::layer::SymbolTable;
use hwc_parser::{MaterialDecl, ProfileDecl};

// ========== hwc-engine trait implementations ==========

impl hwc_engine::SymbolTableTrait for SymbolTable {
    fn get_material(&self, name: &str) -> Result<&MaterialDecl, String> {
        self.get_material(name).map_err(|e| e.to_string())
    }

    fn get_profile(&self, name: &str) -> Result<&ProfileDecl, String> {
        self.get_profile(name).map_err(|e| e.to_string())
    }

    fn measurement_to_nm(&self, measurement: &hwc_parser::Measurement) -> Result<i64, String> {
        measurement.unit.to_nanometers(measurement.value)
    }
}

// ========== hwc-physics trait implementations ==========

impl hwc_physics::electrical::SymbolTableTrait for SymbolTable {
    fn get_material(&self, name: &str) -> Result<&MaterialDecl, String> {
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

impl hwc_physics::thermal::SymbolTableTrait for SymbolTable {
    fn get_material(&self, name: &str) -> Result<&MaterialDecl, String> {
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

impl hwc_physics::electromagnetic::SymbolTableTrait for SymbolTable {
    fn get_material(&self, name: &str) -> Result<&MaterialDecl, String> {
        self.get_material(name).map_err(|e| e.to_string())
    }
}

impl hwc_physics::clearance::SymbolTableTrait for SymbolTable {
    fn get_material(&self, name: &str) -> Result<&MaterialDecl, String> {
        self.get_material(name).map_err(|e| e.to_string())
    }
}

// ========== Helper functions for constraint extraction ==========

/// Extract electrical constraints from a profile definition
fn extract_electrical_constraints(
    profile: &ProfileDecl,
) -> hwc_physics::electrical::ProfileConstraints {
    let mut constraints = hwc_physics::electrical::ProfileConstraints::default();

    if let Some(thermal) = profile.sections.iter().find(|s| s.section_type == "thermal") {
        for (k, expr) in &thermal.fields {
            if let hwc_parser::Expression::Measurement { value, .. } = expr {
                match k.as_str() {
                    "ambient_temp" => constraints.ambient_temp_c = *value,
                    "max_operating_temp" => constraints.max_operating_temp_c = Some(*value),
                    "max_temp_rise" => constraints.max_temp_rise_c = Some(*value),
                    _ => {}
                }
            }
        }
    }

    constraints
}

/// Extract thermal constraints from a profile definition
fn extract_thermal_constraints(
    profile: &ProfileDecl,
) -> hwc_physics::thermal::ProfileConstraints {
    let mut constraints = hwc_physics::thermal::ProfileConstraints::default();

    if let Some(thermal) = profile.sections.iter().find(|s| s.section_type == "thermal") {
        for (k, expr) in &thermal.fields {
            if let hwc_parser::Expression::Measurement { value, .. } = expr {
                match k.as_str() {
                    "ambient_temp" => constraints.ambient_temp_c = *value,
                    "max_operating_temp" => constraints.max_operating_temp_c = Some(*value),
                    "max_temp_rise" => constraints.max_temp_rise_c = Some(*value),
                    _ => {}
                }
            }
        }
    }

    constraints
}
