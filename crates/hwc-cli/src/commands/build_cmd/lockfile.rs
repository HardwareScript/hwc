//! Route lockfile support (legacy module - logic moved to hwc-compiler ir/mod.rs)
//!
//! The lockfile loading and saving is now handled inside `compile_single_space()`
//! in the compiler crate, where it has direct access to the HardwareSpace before
//! and after routing.
