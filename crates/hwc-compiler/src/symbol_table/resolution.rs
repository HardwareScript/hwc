//! Symbol resolution methods (Pass 2) - Unified Definition Architecture

use super::{error::SymbolError, layer::SymbolTable, Definition};
use compact_str::CompactString;
use hwc_parser::{
    logic::{EnumDefinition, LogicDefinition, StructDefinition},
    BridgeDefinition, ComponentDefinition, ConstDefinition, DeviceDefinition,
    InterfaceDefinition, MaterialDefinition, MechanicalDefinition,
    ModuleDefinition, PatternDefinition, ProfileDefinition, ShapeDefinition,
    SignalGroupDefinition, SpaceDefinition, SpiceModelDefinition, SubcircuitDefinition,
    StrategyDefinition, TestDefinition, UnitDefinition,
};

impl SymbolTable {
    /// Get a space definition by name
    pub fn get_space(&self, name: &str) -> Result<&SpaceDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Space(space)) => Ok(space),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "space",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "space", None)),
        }
    }

    /// Get a material definition by name
    pub fn get_material(&self, name: &str) -> Result<&MaterialDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Material(mat)) => Ok(mat),
            Some(Definition::MaterialAlias(alias)) => {
                // Follow alias chain (target is an Identifier, convert to &str)
                self.get_material(alias.target.as_str())
            }
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "material",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "material", None)),
        }
    }

    /// Get a profile definition by name
    pub fn get_profile(&self, name: &str) -> Result<&ProfileDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Profile(prof)) => Ok(prof),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "profile",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "profile", None)),
        }
    }

    /// Get a component definition by name
    pub fn get_component(&self, name: &str) -> Result<&ComponentDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Component(comp)) => Ok(comp),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "component",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "component", None)),
        }
    }

    /// Get a module definition by name
    pub fn get_module(&self, name: &str) -> Result<&ModuleDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Module(module)) => Ok(module),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "module",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "module", None)),
        }
    }

    /// Get a device definition by name
    pub fn get_device(&self, name: &str) -> Result<&DeviceDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Device(dev)) => Ok(dev),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "device",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "device", None)),
        }
    }

    /// Get a mechanical definition by name
    pub fn get_mechanical(&self, name: &str) -> Result<&MechanicalDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Mechanical(mech)) => Ok(mech),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "mechanical",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "mechanical", None)),
        }
    }

    /// Get an interface definition by name
    pub fn get_interface(&self, name: &str) -> Result<&InterfaceDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Interface(iface)) => Ok(iface),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "interface",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "interface", None)),
        }
    }

    /// Get a test definition by name
    pub fn get_test(&self, name: &str) -> Result<&TestDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Test(test)) => Ok(test),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "test",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "test", None)),
        }
    }

    /// Get a signal group definition by name
    pub fn get_signal_group(&self, name: &str) -> Result<&SignalGroupDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::SignalGroup(sg)) => Ok(sg),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "signal_group",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "signal_group", None)),
        }
    }

    /// Get a pattern definition by name
    pub fn get_pattern(&self, name: &str) -> Result<&PatternDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Pattern(pat)) => Ok(pat),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "pattern",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "pattern", None)),
        }
    }

    /// Get a strategy definition by name
    pub fn get_strategy(&self, name: &str) -> Result<&StrategyDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Strategy(strat)) => Ok(strat),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "strategy",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "strategy", None)),
        }
    }

    /// Get a logic definition by name
    pub fn get_logic(&self, name: &str) -> Result<&LogicDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Logic(logic)) => Ok(logic),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "logic",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "logic", None)),
        }
    }

    /// Get an enum definition by name
    pub fn get_enum(&self, name: &str) -> Result<&EnumDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Enum(e)) => Ok(e),
            Some(other) => Err(SymbolError::type_mismatch(name, "enum", other.kind_str())),
            None => Err(SymbolError::undefined(name.into(), "enum", None)),
        }
    }

    /// Get a struct definition by name
    pub fn get_struct(&self, name: &str) -> Result<&StructDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Struct(s)) => Ok(s),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "struct",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "struct", None)),
        }
    }

    /// Get a unit definition by name
    pub fn get_unit(&self, name: &str) -> Option<&UnitDefinition> {
        match self.get_symbol(name) {
            Some(Definition::Unit(unit)) => Some(unit),
            _ => None,
        }
    }

    /// Get a constant definition by name
    pub fn get_const(&self, name: &str) -> Option<&ConstDefinition> {
        match self.get_symbol(name) {
            Some(Definition::Const(c)) => Some(c),
            _ => None,
        }
    }

    /// Get a shape definition by name
    pub fn get_shape(&self, name: &str) -> Option<&ShapeDefinition> {
        match self.get_symbol(name) {
            Some(Definition::Shape(shape)) => Some(shape),
            _ => None,
        }
    }

    /// Get a SPICE model definition by name
    pub fn get_spice_model(&self, name: &str) -> Result<&SpiceModelDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::SpiceModel(model)) => Ok(model),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "spice_model",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "spice_model", None)),
        }
    }

    /// Get a SPICE subcircuit definition by name
    pub fn get_subcircuit(&self, name: &str) -> Result<&SubcircuitDefinition, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Subcircuit(subckt)) => Ok(subckt),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "subcircuit",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "subcircuit", None)),
        }
    }

    /// Helper methods for specific checks
    pub fn has_material(&self, name: &str) -> bool {
        matches!(
            self.get_symbol(name),
            Some(Definition::Material(_)) | Some(Definition::MaterialAlias(_))
        )
    }

    pub fn has_profile(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Profile(_)))
    }

    pub fn has_component(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Component(_)))
    }

    pub fn has_module(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Module(_)))
    }

    pub fn has_device(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Device(_)))
    }

    pub fn has_mechanical(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Mechanical(_)))
    }

    pub fn has_interface(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Interface(_)))
    }

    pub fn has_test(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Test(_)))
    }

    pub fn has_signal_group(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::SignalGroup(_)))
    }

    pub fn has_pattern(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Pattern(_)))
    }

    pub fn has_strategy(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Strategy(_)))
    }

    pub fn has_logic(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Logic(_)))
    }

    pub fn has_enum(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Enum(_)))
    }

    pub fn has_struct(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Struct(_)))
    }

    pub fn has_shape(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Shape(_)))
    }

    pub fn has_spice_model(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::SpiceModel(_)))
    }

    pub fn has_spice_subcircuit(&self, name: &str) -> bool {
        matches!(self.get_symbol(name), Some(Definition::Subcircuit(_)))
    }

    /// Collect all materials (for database population)
    pub fn materials(&self) -> rustc_hash::FxHashMap<CompactString, MaterialDefinition> {
        let mut all_materials = rustc_hash::FxHashMap::default();

        for (_name, def) in self.iter_all_symbols() {
            if let Definition::Material(mat) = def {
                all_materials.insert(mat.name.as_str().into(), mat.clone());
            }
        }

        all_materials
    }

    /// Collect all bridges (for via resolver)
    pub fn get_all_bridges(&self) -> Vec<BridgeDefinition> {
        self.iter_all_symbols()
            .filter_map(|(_name, def)| {
                if let Definition::Bridge(bridge) = def {
                    Some(bridge.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Iterate over all device definitions
    pub fn iter_all_devices(&self) -> impl Iterator<Item = (&CompactString, &DeviceDefinition)> {
        self.iter_all_symbols().filter_map(|(name, def)| {
            if let Definition::Device(device) = def {
                Some((name, device))
            } else {
                None
            }
        })
    }

    /// Debug: List all profile names
    pub fn debug_list_profiles(&self) -> Vec<String> {
        self.iter_all_symbols()
            .filter_map(|(name, def)| {
                if matches!(def, Definition::Profile(_)) {
                    Some(name.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Debug: List all space names in local layer
    pub fn list_local_spaces(&self) -> Vec<String> {
        self.local
            .iter()
            .filter_map(|(name, def)| {
                if matches!(def, Definition::Space(_)) {
                    Some(name.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Debug: List all space names in HPM layers
    pub fn list_hpm_spaces(&self) -> Vec<Vec<String>> {
        self.hpm
            .iter()
            .map(|layer| {
                layer
                    .iter()
                    .filter_map(|(name, def)| {
                        if matches!(def, Definition::Space(_)) {
                            Some(name.to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// Add a unit to the prelude layer (for standard library loading)
    pub fn add_prelude_unit(&mut self, name: CompactString, unit: UnitDefinition) {
        self.prelude.insert(name, Definition::Unit(unit));
    }

    /// Add a constant to the prelude layer (for standard library loading)
    pub fn add_prelude_constant(&mut self, name: CompactString, constant: ConstDefinition) {
        self.prelude.insert(name, Definition::Const(constant));
    }

    /// Get all constants from all layers (for constraint solver, via resolver, etc.)
    /// Returns a map of constant name -> value
    pub fn get_all_constants(&self) -> rustc_hash::FxHashMap<CompactString, f64> {
        let mut constants = rustc_hash::FxHashMap::default();

        for (_name, def) in self.iter_all_symbols() {
            if let Definition::Const(c) = def {
                constants.insert(c.name.clone(), c.value);
            }
        }

        constants
    }

    /// Resolve a unit symbol by looking it up across all layers
    /// Returns the UnitDefinition if found
    pub fn resolve_unit_symbol(&self, symbol: &str) -> Option<&UnitDefinition> {
        match self.get_symbol(symbol) {
            Some(Definition::Unit(unit)) => Some(unit),
            _ => None,
        }
    }

    /// Convert a measurement to nanometers using the symbol table for unit resolution.
    /// This delegates to the canonical ir::conversions::measurement_to_nm function.
    pub fn measurement_to_nm(
        &self,
        measurement: &hwc_parser::Measurement,
    ) -> Result<i64, String> {
        crate::ir::conversions::measurement_to_nm_simple(measurement, self)
    }
}
