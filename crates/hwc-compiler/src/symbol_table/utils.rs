//! Utility functions for symbol table operations

use super::{error::SymbolError, layer::SymbolTable};
use compact_str::CompactString;
use hwc_parser::{MaterialDecl, PinDecl};
use rustc_hash::FxHashMap;

impl SymbolTable {
    /// Expand a module's pin declarations into individual pin name strings.
    pub fn get_module_expanded_pins(
        &self,
        module_name: &str,
    ) -> Result<Vec<CompactString>, SymbolError> {
        let module_def = self.get_module(module_name)?;
        Ok(expand_pin_declarations(&module_def.pins))
    }

    /// Merge properties from a base material with an override material.
    pub fn merge_properties(
        &self,
        base: &MaterialDecl,
        override_mat: &MaterialDecl,
    ) -> MaterialDecl {
        let mut merged = base.clone();

        merged.name = override_mat.name.clone();
        merged.span = override_mat.span;

        let override_props: FxHashMap<&str, &hwc_parser::Expression> = override_mat
            .properties
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        let mut merged_properties = Vec::new();

        for (base_key, base_val) in &base.properties {
            if let Some(override_val) = override_props.get(base_key.as_str()) {
                merged_properties.push((base_key.clone(), (*override_val).clone()));
            } else {
                merged_properties.push((base_key.clone(), base_val.clone()));
            }
        }

        for (override_key, override_val) in &override_mat.properties {
            if !base.properties.iter().any(|(k, _)| k == override_key) {
                merged_properties.push((override_key.clone(), override_val.clone()));
            }
        }

        merged.properties = merged_properties;
        merged
    }
}

/// Expand a list of pin declarations into individual pin name strings.
pub fn expand_pin_declarations(pins: &[PinDecl]) -> Vec<CompactString> {
    pins.iter().map(|p| p.name.clone()).collect()
}
