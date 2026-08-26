//! HardwareScript v0.3.0 Hermetic Sandbox & Step Counter

use super::context::EvalError;

/// Maximum allowed steps per space / program execution (Halting Problem guard)
pub const MAX_EVAL_STEPS: usize = 10_000_000;

/// Maximum allowed call stack recursion depth
pub const MAX_RECURSION_DEPTH: usize = 256;

/// Sandboxed execution limiter
#[derive(Debug, Clone)]
pub struct SandboxGuard {
    step_count: usize,
    max_steps: usize,
    max_depth: usize,
}

impl Default for SandboxGuard {
    fn default() -> Self {
        Self::new(MAX_EVAL_STEPS, MAX_RECURSION_DEPTH)
    }
}

impl SandboxGuard {
    pub fn new(max_steps: usize, max_depth: usize) -> Self {
        Self {
            step_count: 0,
            max_steps,
            max_depth,
        }
    }

    /// Increment step count and check against step limit
    #[inline(always)]
    pub fn tick(&mut self) -> Result<(), EvalError> {
        self.step_count += 1;
        if self.step_count > self.max_steps {
            Err(EvalError::StepLimitExceeded(self.max_steps))
        } else {
            Ok(())
        }
    }

    /// Check if current call stack depth exceeds recursion limit
    #[inline(always)]
    pub fn check_recursion_depth(&self, current_depth: usize) -> Result<(), EvalError> {
        if current_depth > self.max_depth {
            Err(EvalError::RecursionDepthExceeded(self.max_depth))
        } else {
            Ok(())
        }
    }

    /// Get current step count
    pub fn steps_executed(&self) -> usize {
        self.step_count
    }

    /// Reset step counter
    pub fn reset(&mut self) {
        self.step_count = 0;
    }
}
