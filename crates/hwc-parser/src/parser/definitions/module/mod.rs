//! Module definition parsing (pins, components, routes, control flow)
//!
//! This module is organized into logical submodules:
//! - `main`: Main module definition parsing and error recovery
//! - `pins`: Pin and role declaration parsing
//! - `statements`: Component addition (add) and routing (route)
//! - `control_flow`: Loops (for) and conditionals (if)
//! - `expressions`: Array indexing and conditions

mod control_flow;
mod expressions;
mod main;
mod pins;
mod statements;
