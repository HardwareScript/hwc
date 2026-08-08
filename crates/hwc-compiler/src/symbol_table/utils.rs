//! Utility functions for symbol table operations

use super::{error::SymbolError, layer::SymbolTable};
use compact_str::CompactString;
use hwc_parser::MaterialDefinition;
use rustc_hash::FxHashMap;

impl SymbolTable {
    /// Expand a module's pin declarations into individual pin name strings.
    ///
    /// Array pins like `Bus_Out[64]` are expanded into:
    /// `["Bus_Out[0]", "Bus_Out[1]", ..., "Bus_Out[63]"]`
    ///
    /// Simple pins like `VCC` stay as `["VCC"]`.
    ///
    /// This is the canonical expansion used during module instance registration
    /// so that routes like `MainDSP.Bus_Out[0]` can be resolved.
    pub fn get_module_expanded_pins(
        &self,
        module_name: &str,
    ) -> Result<Vec<CompactString>, SymbolError> {
        let module_def = self.get_module(module_name)?;
        Ok(expand_pin_declarations(&module_def.pins))
    }

    /// Merge properties from a base material with an override material.
    ///
    /// This implements property-level shadowing (v0.1.6 Task 4.2):
    /// - Takes all properties from the base material
    /// - Replaces only the properties that are specified in the override
    /// - Keeps all other properties from the base
    ///
    /// # Arguments
    /// * `base` - The base material definition (from a lower authority layer)
    /// * `override_mat` - The override material definition (from a higher authority layer)
    ///
    /// # Returns
    /// A new MaterialDefinition with merged properties
    ///
    /// # Example
    /// ```ignore
    /// // Base material from stdlib has all 5 MPV properties
    /// // Override only specifies density
    /// // Result: merged material has 4 properties from base + 1 from override
    /// let merged = table.merge_properties(&base_copper, &local_copper);
    /// ```
    pub fn merge_properties(
        &self,
        base: &MaterialDefinition,
        override_mat: &MaterialDefinition,
    ) -> MaterialDefinition {
        // Start with a clone of the base material
        let mut merged = base.clone();

        // Update top-level fields from override (name, category, symbol, description, span)
        merged.name = override_mat.name.clone();
        merged.category = override_mat.category.clone();
        merged.symbol = override_mat.symbol.clone();
        merged.description = override_mat.description.clone();
        merged.span = override_mat.span;

        // Build a map of override properties for fast lookup
        let override_props: FxHashMap<&str, &hwc_parser::Property> = override_mat
            .properties
            .iter()
            .map(|prop| (prop.key.as_str(), prop))
            .collect();

        // Merge properties: keep base properties, but replace with override if present
        let mut merged_properties = Vec::new();

        // First, add all base properties (will be replaced if override exists)
        for base_prop in &base.properties {
            if let Some(override_prop) = override_props.get(base_prop.key.as_str()) {
                // Override exists - use it instead
                merged_properties.push((*override_prop).clone());
            } else {
                // No override - keep base property
                merged_properties.push(base_prop.clone());
            }
        }

        // Second, add any new properties from override that weren't in base
        for override_prop in &override_mat.properties {
            if !base.properties.iter().any(|p| p.key == override_prop.key) {
                merged_properties.push(override_prop.clone());
            }
        }

        merged.properties = merged_properties;
        merged
    }
}

/// Expand a list of pin declarations into individual pin name strings.
///
/// For array pins like `Bus_Out[64]`:
///   → `["Bus_Out[0]", "Bus_Out[1]", ..., "Bus_Out[63]"]`
///
/// For simple pins like `VCC`:
///   → `["VCC"]`
pub fn expand_pin_declarations(pins: &[hwc_parser::PinDeclaration]) -> Vec<CompactString> {
    let mut result = Vec::new();
    for pin in pins {
        if let Some(size) = pin.array_size {
            // Array pin: expand into Bus_Out[0], Bus_Out[1], ..., Bus_Out[size-1]
            for i in 0..size {
                result.push(format!("{}[{}]", pin.name, i).into());
            }
        } else {
            // Simple pin: keep as-is
            result.push(pin.name.clone());
        }
    }
    result
}
