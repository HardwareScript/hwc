use crate::commands::build_cmd::BuildConfig;
use hwc_engine::HardwareSpace;
use hwc_physics::error_mapping::PhysicsError;
use miette::Result;
use std::time::Instant;

pub mod continuity;
pub mod drc;
pub mod utils;

/// Validation result (Task 5.1: Phantom Buffer)
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: bool,
    pub violation_count: usize,
    pub violations: Vec<PhysicsError>,
}

impl ValidationResult {
    pub fn success() -> Self {
        Self {
            passed: true,
            violation_count: 0,
            violations: Vec::new(),
        }
    }

    pub fn failed(violations: Vec<PhysicsError>) -> Self {
        Self {
            passed: false,
            violation_count: violations.len(),
            violations,
        }
    }
}

/// Run all validation checks (DRC and connectivity)
/// Returns ValidationResult instead of Result to support Commit Gate
pub fn run_validation_checks(
    space: &HardwareSpace,
    config: &BuildConfig,
    is_artist_mode: bool,
    start_time: Instant,
) -> Result<ValidationResult> {
    let mut all_violations = Vec::new();

    // Run DRC if enabled (requires Professional Mode)
    if !config.skip_drc && !is_artist_mode {
        if let Err(e) = drc::run_drc_check(space, config, start_time) {
            // DRC errors are already formatted, wrap them as PhysicsError
            all_violations.push(PhysicsError::new("DRC", format!("DRC: {}", e).into()));
        }
    }

    // Physical continuity check (P41) replaces the old connectivity checker.
    // It validates all nets including route segments, substrate layers, pours, and contacts.
    if !config.skip_connectivity_check && !config.skip_physical_continuity && !is_artist_mode {
        let (physics_substrate_layers, physics_route_segments) =
            utils::convert_metadata_to_physics(space);
        match continuity::run_physical_continuity_check(
            space,
            &physics_substrate_layers,
            &physics_route_segments,
            config,
            start_time,
        ) {
            Ok(violations) => {
                all_violations.extend(violations);
            }
            Err(e) => {
                all_violations.push(PhysicsError::new(
                    "CONTINUITY",
                    format!("Continuity: {}", e).into(),
                ));
            }
        }
    }

    if all_violations.is_empty() {
        Ok(ValidationResult::success())
    } else {
        Ok(ValidationResult::failed(all_violations))
    }
}
