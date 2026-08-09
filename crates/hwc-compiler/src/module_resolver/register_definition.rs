//! Definition registration into the symbol table (HPM layer per definition).

use crate::module_resolver::ResolverError;
use crate::symbol_table::SymbolTable;
use hwc_parser::Definition;

impl super::ModuleResolver {
    /// Register a definition into the symbol table (HPM layer)
    ///
    /// Imported definitions go into the HPM layer, not the local layer.
    /// This enables the Authority Stack (Local > HPM > Prelude > Core).
    pub(super) fn register_definition(
        &self,
        definition: &Definition,
        arena: &hwc_parser::ast::arena::AstArena,
        symbol_table: &mut SymbolTable,
    ) -> Result<(), ResolverError> {
        match definition {
            Definition::Bridge(bridge_id) => {
                let bridge = &arena.bridge_defs[*bridge_id];
                symbol_table.register_import_bridge(bridge.clone());
                Ok(())
            }
            Definition::Material(mat_id) => {
                let mat = &arena.material_defs[*mat_id];
                symbol_table.register_import_material(mat.clone());
                Ok(())
            }
            Definition::Profile(profile_id) => {
                let profile = &arena.profile_defs[*profile_id];
                symbol_table.register_import_profile(profile.clone());
                Ok(())
            }
            Definition::Component(component) => {
                symbol_table.register_import_component(arena.component_defs[*component].clone());
                Ok(())
            }
            Definition::Module(module_id) => {
                let module = &arena.module_defs[*module_id];
                symbol_table.register_import_module(module.clone());
                Ok(())
            }
            Definition::Logic(logic_def) => {
                let logic = &arena.logic_defs[*logic_def];
                symbol_table.register_import_logic(logic.clone());
                Ok(())
            }
            Definition::Enum(enum_def) => {
                let enum_d = &arena.enum_defs[*enum_def];
                symbol_table.register_import_enum(enum_d.clone());
                Ok(())
            }
            Definition::Struct(struct_def) => {
                let struct_d = &arena.struct_defs[*struct_def];
                symbol_table.register_import_struct(struct_d.clone());
                Ok(())
            }
            Definition::Mechanical(mech_id) => {
                let mechanical = &arena.mechanical_defs[*mech_id];
                symbol_table.register_import_mechanical(mechanical.clone());
                Ok(())
            }
            Definition::Interface(iface_id) => {
                let interface = &arena.interface_defs[*iface_id];
                symbol_table.register_import_interface(interface.clone());
                Ok(())
            }
            Definition::PolymorphicInterface(_poly_interface) => {
                // TODO: Register polymorphic interfaces in symbol table
                Ok(())
            }
            Definition::Test(test_id) => {
                let test = &arena.test_defs[*test_id];
                symbol_table.register_import_test(test.clone());
                Ok(())
            }
            Definition::SignalGroup(signal_group) => {
                let sg = &arena.signal_group_defs[*signal_group];
                symbol_table.register_import_signal_group(sg.clone());
                Ok(())
            }
            Definition::Pattern(pattern) => {
                let pat = &arena.pattern_defs[*pattern];
                symbol_table.register_import_pattern(pat.clone());
                Ok(())
            }
            Definition::Strategy(strategy) => {
                let strat = &arena.strategy_defs[*strategy];
                symbol_table.register_import_strategy(strat.clone());
                Ok(())
            }
            Definition::Unit(unit_id) => {
                let unit = &arena.unit_defs[*unit_id];
                symbol_table.register_import_unit(unit.clone());
                Ok(())
            }
            Definition::Device(device_id) => {
                let device = &arena.device_defs[*device_id];
                symbol_table.register_import_device(device.clone());
                Ok(())
            }
            Definition::Const(const_id) => {
                let const_def = &arena.const_defs[*const_id];
                symbol_table.register_import_constant(const_def.clone());
                Ok(())
            }
            Definition::Shape(shape_def) => {
                let shape = &arena.shape_defs[*shape_def];
                symbol_table.register_import_shape(shape.clone());
                Ok(())
            }
            Definition::MaterialAlias(alias) => {
                let mat_alias = &arena.material_alias_defs[*alias];
                symbol_table.register_import_material_alias(mat_alias.clone());
                Ok(())
            }
            Definition::Space(space_id) => {
                // v0.2.1: Register imported space definitions for hierarchical composition
                let space_def = &arena.space_defs[*space_id];
                symbol_table.register_import_space(space_def.clone());
                Ok(())
            }
            Definition::SpiceModel(spice_model) => {
                // v0.2.1: Register imported SPICE model cards for PDK physics
                let sm = &arena.spice_model_defs[*spice_model];
                symbol_table.register_import_spice_model(sm.clone());
                Ok(())
            }
            Definition::Subcircuit(subcircuit) => {
                // v0.3.0+: Register imported native typed subcircuit definitions (replaces raw SPICE)
                let sub = &arena.subcircuit_defs[*subcircuit];
                symbol_table.register_import_subcircuit(sub.clone());
                Ok(())
            }
        }
    }
}
