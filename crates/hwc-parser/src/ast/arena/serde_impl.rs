//! Serde serialization/deserialization for AstArena
//!
//! This module implements custom Serialize and Deserialize traits for AstArena
//! to ensure proper handling of all arena fields.

use super::ast_arena::AstArena;
use serde::{Deserialize, Serialize};

impl Serialize for AstArena {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AstArena", 34)?;
        state.serialize_field("components", &self.components)?;
        state.serialize_field("pours", &self.pours)?;
        state.serialize_field("planes", &self.planes)?;
        state.serialize_field("polygons", &self.polygons)?;
        state.serialize_field("contacts", &self.contacts)?;
        state.serialize_field("routes", &self.routes)?;
        state.serialize_field("space_instances", &self.space_instances)?;
        state.serialize_field("for_loops", &self.for_loops)?;
        state.serialize_field("regions", &self.regions)?;
        state.serialize_field("substrates", &self.substrates)?;
        state.serialize_field("module_components", &self.module_components)?;
        state.serialize_field("module_internals", &self.module_internals)?;
        state.serialize_field("component_defs", &self.component_defs)?;
        state.serialize_field("material_defs", &self.material_defs)?;
        state.serialize_field("module_defs", &self.module_defs)?;
        state.serialize_field("profile_defs", &self.profile_defs)?;
        state.serialize_field("space_defs", &self.space_defs)?;
        state.serialize_field("bridge_defs", &self.bridge_defs)?;
        state.serialize_field("mechanical_defs", &self.mechanical_defs)?;
        state.serialize_field("interface_defs", &self.interface_defs)?;
        state.serialize_field("test_defs", &self.test_defs)?;
        state.serialize_field("device_defs", &self.device_defs)?;
        state.serialize_field("unit_defs", &self.unit_defs)?;
        state.serialize_field("const_defs", &self.const_defs)?;
        state.serialize_field("pattern_defs", &self.pattern_defs)?;
        state.serialize_field("strategy_defs", &self.strategy_defs)?;
        state.serialize_field("signal_group_defs", &self.signal_group_defs)?;
        state.serialize_field("material_alias_defs", &self.material_alias_defs)?;
        state.serialize_field("enum_defs", &self.enum_defs)?;
        state.serialize_field("struct_defs", &self.struct_defs)?;
        state.serialize_field("logic_defs", &self.logic_defs)?;
        state.serialize_field("shape_defs", &self.shape_defs)?;
        state.serialize_field("spice_model_defs", &self.spice_model_defs)?;
        state.serialize_field("subcircuit_defs", &self.subcircuit_defs)?;
        state.serialize_field(
            "polymorphic_interface_defs",
            &self.polymorphic_interface_defs,
        )?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for AstArena {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Components,
            Pours,
            Planes,
            Polygons,
            Contacts,
            Routes,
            SpaceInstances,
            ForLoops,
            Regions,
            Substrates,
            ModuleComponents,
            ModuleInternals,
            ComponentDefs,
            MaterialDefs,
            ModuleDefs,
            ProfileDefs,
            SpaceDefs,
            BridgeDefs,
            MechanicalDefs,
            InterfaceDefs,
            TestDefs,
            DeviceDefs,
            UnitDefs,
            ConstDefs,
            PatternDefs,
            StrategyDefs,
            SignalGroupDefs,
            MaterialAliasDefs,
            EnumDefs,
            StructDefs,
            LogicDefs,
            ShapeDefs,
            SpiceModelDefs,
            SubcircuitDefs,
            PolymorphicInterfaceDefs,
        }

        struct AstArenaVisitor;

        impl<'de> serde::de::Visitor<'de> for AstArenaVisitor {
            type Value = AstArena;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct AstArena")
            }

            fn visit_map<V>(self, mut map: V) -> Result<AstArena, V::Error>
            where
                V: serde::de::MapAccess<'de>,
            {
                let mut components = None;
                let mut pours = None;
                let mut planes = None;
                let mut polygons = None;
                let mut contacts = None;
                let mut routes = None;
                let mut space_instances = None;
                let mut for_loops = None;
                let mut regions = None;
                let mut substrates = None;
                let mut module_components = None;
                let mut module_internals = None;
                let mut component_defs = None;
                let mut material_defs = None;
                let mut module_defs = None;
                let mut profile_defs = None;
                let mut space_defs = None;
                let mut bridge_defs = None;
                let mut mechanical_defs = None;
                let mut interface_defs = None;
                let mut test_defs = None;
                let mut device_defs = None;
                let mut unit_defs = None;
                let mut const_defs = None;
                let mut pattern_defs = None;
                let mut strategy_defs = None;
                let mut signal_group_defs = None;
                let mut material_alias_defs = None;
                let mut enum_defs = None;
                let mut struct_defs = None;
                let mut logic_defs = None;
                let mut shape_defs = None;
                let mut spice_model_defs = None;
                let mut subcircuit_defs = None;
                let mut polymorphic_interface_defs = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Components => components = Some(map.next_value()?),
                        Field::Pours => pours = Some(map.next_value()?),
                        Field::Planes => planes = Some(map.next_value()?),
                        Field::Polygons => polygons = Some(map.next_value()?),
                        Field::Contacts => contacts = Some(map.next_value()?),
                        Field::Routes => routes = Some(map.next_value()?),
                        Field::SpaceInstances => space_instances = Some(map.next_value()?),
                        Field::ForLoops => for_loops = Some(map.next_value()?),
                        Field::Regions => regions = Some(map.next_value()?),
                        Field::Substrates => substrates = Some(map.next_value()?),
                        Field::ModuleComponents => module_components = Some(map.next_value()?),
                        Field::ModuleInternals => module_internals = Some(map.next_value()?),
                        Field::ComponentDefs => component_defs = Some(map.next_value()?),
                        Field::MaterialDefs => material_defs = Some(map.next_value()?),
                        Field::ModuleDefs => module_defs = Some(map.next_value()?),
                        Field::ProfileDefs => profile_defs = Some(map.next_value()?),
                        Field::SpaceDefs => space_defs = Some(map.next_value()?),
                        Field::BridgeDefs => bridge_defs = Some(map.next_value()?),
                        Field::MechanicalDefs => mechanical_defs = Some(map.next_value()?),
                        Field::InterfaceDefs => interface_defs = Some(map.next_value()?),
                        Field::TestDefs => test_defs = Some(map.next_value()?),
                        Field::DeviceDefs => device_defs = Some(map.next_value()?),
                        Field::UnitDefs => unit_defs = Some(map.next_value()?),
                        Field::ConstDefs => const_defs = Some(map.next_value()?),
                        Field::PatternDefs => pattern_defs = Some(map.next_value()?),
                        Field::StrategyDefs => strategy_defs = Some(map.next_value()?),
                        Field::SignalGroupDefs => signal_group_defs = Some(map.next_value()?),
                        Field::MaterialAliasDefs => material_alias_defs = Some(map.next_value()?),
                        Field::EnumDefs => enum_defs = Some(map.next_value()?),
                        Field::StructDefs => struct_defs = Some(map.next_value()?),
                        Field::LogicDefs => logic_defs = Some(map.next_value()?),
                        Field::ShapeDefs => shape_defs = Some(map.next_value()?),
                        Field::SpiceModelDefs => spice_model_defs = Some(map.next_value()?),
                        Field::SubcircuitDefs => subcircuit_defs = Some(map.next_value()?),
                        Field::PolymorphicInterfaceDefs => {
                            polymorphic_interface_defs = Some(map.next_value()?)
                        }
                    }
                }

                Ok(AstArena {
                    components: components.unwrap_or_default(),
                    pours: pours.unwrap_or_default(),
                    planes: planes.unwrap_or_default(),
                    polygons: polygons.unwrap_or_default(),
                    contacts: contacts.unwrap_or_default(),
                    routes: routes.unwrap_or_default(),
                    space_instances: space_instances.unwrap_or_default(),
                    for_loops: for_loops.unwrap_or_default(),
                    regions: regions.unwrap_or_default(),
                    substrates: substrates.unwrap_or_default(),
                    module_components: module_components.unwrap_or_default(),
                    module_internals: module_internals.unwrap_or_default(),
                    component_defs: component_defs.unwrap_or_default(),
                    material_defs: material_defs.unwrap_or_default(),
                    module_defs: module_defs.unwrap_or_default(),
                    profile_defs: profile_defs.unwrap_or_default(),
                    space_defs: space_defs.unwrap_or_default(),
                    bridge_defs: bridge_defs.unwrap_or_default(),
                    mechanical_defs: mechanical_defs.unwrap_or_default(),
                    interface_defs: interface_defs.unwrap_or_default(),
                    test_defs: test_defs.unwrap_or_default(),
                    device_defs: device_defs.unwrap_or_default(),
                    unit_defs: unit_defs.unwrap_or_default(),
                    const_defs: const_defs.unwrap_or_default(),
                    pattern_defs: pattern_defs.unwrap_or_default(),
                    strategy_defs: strategy_defs.unwrap_or_default(),
                    signal_group_defs: signal_group_defs.unwrap_or_default(),
                    material_alias_defs: material_alias_defs.unwrap_or_default(),
                    enum_defs: enum_defs.unwrap_or_default(),
                    struct_defs: struct_defs.unwrap_or_default(),
                    logic_defs: logic_defs.unwrap_or_default(),
                    shape_defs: shape_defs.unwrap_or_default(),
                    spice_model_defs: spice_model_defs.unwrap_or_default(),
                    subcircuit_defs: subcircuit_defs.unwrap_or_default(),
                    polymorphic_interface_defs: polymorphic_interface_defs.unwrap_or_default(),
                })
            }
        }

        const FIELDS: &[&str] = &[
            "components",
            "pours",
            "planes",
            "polygons",
            "contacts",
            "routes",
            "space_instances",
            "for_loops",
            "regions",
            "substrates",
            "module_components",
            "module_internals",
            "component_defs",
            "material_defs",
            "module_defs",
            "profile_defs",
            "space_defs",
            "bridge_defs",
            "mechanical_defs",
            "interface_defs",
            "test_defs",
            "device_defs",
            "unit_defs",
            "const_defs",
            "pattern_defs",
            "strategy_defs",
            "signal_group_defs",
            "material_alias_defs",
            "enum_defs",
            "struct_defs",
            "logic_defs",
            "shape_defs",
            "spice_model_defs",
            "subcircuit_defs",
            "polymorphic_interface_defs",
        ];
        deserializer.deserialize_struct("AstArena", FIELDS, AstArenaVisitor)
    }
}
