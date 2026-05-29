//! Progressive Alignment Validation Module
//!
//! This module implements "Progressive Alignment" (GAP 7) - the killer feature
//! that makes Hardware Script foundry-ready.
//!
//! # Philosophy: Aligning Physical Art with Logical Intent
//!
//! Alignment validation is **optional** and **progressive**:
//! - **Artist Mode** (default): No `implements` clause → Skip alignment, full creative freedom
//! - **Professional Mode**: `implements ModuleName` → Enforce alignment, halt on mismatch
//!
//! # The Hardware Script Way
//!
//! Traditional EDA tools separate "schematic" and "layout" into different files,
//! requiring external LVS tools to verify they match. Hardware Script unifies them:
//!
//! ```hardware
//! module Inverter_Logic:
//!     input: VIN
//!     output: VOUT
//!     power: VDD
//!     ground: GND
//!     
//!     M1: NMOS(drain: VOUT, gate: VIN, source: GND, bulk: GND)
//!     M2: PMOS(drain: VOUT, gate: VIN, source: VDD, bulk: VDD)
//!
//! space CMOS_Inverter implements Inverter_Logic:  # ← Triggers alignment validation
//!     dimensions: 2mm by 1.5mm by 1mm
//!     grid: 100nm
//!     # ... physical geometry ...
//! ```
//!
//! # Architecture
//!
//! The alignment engine uses SPICE as the common comparison format:
//! 1. **Physical Netlist**: Extracted from geometry (Device Extractor)
//! 2. **Logical Netlist**: Synthesized from module definition (Logical Synthesizer)
//! 3. **Comparison**: Graph isomorphism between the two netlists (Graph Matcher)
//!
//! # Implementation Status
//!
//! ## Phase 1: Electrical Truth (SPICE as Common Format)
//! - [x] Physical netlist data structures
//! - [ ] Logical netlist data structures
//! - [ ] Logical synthesizer implementation
//! - [ ] Net naming standardization
//!
//! ## Phase 2: Graph Isomorphism (The Brain)
//! - [ ] Graph matcher implementation
//! - [ ] Device count verification
//! - [ ] Device type verification
//! - [ ] Connectivity verification
//! - [ ] Port mapping verification
//! - [ ] Parameter checking (W/L tolerance)
//!
//! ## Phase 3: Progressive Trigger (`implements` Keyword)
//! - [ ] Parser support for `implements` clause
//! - [ ] Artist Mode implementation
//! - [ ] Professional Mode implementation
//!
//! ## Phase 4: Actionable Errors
//! - [ ] Alignment error diagnostics
//! - [ ] Spatial highlighting
//!
//! ## Phase 5: Export Guard
//! - [ ] Integration with compilation pipeline
//! - [ ] Export prevention on alignment failure

pub mod error;
pub mod graph_matcher;
pub mod logical_synthesizer;
pub mod netlist;
pub mod validator;

pub use error::AlignmentError;
pub use graph_matcher::GraphMatcher;
pub use logical_synthesizer::LogicalSynthesizer;
pub use netlist::{
    DeviceTypeId, DeviceTypeRegistry, LogicalDevice, LogicalNetlist, NetInfo, PhysicalDevice,
    PhysicalNetlist, PortDirection, PortInfo,
};
pub use validator::{AlignmentResult, AlignmentValidator};
