//! HardwareScript v0.3.1 Unified `Value` Model & 7-Base SI Picometer Dimensional Arithmetic
//!
//! Implements the canonical `Value` type, the 7-Base SI `SiDimension` vector,
//! pure `CellLayout` composition, and strict 128-bit integer dimensional algebra.

use hwc_types::SiDimension;

mod cell;
mod cell_layout;
mod ids;
mod measurement;
mod placed;
mod transform;
mod value_arithmetic;
mod value_core;
mod value_display;
mod value_enum;

pub use cell::{CellContact, CellDevice, CellPolygon, CellPort};
pub use cell_layout::CellLayout;
pub use ids::{DeviceId, FunctionId, SpaceId};
pub use measurement::MeasurementValue;
pub use placed::{PlacedCellInstance, PlacedPort};
pub use transform::Transform2D;
pub use value_enum::Value;

pub type UnitDimension = SiDimension;
pub type PhysicalDimension = SiDimension;
pub type PhysicalValue = MeasurementValue;
