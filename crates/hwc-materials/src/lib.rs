// SPDX-License-Identifier: AGPL-3.0-or-later WITH HardwareScript-Compiler-Output-Exception
// Copyright (C) 2024-2026 Olowookere Olamide and HardwareScript Contributors
//
// This file is part of the Hardware Script compiler (hwc).
//
// hwc is free software: you can redistribute it and/or modify it under the terms
// of the GNU Affero General Public License as published by the Free Software Foundation,
// either version 3 of the License, or (at your option) any later version, WITH the
// HardwareScript Compiler Output Exception.
//
// See LICENSE.md and COMPILER-OUTPUT-EXCEPTION.md in the repository root for details.

pub mod constraints;
pub mod database;
pub mod error_codes;
pub mod material;
pub mod routing_intent;
pub mod stackup;

pub use constraints::{
    BridgeRule, ClearanceConstraints, ConstraintError, ConstraintSet, LayerConstraints,
    RoutableMode, StackupConstraints, ThermalConstraints, TraceConstraints, ViaConstraints,
};
pub use database::{MaterialDatabase, MaterialError};
pub use material::{
    BiasRequirement, ConductorProperties, DopingType, InsulatorProperties, ManufacturingProcess,
    MaterialMetadata, NetClassification, SemiconductorProperties,
};
pub use routing_intent::{IntentCostWeights, RoutingIntent};
pub use stackup::{BoardSpecification, ImpedanceParameters, Layer, StackupError, StackupProfile};
