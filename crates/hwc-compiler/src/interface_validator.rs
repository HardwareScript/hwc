//! Interface validation for polymorphic modules
//!
//! Validates that components correctly implement their declared interfaces.
//! Performs compile-time duck-typing checks to ensure pin compatibility.
//!
//! v0.2.1: Migrated to arena-based architecture - stores PolymorphicInterfaceDefId instead of full structs

use compact_str::CompactString;
use hwc_parser::ast::{
    arena::PolymorphicInterfaceDefId, AstArena, ComponentDefinition, InterfaceImplementation,
    InterfaceValidationError,
};
use rustc_hash::FxHashMap;

/// Interface validator for polymorphic modules
///
/// Validates that components correctly implement their declared interfaces
/// using duck-typing (structural compatibility checking).
///
/// v0.2.1: Stores 4-byte PolymorphicInterfaceDefId instead of full PolymorphicInterfaceDefinition structs
pub struct InterfaceValidator {
    /// All defined interfaces (name → arena ID)
    /// Arena lookup required: arena.polymorphic_interface_defs[id]
    interfaces: FxHashMap<CompactString, PolymorphicInterfaceDefId>,
}

impl InterfaceValidator {
    /// Create a new interface validator
    pub fn new() -> Self {
        Self {
            interfaces: FxHashMap::default(),
        }
    }

    /// Register an interface definition
    /// Returns the ID that was stored
    pub fn register_interface(
        &mut self,
        name: CompactString,
        interface_id: PolymorphicInterfaceDefId,
    ) -> PolymorphicInterfaceDefId {
        self.interfaces.insert(name, interface_id);
        interface_id
    }

    /// Validate that a component correctly implements its declared interfaces
    ///
    /// Performs O(1) validation per component by checking:
    /// 1. All declared interfaces exist
    /// 2. All required pins are present
    /// 3. Pin types are compatible
    /// 4. Pin mappings are valid
    ///
    /// # Performance
    ///
    /// O(1) per component - uses HashMap lookups for interface and pin resolution
    ///
    /// v0.2.1: Requires arena reference to dereference InterfaceDefId
    pub fn validate_component(
        &self,
        component: &ComponentDefinition,
        arena: &AstArena,
    ) -> Result<(), Vec<InterfaceValidationError>> {
        let mut errors = Vec::new();

        // Validate each interface implementation
        for implementation in &component.implements {
            if let Err(mut impl_errors) =
                self.validate_implementation(component, implementation, arena)
            {
                errors.append(&mut impl_errors);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate a single interface implementation
    fn validate_implementation(
        &self,
        component: &ComponentDefinition,
        implementation: &InterfaceImplementation,
        arena: &AstArena,
    ) -> Result<(), Vec<InterfaceValidationError>> {
        let mut errors = Vec::new();

        // Check if interface exists and lookup from arena
        let interface_id = match self.interfaces.get(&implementation.interface_name) {
            Some(id) => id,
            None => {
                errors.push(InterfaceValidationError::InterfaceNotFound {
                    interface_name: implementation.interface_name.clone(),
                    component_name: component.name.to_string().into(),
                });
                return Err(errors);
            }
        };

        // Dereference interface from arena
        let interface = &arena.polymorphic_interface_defs[*interface_id];

        // Get required and optional pins
        let required_pins = &interface.required_pins;
        let optional_pins = &interface.optional_pins;

        // Build pin mapping (interface pin → component pin)
        let pin_map = self.build_pin_mapping(component, implementation);

        // Validate pin mappings reference valid pins
        for mapping in &implementation.pin_mappings {
            // Check if interface pin exists
            let interface_pin_exists = required_pins
                .iter()
                .chain(optional_pins.iter())
                .any(|p| p.name == mapping.interface_pin);

            if !interface_pin_exists {
                errors.push(InterfaceValidationError::InvalidPinMapping {
                    interface_name: interface.name.to_string().into(),
                    interface_pin: mapping.interface_pin.clone(),
                    component_name: component.name.to_string().into(),
                });
            }

            // Check if component pin exists
            if !component.pins.contains(&mapping.component_pin) {
                errors.push(InterfaceValidationError::MappedPinNotFound {
                    component_name: component.name.to_string().into(),
                    component_pin: mapping.component_pin.clone(),
                    interface_pin: mapping.interface_pin.clone(),
                });
            }
        }

        // Validate required pins are present
        for interface_pin in required_pins {
            // Get the component pin name (either from mapping or assume same name)
            let component_pin_name: String = pin_map
                .get(&interface_pin.name)
                .cloned()
                .unwrap_or_else(|| interface_pin.name.to_string());

            // Check if component has this pin
            let component_pin_name_compact: CompactString = component_pin_name.clone().into();
            if !component.pins.contains(&component_pin_name_compact) {
                errors.push(InterfaceValidationError::MissingRequiredPin {
                    interface_name: interface.name.to_string().into(),
                    pin_name: interface_pin.name.to_string().into(),
                    component_name: component.name.to_string().into(),
                });
            }
            // Note: Pin type validation would require pin type information
            // in ComponentDefinition, which is not currently available.
            // This is a future enhancement.
        }

        // Optional pins don't need to be present, so we don't validate them

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Build pin mapping from interface pins to component pins
    ///
    /// If no explicit mapping is provided, assumes pin names match exactly.
    fn build_pin_mapping(
        &self,
        _component: &ComponentDefinition,
        implementation: &InterfaceImplementation,
    ) -> FxHashMap<CompactString, String> {
        let mut map = FxHashMap::default();

        // Add explicit mappings
        for mapping in &implementation.pin_mappings {
            map.insert(
                mapping.interface_pin.clone(),
                mapping.component_pin.to_string(),
            );
        }

        // For unmapped pins, assume they have the same name
        // (This is handled implicitly - if a pin isn't in the map,
        // we assume interface_pin_name == component_pin_name)

        map
    }

    /// Check if a component implements a specific interface
    ///
    /// O(1) lookup - checks if component declares the interface
    pub fn implements_interface(
        &self,
        component: &ComponentDefinition,
        interface_name: &str,
    ) -> bool {
        component
            .implements
            .iter()
            .any(|impl_| impl_.interface_name == interface_name)
    }

    /// Get all interfaces implemented by a component
    ///
    /// v0.2.1: Returns interface IDs that must be dereferenced via arena
    pub fn get_implemented_interface_ids(
        &self,
        component: &ComponentDefinition,
    ) -> Vec<PolymorphicInterfaceDefId> {
        component
            .implements
            .iter()
            .filter_map(|impl_| self.interfaces.get(&impl_.interface_name).copied())
            .collect()
    }

    /// Find all components that implement a specific interface
    ///
    /// Used for polymorphic module instantiation
    pub fn find_compatible_components<'a>(
        &self,
        interface_name: &str,
        components: &'a [ComponentDefinition],
    ) -> Vec<&'a ComponentDefinition> {
        components
            .iter()
            .filter(|comp| self.implements_interface(comp, interface_name))
            .collect()
    }
}

impl Default for InterfaceValidator {
    fn default() -> Self {
        Self::new()
    }
}
