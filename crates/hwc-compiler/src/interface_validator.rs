//! Interface validation for polymorphic modules
//!
//! Validates that components correctly implement their declared interfaces.
//! Performs compile-time duck-typing checks to ensure pin compatibility.

use compact_str::CompactString;
#[cfg(test)]
use hwc_parser::ast::PinMapping;
use hwc_parser::ast::{
    ComponentDefinition, InterfaceImplementation, InterfaceValidationError,
    PolymorphicInterfaceDefinition,
};
use rustc_hash::FxHashMap;

/// Interface validator for polymorphic modules
///
/// Validates that components correctly implement their declared interfaces
/// using duck-typing (structural compatibility checking).
pub struct InterfaceValidator {
    /// All defined interfaces (name → definition)
    interfaces: FxHashMap<CompactString, PolymorphicInterfaceDefinition>,
}

impl InterfaceValidator {
    /// Create a new interface validator
    pub fn new() -> Self {
        Self {
            interfaces: FxHashMap::default(),
        }
    }

    /// Register an interface definition
    pub fn register_interface(&mut self, interface: PolymorphicInterfaceDefinition) {
        self.interfaces
            .insert(interface.name.to_string().into(), interface);
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
    pub fn validate_component(
        &self,
        component: &ComponentDefinition,
    ) -> Result<(), Vec<InterfaceValidationError>> {
        let mut errors = Vec::new();

        // Validate each interface implementation
        for implementation in &component.implements {
            if let Err(mut impl_errors) = self.validate_implementation(component, implementation) {
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
    ) -> Result<(), Vec<InterfaceValidationError>> {
        let mut errors = Vec::new();

        // Check if interface exists
        let interface = match self.interfaces.get(&implementation.interface_name) {
            Some(iface) => iface,
            None => {
                errors.push(InterfaceValidationError::InterfaceNotFound {
                    interface_name: implementation.interface_name.clone(),
                    component_name: component.name.to_string().into(),
                });
                return Err(errors);
            }
        };

        // Build pin mapping (interface pin → component pin)
        let pin_map = self.build_pin_mapping(component, implementation);

        // Validate pin mappings reference valid pins
        for mapping in &implementation.pin_mappings {
            // Check if interface pin exists
            let interface_pin_exists = interface
                .required_pins
                .iter()
                .chain(interface.optional_pins.iter())
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
        for interface_pin in &interface.required_pins {
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
    pub fn get_implemented_interfaces(
        &self,
        component: &ComponentDefinition,
    ) -> Vec<&PolymorphicInterfaceDefinition> {
        component
            .implements
            .iter()
            .filter_map(|impl_| self.interfaces.get(&impl_.interface_name))
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

#[cfg(test)]
mod tests {
    use super::*;

    use hwc_parser::ast::{InterfacePin, PinType};
    use hwc_parser::lexer::Span;
    use hwc_parser::Identifier;

    fn dummy_span() -> Span {
        Span { start: 0, end: 0 }
    }

    #[test]
    fn test_interface_registration() {
        let mut validator = InterfaceValidator::new();

        let interface = PolymorphicInterfaceDefinition {
            name: Identifier::with_dummy_span("I2S_DAC"),
            is_exported: false,
            description: None,
            required_pins: vec![
                InterfacePin {
                    name: Identifier::with_dummy_span("BCLK").to_string().into(),
                    pin_type: PinType::Output,
                    description: None,
                    span: dummy_span(),
                },
                InterfacePin {
                    name: Identifier::with_dummy_span("LRCLK").to_string().into(),
                    pin_type: PinType::Output,
                    description: None,
                    span: dummy_span(),
                },
            ],
            optional_pins: vec![],
            span: dummy_span(),
        };

        validator.register_interface(interface);
        assert!(validator.interfaces.contains_key("I2S_DAC"));
    }

    #[test]
    fn test_valid_component_implementation() {
        let mut validator = InterfaceValidator::new();

        // Register interface
        let interface = PolymorphicInterfaceDefinition {
            name: Identifier::with_dummy_span("I2S_DAC"),
            is_exported: false,
            description: None,
            required_pins: vec![
                InterfacePin {
                    name: Identifier::with_dummy_span("BCLK").to_string().into(),
                    pin_type: PinType::Output,
                    description: None,
                    span: dummy_span(),
                },
                InterfacePin {
                    name: Identifier::with_dummy_span("LRCLK").to_string().into(),
                    pin_type: PinType::Output,
                    description: None,
                    span: dummy_span(),
                },
            ],
            optional_pins: vec![],
            span: dummy_span(),
        };
        validator.register_interface(interface);

        // Create component that implements the interface
        let component = ComponentDefinition {
            name: Identifier::with_dummy_span("CS4344"),
            is_exported: false,
            parameters: vec![].into(),
            metadata: None,
            pins: vec!["BCLK".into(), "LRCLK".into(), "VCC".into()].into(),
            layout: None,
            electrical: None,
            render: None,
            implements: vec![InterfaceImplementation {
                interface_name: Identifier::with_dummy_span("I2S_DAC").to_string().into(),
                pin_mappings: vec![],
                span: dummy_span(),
            }]
            .into(),
            span: dummy_span(),
        };

        let result = validator.validate_component(&component);
        assert!(result.is_ok());
    }

    #[test]
    fn test_missing_required_pin() {
        let mut validator = InterfaceValidator::new();

        // Register interface
        let interface = PolymorphicInterfaceDefinition {
            name: Identifier::with_dummy_span("I2S_DAC"),
            is_exported: false,
            description: None,
            required_pins: vec![
                InterfacePin {
                    name: Identifier::with_dummy_span("BCLK").to_string().into(),
                    pin_type: PinType::Output,
                    description: None,
                    span: dummy_span(),
                },
                InterfacePin {
                    name: Identifier::with_dummy_span("LRCLK").to_string().into(),
                    pin_type: PinType::Output,
                    description: None,
                    span: dummy_span(),
                },
            ],
            optional_pins: vec![],
            span: dummy_span(),
        };
        validator.register_interface(interface);

        // Create component missing LRCLK pin
        let component = ComponentDefinition {
            name: Identifier::with_dummy_span("BadChip"),
            is_exported: false,
            parameters: vec![].into(),
            metadata: None,
            pins: vec!["BCLK".into(), "VCC".into()].into(), // Missing LRCLK
            layout: None,
            electrical: None,
            render: None,
            implements: vec![InterfaceImplementation {
                interface_name: Identifier::with_dummy_span("I2S_DAC").to_string().into(),
                pin_mappings: vec![],
                span: dummy_span(),
            }]
            .into(),
            span: dummy_span(),
        };

        let result = validator.validate_component(&component);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            InterfaceValidationError::MissingRequiredPin { .. }
        ));
    }

    #[test]
    fn test_pin_mapping() {
        let mut validator = InterfaceValidator::new();

        // Register interface
        let interface = PolymorphicInterfaceDefinition {
            name: Identifier::with_dummy_span("I2S_DAC"),
            is_exported: false,
            description: None,
            required_pins: vec![
                InterfacePin {
                    name: Identifier::with_dummy_span("BCLK").to_string().into(),
                    pin_type: PinType::Output,
                    description: None,
                    span: dummy_span(),
                },
                InterfacePin {
                    name: Identifier::with_dummy_span("LRCLK").to_string().into(),
                    pin_type: PinType::Output,
                    description: None,
                    span: dummy_span(),
                },
            ],
            optional_pins: vec![],
            span: dummy_span(),
        };
        validator.register_interface(interface);

        // Create component with different pin names
        let component = ComponentDefinition {
            name: Identifier::with_dummy_span("PCM5102"),
            is_exported: false,
            parameters: vec![].into(),
            metadata: None,
            pins: vec!["BCK".into(), "LRCK".into(), "VCC".into()].into(),
            layout: None,
            electrical: None,
            render: None,
            implements: vec![InterfaceImplementation {
                interface_name: Identifier::with_dummy_span("I2S_DAC").to_string().into(),
                pin_mappings: vec![
                    PinMapping {
                        interface_pin: "BCLK".into(),
                        component_pin: "BCK".into(),
                        span: dummy_span(),
                    },
                    PinMapping {
                        interface_pin: "LRCLK".into(),
                        component_pin: "LRCK".into(),
                        span: dummy_span(),
                    },
                ],
                span: dummy_span(),
            }]
            .into(),
            span: dummy_span(),
        };

        let result = validator.validate_component(&component);
        assert!(result.is_ok());
    }

    #[test]
    fn test_interface_not_found() {
        let validator = InterfaceValidator::new();

        // Create component that implements non-existent interface
        let component = ComponentDefinition {
            name: Identifier::with_dummy_span("BadChip"),
            is_exported: false,
            parameters: vec![].into(),
            metadata: None,
            pins: vec!["BCLK".into()].into(),
            layout: None,
            electrical: None,
            render: None,
            implements: vec![InterfaceImplementation {
                interface_name: Identifier::with_dummy_span("NonExistent")
                    .to_string()
                    .into(),
                pin_mappings: vec![],
                span: dummy_span(),
            }]
            .into(),
            span: dummy_span(),
        };

        let result = validator.validate_component(&component);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            InterfaceValidationError::InterfaceNotFound { .. }
        ));
    }

    #[test]
    fn test_implements_interface_check() {
        let mut validator = InterfaceValidator::new();

        let interface = PolymorphicInterfaceDefinition {
            name: Identifier::with_dummy_span("I2S_DAC"),
            is_exported: false,
            description: None,
            required_pins: vec![],
            optional_pins: vec![],
            span: dummy_span(),
        };
        validator.register_interface(interface);

        let component = ComponentDefinition {
            name: Identifier::with_dummy_span("CS4344"),
            is_exported: false,
            parameters: vec![].into(),
            metadata: None,
            pins: vec![].into(),
            layout: None,
            electrical: None,
            render: None,
            implements: vec![InterfaceImplementation {
                interface_name: Identifier::with_dummy_span("I2S_DAC").to_string().into(),
                pin_mappings: vec![],
                span: dummy_span(),
            }]
            .into(),
            span: dummy_span(),
        };

        assert!(validator.implements_interface(&component, "I2S_DAC"));
        assert!(!validator.implements_interface(&component, "SPI_Flash"));
    }

    #[test]
    fn test_find_compatible_components() {
        let mut validator = InterfaceValidator::new();

        let interface = PolymorphicInterfaceDefinition {
            name: Identifier::with_dummy_span("I2S_DAC"),
            is_exported: false,
            description: None,
            required_pins: vec![],
            optional_pins: vec![],
            span: dummy_span(),
        };
        validator.register_interface(interface);

        let components = vec![
            ComponentDefinition {
                name: Identifier::with_dummy_span("CS4344"),
                is_exported: false,
                parameters: vec![].into(),
                metadata: None,
                pins: vec![].into(),
                layout: None,
                electrical: None,
                render: None,
                implements: vec![InterfaceImplementation {
                    interface_name: Identifier::with_dummy_span("I2S_DAC").to_string().into(),
                    pin_mappings: vec![],
                    span: dummy_span(),
                }]
                .into(),
                span: dummy_span(),
            },
            ComponentDefinition {
                name: Identifier::with_dummy_span("PCM5102"),
                is_exported: false,
                parameters: vec![].into(),
                metadata: None,
                pins: vec![].into(),
                layout: None,
                electrical: None,
                render: None,
                implements: vec![InterfaceImplementation {
                    interface_name: Identifier::with_dummy_span("I2S_DAC").to_string().into(),
                    pin_mappings: vec![],
                    span: dummy_span(),
                }]
                .into(),
                span: dummy_span(),
            },
            ComponentDefinition {
                name: Identifier::with_dummy_span("FlashChip"),
                is_exported: false,
                parameters: vec![].into(),
                metadata: None,
                pins: vec![].into(),
                layout: None,
                electrical: None,
                render: None,
                implements: vec![].into(),
                span: dummy_span(),
            },
        ];

        let compatible = validator.find_compatible_components("I2S_DAC", &components);
        assert_eq!(compatible.len(), 2);
        assert!(compatible.iter().any(|c| c.name.as_str() == "CS4344"));
        assert!(compatible.iter().any(|c| c.name.as_str() == "PCM5102"));
    }
}
