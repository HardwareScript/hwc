//! AstArena: Centralized storage for all AST nodes
//!
//! All AST nodes are stored in contiguous vectors and referenced
//! via type-safe u32 indices. This provides:
//! - Cache-friendly sequential access
//! - No lifetime management
//! - Type-safe references
//! - Efficient bulk operations

use super::core::IndexVec;
use super::id_types::*;
use crate::ast::component::{ComponentDefinition, ComponentPlacement};
use crate::ast::module::ModuleComponentPlacement;
use crate::ast::space::ModuleInternalPlacement;
use crate::ast::space::{
    ContactPlacement, PlanePlacement, PolygonPlacement, PourPlacement, RegionDefinition, Route,
    SpaceForLoop, SpaceInstancePlacement, SubstratePlacement,
};

/// Centralized arena for all AST nodes
///
/// All AST nodes are stored in contiguous vectors and referenced
/// via type-safe u32 indices.
///
/// # Memory Layout
///
/// ```text
/// components: [Component₀, Component₁, Component₂, ...]  ← Contiguous
/// routes:     [Route₀, Route₁, Route₂, ...]              ← Contiguous
/// pours:      [Pour₀, Pour₁, Pour₂, ...]                 ← Contiguous
/// ```
///
/// # Thread Safety
///
/// AstArena can be shared across threads (via Arc or &) because:
/// - All ID types are Copy + Send + Sync
/// - Immutable access to arena is safe from multiple threads
///
/// # Performance
///
/// - Allocation: O(1) amortized (Vec::push)
/// - Lookup: O(1) (array indexing)
/// - Traversal: Sequential scan (optimal cache usage)
/// - Drop: O(n) but fast (contiguous deallocation)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AstArena {
    // Space placements
    pub components: IndexVec<ComponentId, ComponentPlacement>,
    pub pours: IndexVec<PourId, PourPlacement>,
    pub planes: IndexVec<PlaneId, PlanePlacement>,
    pub polygons: IndexVec<PolygonId, PolygonPlacement>,
    pub contacts: IndexVec<ContactId, ContactPlacement>,
    pub routes: IndexVec<RouteId, Route>,
    pub space_instances: IndexVec<SpaceInstanceId, SpaceInstancePlacement>,
    pub for_loops: IndexVec<ForLoopId, SpaceForLoop>,
    pub regions: IndexVec<RegionId, RegionDefinition>,
    pub substrates: IndexVec<SubstrateId, SubstratePlacement>,

    // Module placements
    pub module_components: IndexVec<ModuleComponentId, ModuleComponentPlacement>,
    pub module_internals: IndexVec<ModuleInternalId, ModuleInternalPlacement>,

    // Top-level definitions
    pub component_defs: IndexVec<ComponentDefId, ComponentDefinition>,
    pub material_defs: IndexVec<MaterialDefId, crate::ast::MaterialDefinition>,
    pub module_defs: IndexVec<ModuleDefId, crate::ast::ModuleDefinition>,
    pub profile_defs: IndexVec<ProfileDefId, crate::ast::ProfileDefinition>,
    pub space_defs: IndexVec<SpaceDefId, crate::ast::SpaceDefinition>,
    pub bridge_defs: IndexVec<BridgeDefId, crate::ast::BridgeDefinition>,
    pub mechanical_defs: IndexVec<MechanicalDefId, crate::ast::MechanicalDefinition>,
    pub interface_defs: IndexVec<InterfaceDefId, crate::ast::InterfaceDefinition>,
    pub test_defs: IndexVec<TestDefId, crate::ast::TestDefinition>,
    pub device_defs: IndexVec<DeviceDefId, crate::ast::DeviceDefinition>,
    pub unit_defs: IndexVec<UnitDefId, crate::ast::UnitDefinition>,
    pub const_defs: IndexVec<ConstDefId, crate::ast::ConstDefinition>,

    // Additional definition types
    pub pattern_defs: IndexVec<PatternDefId, crate::ast::PatternDefinition>,
    pub strategy_defs: IndexVec<StrategyDefId, crate::ast::StrategyDefinition>,
    pub signal_group_defs: IndexVec<SignalGroupDefId, crate::ast::SignalGroupDefinition>,
    pub material_alias_defs: IndexVec<MaterialAliasDefId, crate::ast::MaterialAliasDefinition>,
    pub enum_defs: IndexVec<EnumDefId, crate::ast::EnumDefinition>,
    pub struct_defs: IndexVec<StructDefId, crate::ast::StructDefinition>,
    pub logic_defs: IndexVec<LogicDefId, crate::ast::LogicDefinition>,
    pub shape_defs: IndexVec<ShapeDefId, crate::ast::ShapeDefinition>,
    pub spice_model_defs: IndexVec<SpiceModelDefId, crate::ast::SpiceModelDefinition>,
    pub subcircuit_defs: IndexVec<SubcircuitDefId, crate::ast::SubcircuitDefinition>,
    pub polymorphic_interface_defs:
        IndexVec<PolymorphicInterfaceDefId, crate::ast::PolymorphicInterfaceDefinition>,
}

impl AstArena {
    /// Create a new empty arena
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new arena with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            components: IndexVec::with_capacity(capacity / 2),
            pours: IndexVec::with_capacity(capacity / 10),
            planes: IndexVec::with_capacity(capacity / 20),
            polygons: IndexVec::with_capacity(capacity / 20),
            contacts: IndexVec::with_capacity(capacity / 10),
            routes: IndexVec::with_capacity(capacity / 5),
            space_instances: IndexVec::with_capacity(capacity / 20),
            for_loops: IndexVec::with_capacity(capacity / 50),
            regions: IndexVec::with_capacity(capacity / 50),
            substrates: IndexVec::with_capacity(capacity / 50),
            module_components: IndexVec::with_capacity(capacity / 10),
            module_internals: IndexVec::with_capacity(capacity / 20),

            // Top-level definitions
            component_defs: IndexVec::with_capacity(capacity / 10),
            material_defs: IndexVec::with_capacity(capacity / 20),
            module_defs: IndexVec::with_capacity(capacity / 20),
            profile_defs: IndexVec::with_capacity(capacity / 50),
            space_defs: IndexVec::with_capacity(capacity / 20),
            bridge_defs: IndexVec::with_capacity(capacity / 100),
            mechanical_defs: IndexVec::with_capacity(capacity / 100),
            interface_defs: IndexVec::with_capacity(capacity / 50),
            test_defs: IndexVec::with_capacity(capacity / 100),
            device_defs: IndexVec::with_capacity(capacity / 50),
            unit_defs: IndexVec::with_capacity(capacity / 100),
            const_defs: IndexVec::with_capacity(capacity / 50),

            // Additional definition types
            pattern_defs: IndexVec::with_capacity(capacity / 100),
            strategy_defs: IndexVec::with_capacity(capacity / 100),
            signal_group_defs: IndexVec::with_capacity(capacity / 100),
            material_alias_defs: IndexVec::with_capacity(capacity / 100),
            enum_defs: IndexVec::with_capacity(capacity / 100),
            struct_defs: IndexVec::with_capacity(capacity / 100),
            logic_defs: IndexVec::with_capacity(capacity / 100),
            shape_defs: IndexVec::with_capacity(capacity / 100),
            spice_model_defs: IndexVec::with_capacity(capacity / 100),
            subcircuit_defs: IndexVec::with_capacity(capacity / 100),
            polymorphic_interface_defs: IndexVec::with_capacity(capacity / 100),
        }
    }

    // =============================================================================
    // Space Placement Allocators
    // =============================================================================

    #[inline]
    pub fn alloc_component(&mut self, comp: ComponentPlacement) -> ComponentId {
        self.components.push(comp)
    }

    #[inline]
    pub fn alloc_pour(&mut self, pour: PourPlacement) -> PourId {
        self.pours.push(pour)
    }

    #[inline]
    pub fn alloc_plane(&mut self, plane: PlanePlacement) -> PlaneId {
        self.planes.push(plane)
    }

    #[inline]
    pub fn alloc_polygon(&mut self, polygon: PolygonPlacement) -> PolygonId {
        self.polygons.push(polygon)
    }

    #[inline]
    pub fn alloc_contact(&mut self, contact: ContactPlacement) -> ContactId {
        self.contacts.push(contact)
    }

    #[inline]
    pub fn alloc_route(&mut self, route: Route) -> RouteId {
        self.routes.push(route)
    }

    #[inline]
    pub fn alloc_space_instance(&mut self, inst: SpaceInstancePlacement) -> SpaceInstanceId {
        self.space_instances.push(inst)
    }

    #[inline]
    pub fn alloc_for_loop(&mut self, loop_stmt: SpaceForLoop) -> ForLoopId {
        self.for_loops.push(loop_stmt)
    }

    #[inline]
    pub fn alloc_region(&mut self, region: RegionDefinition) -> RegionId {
        self.regions.push(region)
    }

    #[inline]
    pub fn alloc_substrate(&mut self, substrate: SubstratePlacement) -> SubstrateId {
        self.substrates.push(substrate)
    }

    // =============================================================================
    // Module Placement Allocators
    // =============================================================================

    #[inline]
    pub fn alloc_module_component(&mut self, mc: ModuleComponentPlacement) -> ModuleComponentId {
        self.module_components.push(mc)
    }

    #[inline]
    pub fn alloc_module_internal(&mut self, mi: ModuleInternalPlacement) -> ModuleInternalId {
        self.module_internals.push(mi)
    }

    // =============================================================================
    // Top-Level Definition Allocators
    // =============================================================================

    #[inline]
    pub fn alloc_component_def(&mut self, cd: ComponentDefinition) -> ComponentDefId {
        self.component_defs.push(cd)
    }

    #[inline]
    pub fn alloc_material_def(&mut self, md: crate::ast::MaterialDefinition) -> MaterialDefId {
        self.material_defs.push(md)
    }

    #[inline]
    pub fn alloc_module_def(&mut self, md: crate::ast::ModuleDefinition) -> ModuleDefId {
        self.module_defs.push(md)
    }

    #[inline]
    pub fn alloc_profile_def(&mut self, pd: crate::ast::ProfileDefinition) -> ProfileDefId {
        self.profile_defs.push(pd)
    }

    #[inline]
    pub fn alloc_space_def(&mut self, sd: crate::ast::SpaceDefinition) -> SpaceDefId {
        self.space_defs.push(sd)
    }

    #[inline]
    pub fn alloc_bridge_def(&mut self, bd: crate::ast::BridgeDefinition) -> BridgeDefId {
        self.bridge_defs.push(bd)
    }

    #[inline]
    pub fn alloc_mechanical_def(
        &mut self,
        md: crate::ast::MechanicalDefinition,
    ) -> MechanicalDefId {
        self.mechanical_defs.push(md)
    }

    #[inline]
    pub fn alloc_interface_def(&mut self, id: crate::ast::InterfaceDefinition) -> InterfaceDefId {
        self.interface_defs.push(id)
    }

    #[inline]
    pub fn alloc_test_def(&mut self, td: crate::ast::TestDefinition) -> TestDefId {
        self.test_defs.push(td)
    }

    #[inline]
    pub fn alloc_device_def(&mut self, dd: crate::ast::DeviceDefinition) -> DeviceDefId {
        self.device_defs.push(dd)
    }

    #[inline]
    pub fn alloc_unit_def(&mut self, ud: crate::ast::UnitDefinition) -> UnitDefId {
        self.unit_defs.push(ud)
    }

    #[inline]
    pub fn alloc_const_def(&mut self, cd: crate::ast::ConstDefinition) -> ConstDefId {
        self.const_defs.push(cd)
    }

    // =============================================================================
    // Additional Definition Allocators
    // =============================================================================

    #[inline]
    pub fn alloc_pattern_def(&mut self, pd: crate::ast::PatternDefinition) -> PatternDefId {
        self.pattern_defs.push(pd)
    }

    #[inline]
    pub fn alloc_strategy_def(&mut self, sd: crate::ast::StrategyDefinition) -> StrategyDefId {
        self.strategy_defs.push(sd)
    }

    #[inline]
    pub fn alloc_signal_group_def(
        &mut self,
        sgd: crate::ast::SignalGroupDefinition,
    ) -> SignalGroupDefId {
        self.signal_group_defs.push(sgd)
    }

    #[inline]
    pub fn alloc_material_alias_def(
        &mut self,
        mad: crate::ast::MaterialAliasDefinition,
    ) -> MaterialAliasDefId {
        self.material_alias_defs.push(mad)
    }

    #[inline]
    pub fn alloc_enum_def(&mut self, ed: crate::ast::EnumDefinition) -> EnumDefId {
        self.enum_defs.push(ed)
    }

    #[inline]
    pub fn alloc_struct_def(&mut self, sd: crate::ast::StructDefinition) -> StructDefId {
        self.struct_defs.push(sd)
    }

    #[inline]
    pub fn alloc_logic_def(&mut self, ld: crate::ast::LogicDefinition) -> LogicDefId {
        self.logic_defs.push(ld)
    }

    #[inline]
    pub fn alloc_shape_def(&mut self, sd: crate::ast::ShapeDefinition) -> ShapeDefId {
        self.shape_defs.push(sd)
    }

    #[inline]
    pub fn alloc_spice_model_def(
        &mut self,
        smd: crate::ast::SpiceModelDefinition,
    ) -> SpiceModelDefId {
        self.spice_model_defs.push(smd)
    }

    #[inline]
    pub fn alloc_subcircuit_def(
        &mut self,
        scd: crate::ast::SubcircuitDefinition,
    ) -> SubcircuitDefId {
        self.subcircuit_defs.push(scd)
    }

    #[inline]
    pub fn alloc_polymorphic_interface_def(
        &mut self,
        pid: crate::ast::PolymorphicInterfaceDefinition,
    ) -> PolymorphicInterfaceDefId {
        self.polymorphic_interface_defs.push(pid)
    }

    /// Clear all arena contents (useful for reusing arena between parses)
    pub fn clear(&mut self) {
        self.components.clear();
        self.pours.clear();
        self.planes.clear();
        self.polygons.clear();
        self.contacts.clear();
        self.routes.clear();
        self.space_instances.clear();
        self.for_loops.clear();
        self.regions.clear();
        self.substrates.clear();
        self.module_components.clear();
        self.module_internals.clear();

        // Clear all definition types
        self.component_defs.clear();
        self.material_defs.clear();
        self.module_defs.clear();
        self.profile_defs.clear();
        self.space_defs.clear();
        self.bridge_defs.clear();
        self.mechanical_defs.clear();
        self.interface_defs.clear();
        self.test_defs.clear();
        self.device_defs.clear();
        self.unit_defs.clear();
        self.const_defs.clear();

        // Clear additional definition types
        self.pattern_defs.clear();
        self.strategy_defs.clear();
        self.signal_group_defs.clear();
        self.material_alias_defs.clear();
        self.enum_defs.clear();
        self.struct_defs.clear();
        self.logic_defs.clear();
        self.shape_defs.clear();
        self.spice_model_defs.clear();
        self.subcircuit_defs.clear();
        self.polymorphic_interface_defs.clear();
    }

    /// Get total memory usage estimate in bytes
    pub fn memory_usage(&self) -> usize {
        self.components.len() * std::mem::size_of::<ComponentPlacement>()
            + self.pours.len() * std::mem::size_of::<PourPlacement>()
            + self.planes.len() * std::mem::size_of::<PlanePlacement>()
            + self.polygons.len() * std::mem::size_of::<PolygonPlacement>()
            + self.contacts.len() * std::mem::size_of::<ContactPlacement>()
            + self.routes.len() * std::mem::size_of::<Route>()
            + self.space_instances.len() * std::mem::size_of::<SpaceInstancePlacement>()
            + self.for_loops.len() * std::mem::size_of::<SpaceForLoop>()
            + self.regions.len() * std::mem::size_of::<RegionDefinition>()
            + self.substrates.len() * std::mem::size_of::<SubstratePlacement>()
            + self.module_components.len() * std::mem::size_of::<ModuleComponentPlacement>()
            + self.module_internals.len() * std::mem::size_of::<ModuleInternalPlacement>()
            + self.component_defs.len() * std::mem::size_of::<ComponentDefinition>()
    }
}
