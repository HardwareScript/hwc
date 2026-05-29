/// Build command errors with miette diagnostics
///
/// Task 5.1: Phantom Buffer - Commit Gate Architecture
/// Restored to use proper miette formatting instead of custom box-drawing

use hwc_physics::error_mapping::PhysicsError;
use miette::Diagnostic;
use thiserror::Error;

/// Build errors for the Commit Gate architecture
#[derive(Debug, Error, Diagnostic)]
pub enum BuildError {
    /// Commit Gate closed - validation failed in Architecture Mode
    #[error("Physical integrity validation failed: {violation_count} violation(s) in Architecture Mode")]
    #[diagnostic(
        code(hwc::build::commit_gate_closed),
        help("{violations}\n\nOptions:\n  • Fix the violations listed above\n  • Use --skip-physical-continuity to bypass validation (debugging only)\n  • Use --force-export to override the gate (debugging only)\n  • Remove 'implements' keyword to switch to Artist Mode")
    )]
    CommitGateClosed {
        violation_count: usize,
        violations: String,
    },


}

impl BuildError {
    /// Create a CommitGateClosed error from validation results
    pub fn from_validation_failures(violations: &[PhysicsError]) -> Self {
        let violations_text = violations
            .iter()
            .map(|v| {
                let mut text = format!("  • [{}] {}", v.code, v.message);
                if let Some(ref suggestion) = v.suggestion {
                    text.push_str(&format!("\n    💡 {}", suggestion));
                }
                text
            })
            .collect::<Vec<_>>()
            .join("\n");
        
        Self::CommitGateClosed {
            violation_count: violations.len(),
            violations: violations_text,
        }
    }
}
