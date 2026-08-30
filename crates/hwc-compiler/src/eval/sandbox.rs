//! Deterministic execution guard and host memory safety monitor for hwc-eval (Phase 2).
//!
//! Replaces legacy arbitrary step ceilings with deterministic fuel budgeting
//! and host RAM allocation tracking (2 GB default quota).

use miette::Diagnostic;
use thiserror::Error;

/// Standard base fuel budget for any module / evaluation context.
pub const DEFAULT_BASE_FUEL: u64 = 100_000_000;

/// Maximum allowed call stack recursion depth.
pub const MAX_CALL_STACK_DEPTH: usize = 256;

/// Default host memory allocation quota (2 GB).
pub const DEFAULT_MAX_MEMORY_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Fuel scaling per 1 mm^2 of physical space area (10M fuel / mm^2).
pub const FUEL_PER_MM2: u64 = 10_000_000;

/// Number of square picometers in 1 mm^2: (10^9 pm)^2 = 10^18 pm^2.
const PM2_PER_MM2: f64 = 1.0e18;

/// Diagnostic errors emitted by the deterministic evaluation sandbox.
#[derive(Error, Diagnostic, Debug, Clone, PartialEq)]
pub enum SandboxError {
    #[error("Comptime Evaluation Fuel Exhausted: executed {fuel_consumed} instructions")]
    #[diagnostic(
        code(C01),
        help("A potential infinite loop was intercepted. If this large array synthesis is intentional, increase the budget using '#[comptime_fuel({suggested_fuel})]' on the space declaration.")
    )]
    FuelExhausted {
        fuel_consumed: u64,
        suggested_fuel: u64,
    },

    #[error("Recursion depth limit exceeded (Maximum {max_depth} stack frames)")]
    #[diagnostic(
        code(C02),
        help("Comptime layout generators cannot recurse deeper than 256 frames. Convert recursive generators to iterative loops.")
    )]
    RecursionDepthExceeded { max_depth: usize },

    #[error("Memory quota exceeded: Comptime evaluation allocated {allocated_mb} MB (Quota limit: {limit_mb} MB)")]
    #[diagnostic(
        code(C03),
        help("The design exceeded the maximum allowed memory footprint. Check for unbounded array growth or infinite collection allocation.")
    )]
    MemoryLimitExceeded {
        allocated_mb: usize,
        limit_mb: usize,
    },
}

/// Computes the total deterministic fuel budget for a space given optional
/// physical dimensions in picometers and optional explicit `#[comptime_fuel]` attribute.
pub fn calculate_fuel(
    width_pm: Option<i128>,
    height_pm: Option<i128>,
    explicit_fuel: Option<i64>,
) -> u64 {
    let mut total_fuel = DEFAULT_BASE_FUEL;

    // Fuel_Area = (Width_pm * Height_pm / 10^18 pm^2) * 10_000_000
    if let (Some(w), Some(h)) = (width_pm, height_pm) {
        if w > 0 && h > 0 {
            let area_pm2 = (w as f64) * (h as f64);
            let mm2 = area_pm2 / PM2_PER_MM2;
            let area_fuel = (mm2 * (FUEL_PER_MM2 as f64)) as u64;
            total_fuel = total_fuel.saturating_add(area_fuel);
        }
    }

    if let Some(explicit) = explicit_fuel {
        if explicit > 0 {
            total_fuel = total_fuel.saturating_add(explicit as u64);
        }
    }

    total_fuel
}

/// Deterministic Execution & Memory Safety Monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct DeterministicGuard {
    pub fuel_remaining: u64,
    pub total_fuel_budget: u64,
    pub allocated_bytes: usize,
    pub max_memory_bytes: usize,
}

impl Default for DeterministicGuard {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_FUEL, DEFAULT_MAX_MEMORY_BYTES)
    }
}

impl DeterministicGuard {
    pub fn new(fuel_budget: u64, max_memory_bytes: usize) -> Self {
        Self {
            fuel_remaining: fuel_budget,
            total_fuel_budget: fuel_budget,
            allocated_bytes: 0,
            max_memory_bytes,
        }
    }

    /// Creates a guard with default memory quota for a given fuel budget.
    pub fn with_fuel(fuel_budget: u64) -> Self {
        Self::new(fuel_budget, DEFAULT_MAX_MEMORY_BYTES)
    }

    /// Decrements deterministic instruction fuel by 1.
    #[inline(always)]
    pub fn consume_step(&mut self) -> Result<(), SandboxError> {
        if self.fuel_remaining == 0 {
            return Err(SandboxError::FuelExhausted {
                fuel_consumed: self.total_fuel_budget,
                suggested_fuel: self.total_fuel_budget.saturating_mul(2),
            });
        }
        self.fuel_remaining -= 1;
        Ok(())
    }

    /// Tracks heap / buffer memory allocation.
    #[inline(always)]
    pub fn track_allocation(&mut self, bytes: usize) -> Result<(), SandboxError> {
        self.allocated_bytes = self.allocated_bytes.saturating_add(bytes);
        if self.allocated_bytes > self.max_memory_bytes {
            return Err(SandboxError::MemoryLimitExceeded {
                allocated_mb: self.allocated_bytes / (1024 * 1024),
                limit_mb: self.max_memory_bytes / (1024 * 1024),
            });
        }
        Ok(())
    }

    /// Tracks deallocation of memory.
    #[inline(always)]
    pub fn track_deallocation(&mut self, bytes: usize) {
        self.allocated_bytes = self.allocated_bytes.saturating_sub(bytes);
    }

    /// Checks if current call stack depth exceeds recursion limit.
    #[inline(always)]
    pub fn check_recursion_depth(&self, current_depth: usize) -> Result<(), SandboxError> {
        if current_depth > MAX_CALL_STACK_DEPTH {
            Err(SandboxError::RecursionDepthExceeded {
                max_depth: MAX_CALL_STACK_DEPTH,
            })
        } else {
            Ok(())
        }
    }

    /// Returns the total fuel consumed so far.
    pub fn fuel_consumed(&self) -> u64 {
        self.total_fuel_budget.saturating_sub(self.fuel_remaining)
    }
}
