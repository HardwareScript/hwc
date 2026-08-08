//! Definition registration into the symbol table (HPM layer per definition).

use crate::module_resolver::ResolverError;
use crate::symbol_table::SymbolTable;
use hwc_parser::Definition;

impl super::ModuleResolver {
    /// Register a definition into the symbol table (HPM layer)
    ///
    /// Imported definitions go into the HPM layer, not the local layer.
    /// This enables the Authority Stack (Local > HPM > Prelude > Core).
    pub(super) fn register_definition<'ast>(
        &self,
        definition: &Definition<'ast>,
        symbol_table: &mut SymbolTable<'ast>,
    ) -> Result<(), ResolverError> {
        match definition {
            Definition::Bridge(bridge) => {
                symbol_table.register_import_bridge(bridge.clone());
                Ok(())
            }
            Definition::Material(mat) => {
                symbol_table.register_import_material(mat.clone());
                Ok(())
            }
            Definition::Profile(profile) => {
                symbol_table.register_import_profile(profile.as_ref().clone());
                Ok(())
            }
            Definition::Component(component) => {
                symbol_table.register_import_component(component.clone());
                Ok(())
            }
            Definition::Module(module) => {
                symbol_table.register_import_module(module.clone());
                Ok(())
            }
            Definition::Logic(logic_def) => {
                symbol_table.register_import_logic(logic_def.clone());
                Ok(())
            }
            Definition::Enum(enum_def) => {
                symbol_table.register_import_enum(enum_def.clone());
                Ok(())
            }
            Definition::Struct(struct_def) => {
                symbol_table.register_import_struct(struct_def.clone());
                Ok(())
            }
            Definition::Mechanical(mechanical) => {
                symbol_table.register_import_mechanical(mechanical.clone());
                Ok(())
            }
            Definition::Interface(interface) => {
                symbol_table.register_import_interface(interface.clone());
                Ok(())
            }
            Definition::PolymorphicInterface(_poly_interface) => {
                // TODO: Register polymorphic interfaces in symbol table
                Ok(())
            }
            Definition::Test(test) => {
                symbol_table.register_import_test(test.clone());
                Ok(())
            }
            Definition::SignalGroup(signal_group) => {
                symbol_table.register_import_signal_group(signal_group.clone());
                Ok(())
            }
            Definition::Pattern(pattern) => {
                symbol_table.register_import_pattern(pattern.clone());
                Ok(())
            }
            Definition::Strategy(strategy) => {
                symbol_table.register_import_strategy(strategy.clone());
                Ok(())
            }
            Definition::Unit(unit) => {
                symbol_table.register_import_unit(unit.clone());
                Ok(())
            }
            Definition::Device(device) => {
                symbol_table.register_import_device(device.clone());
                Ok(())
            }
            Definition::Const(const_def) => {
                symbol_table.register_import_constant(const_def.clone());
                Ok(())
            }
            Definition::Shape(shape_def) => {
                symbol_table.register_import_shape(shape_def.clone());
                Ok(())
            }
            Definition::MaterialAlias(alias) => {
                symbol_table.register_import_material_alias(alias.clone());
                Ok(())
            }
            Definition::Space(space_def) => {
                // v0.2.1: Register imported space definitions for hierarchical composition
                symbol_table.register_import_space((**space_def).clone());
                Ok(())
            }
            Definition::SpiceModel(spice_model) => {
                // v0.2.1: Register imported SPICE model cards for PDK physics
                symbol_table.register_import_spice_model(spice_model.clone());
                Ok(())
            }
            Definition::Subcircuit(subcircuit) => {
                // v0.3.0+: Register imported native typed subcircuit definitions (replaces raw SPICE)
                symbol_table.register_import_subcircuit(subcircuit.clone());
                Ok(())
            }
        }
    }
}
