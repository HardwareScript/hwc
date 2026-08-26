//! All ID type definitions for arena-allocated AST nodes
//!
//! Each ID type is a newtype wrapper around u32 that provides:
//! - Type safety (can't mix ComponentId with RouteId)
//! - Small size (4 bytes vs 8-byte pointers)
//! - Copy semantics (no lifetime management)
//! - Serialization support

use crate::define_id_type;

// =============================================================================
// Space Placement ID Types
// =============================================================================

define_id_type!(ComponentId);
define_id_type!(PourId);
define_id_type!(PlaneId);
define_id_type!(PolygonId);
define_id_type!(ContactId);
define_id_type!(RouteId);
define_id_type!(ForLoopId);
define_id_type!(RegionId);
define_id_type!(SubstrateId);

// =============================================================================
// Module Placement ID Types
// =============================================================================

define_id_type!(ModuleComponentId);
define_id_type!(ModuleInternalId);

// =============================================================================
// Top-Level Definition ID Types
// =============================================================================

define_id_type!(FunctionDefId);
define_id_type!(ComponentDefId);
define_id_type!(MaterialDefId);
define_id_type!(ModuleDefId);
define_id_type!(ProfileDefId);
define_id_type!(SpaceDefId);
define_id_type!(BridgeDefId);
define_id_type!(MechanicalDefId);
define_id_type!(InterfaceDefId);
define_id_type!(TestDefId);
define_id_type!(DeviceDefId);
define_id_type!(UnitDefId);
define_id_type!(ConstDefId);

// =============================================================================
// Additional Definition ID Types
// =============================================================================

define_id_type!(PatternDefId);
define_id_type!(StrategyDefId);
define_id_type!(SignalGroupDefId);
define_id_type!(MaterialAliasDefId);
define_id_type!(EnumDefId);
define_id_type!(StructDefId);
define_id_type!(LogicDefId);
define_id_type!(ShapeDefId);
define_id_type!(SpiceModelDefId);
define_id_type!(SubcircuitDefId);
define_id_type!(PolymorphicInterfaceDefId);
