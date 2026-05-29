pub mod constraints;
pub mod database;
pub mod error_codes;
pub mod material;
pub mod stackup;

pub use constraints::{
    BridgeRule, ClearanceConstraints, ConstraintError, ConstraintSet, LayerConstraints,
    StackupConstraints, ThermalConstraints, TraceConstraints, ViaConstraints,
};
pub use database::{MaterialDatabase, MaterialError};
pub use material::{
    BiasRequirement, ConductorProperties, DopingType, InsulatorProperties, ManufacturingProcess,
    MaterialMetadata, NetClassification, SemiconductorProperties,
};
pub use stackup::{BoardSpecification, ImpedanceParameters, Layer, StackupError, StackupProfile};
