//! Real-Time Static Timing Analysis (STA) using Elmore Delay
//!
//! This module provides fast first-order timing analysis for Hardware Script designs.
//! Instead of running full SPICE simulation (which takes seconds for large designs),
//! we use the Elmore delay model to estimate signal propagation delays in < 1ms.
//!
//! # The Problem
//! RCX extracts parasitics (R and C), but running full SPICE on billion gates takes
//! seconds. Users need instant timing feedback in the IDE as they move wires.
//!
//! # The Solution
//! Elmore delay provides a first-order approximation of signal delay using extracted
//! parasitics. It's fast enough for real-time IDE feedback while being accurate enough
//! for early design validation.
//!
//! # Elmore Delay Formula
//! For an RC tree, the delay from source to sink is:
//! ```text
//! τ = Σ(R_i × C_downstream_i)
//! ```
//! where:
//! - R_i is the resistance of segment i
//! - C_downstream_i is the total capacitance downstream of segment i
//!
//! # Performance Target
//! Calculate timing for 1000 nets in < 1ms (< 1μs per net)

use crate::parasitic::ParasiticValues;
use compact_str::CompactString;

/// Timing constraint for a net
#[derive(Debug, Clone)]
pub struct TimingConstraint {
    /// Net name
    pub net_name: CompactString,
    /// Maximum allowed delay in picoseconds
    pub max_delay_ps: f64,
    /// Minimum required delay in picoseconds (for hold time)
    pub min_delay_ps: Option<f64>,
}

/// Timing analysis result for a net
#[derive(Debug, Clone)]
pub struct TimingResult {
    /// Net name
    pub net_name: CompactString,
    /// Calculated Elmore delay in picoseconds
    pub elmore_delay_ps: f64,
    /// Timing slack in picoseconds (positive = meets timing, negative = violation)
    pub slack_ps: f64,
    /// Whether the net meets timing constraints
    pub meets_timing: bool,
}

/// Timing violation
#[derive(Debug, Clone)]
pub enum TimingViolation {
    /// Setup time violation (delay too long)
    SetupViolation {
        net: CompactString,
        actual_delay_ps: f64,
        max_delay_ps: f64,
        slack_ps: f64,
    },
    /// Hold time violation (delay too short)
    HoldViolation {
        net: CompactString,
        actual_delay_ps: f64,
        min_delay_ps: f64,
        slack_ps: f64,
    },
}

/// Real-time timing analyzer using Elmore delay model
///
/// This provides O(N) timing analysis where N is the number of trace segments.
/// For typical designs, this is fast enough for real-time IDE feedback.
#[derive(Default)]
pub struct TimingAnalyzer {
    // Analyzer state
}

impl TimingAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    /// Calculate Elmore delay for a single trace segment
    ///
    /// For a simple RC segment, the Elmore delay is:
    /// τ = R × C_downstream
    ///
    /// # Arguments
    /// * `resistance_ohm` - Segment resistance in ohms
    /// * `capacitance_downstream_pf` - Total capacitance downstream in picofarads
    ///
    /// # Returns
    /// Delay in picoseconds
    ///
    /// # Formula
    /// τ (ps) = R (Ω) × C (pF) × 1000
    /// (The factor of 1000 converts from ns to ps, since R×C gives ns)
    #[inline]
    pub fn calculate_segment_delay(
        &self,
        resistance_ohm: f64,
        capacitance_downstream_pf: f64,
    ) -> f64 {
        // τ = R × C
        // R in Ω, C in pF → delay in ns
        // Convert to ps by multiplying by 1000
        resistance_ohm * capacitance_downstream_pf * 1000.0
    }

    /// Calculate Elmore delay for a trace with multiple segments
    ///
    /// For an RC tree, the total delay is the sum of individual segment delays:
    /// τ_total = Σ(R_i × C_downstream_i)
    ///
    /// # Arguments
    /// * `segments` - Vector of (resistance, capacitance_downstream) pairs
    ///
    /// # Returns
    /// Total Elmore delay in picoseconds
    ///
    /// # Performance
    /// O(N) where N is the number of segments
    pub fn calculate_elmore_delay(&self, segments: &[(f64, f64)]) -> f64 {
        segments
            .iter()
            .map(|(r, c)| self.calculate_segment_delay(*r, *c))
            .sum()
    }

    /// Calculate Elmore delay from parasitic values
    ///
    /// This is a simplified version that treats the entire trace as a single segment.
    /// For more accurate results, the trace should be divided into multiple segments.
    ///
    /// # Arguments
    /// * `parasitics` - Extracted parasitic values (R and C)
    ///
    /// # Returns
    /// Elmore delay in picoseconds
    pub fn calculate_delay_from_parasitics(&self, parasitics: &ParasiticValues) -> f64 {
        // For a single segment, C_downstream = total capacitance
        self.calculate_segment_delay(parasitics.resistance_ohm, parasitics.capacitance_pf)
    }

    /// Analyze timing for a net with constraints
    ///
    /// # Arguments
    /// * `net_name` - Name of the net
    /// * `elmore_delay_ps` - Calculated Elmore delay in picoseconds
    /// * `constraint` - Timing constraint for the net
    ///
    /// # Returns
    /// Timing analysis result with slack calculation
    pub fn analyze_timing(
        &self,
        net_name: impl Into<String>,
        elmore_delay_ps: f64,
        constraint: &TimingConstraint,
    ) -> TimingResult {
        let net_name = net_name.into();

        // Calculate slack: slack = max_delay - actual_delay
        // Positive slack = meets timing
        // Negative slack = timing violation
        let slack_ps = constraint.max_delay_ps - elmore_delay_ps;
        let meets_timing = slack_ps >= 0.0;

        TimingResult {
            net_name: net_name.into(),
            elmore_delay_ps,
            slack_ps,
            meets_timing,
        }
    }

    /// Validate timing against constraints
    ///
    /// # Arguments
    /// * `net_name` - Name of the net
    /// * `elmore_delay_ps` - Calculated Elmore delay in picoseconds
    /// * `constraint` - Timing constraint for the net
    ///
    /// # Returns
    /// Ok if timing is met, Err with violation details otherwise
    pub fn validate_timing(
        &self,
        net_name: impl Into<String>,
        elmore_delay_ps: f64,
        constraint: &TimingConstraint,
    ) -> Result<(), TimingViolation> {
        let net_name = net_name.into();

        // Check setup time (maximum delay)
        if elmore_delay_ps > constraint.max_delay_ps {
            let slack_ps = constraint.max_delay_ps - elmore_delay_ps;
            return Err(TimingViolation::SetupViolation {
                net: net_name.into(),
                actual_delay_ps: elmore_delay_ps,
                max_delay_ps: constraint.max_delay_ps,
                slack_ps,
            });
        }

        // Check hold time (minimum delay) if specified
        if let Some(min_delay_ps) = constraint.min_delay_ps {
            if elmore_delay_ps < min_delay_ps {
                let slack_ps = elmore_delay_ps - min_delay_ps;
                return Err(TimingViolation::HoldViolation {
                    net: net_name.into(),
                    actual_delay_ps: elmore_delay_ps,
                    min_delay_ps,
                    slack_ps,
                });
            }
        }

        Ok(())
    }

    /// Batch timing analysis for multiple nets
    ///
    /// This is optimized for analyzing many nets at once, which is common
    /// during incremental compilation or real-time IDE updates.
    ///
    /// # Arguments
    /// * `nets` - Vector of (net_name, parasitics, constraint) tuples
    ///
    /// # Returns
    /// Vector of timing results
    ///
    /// # Performance
    /// O(N) where N is the total number of nets
    pub fn analyze_batch(
        &self,
        nets: &[(String, ParasiticValues, TimingConstraint)],
    ) -> Vec<TimingResult> {
        nets.iter()
            .map(|(name, parasitics, constraint)| {
                let delay = self.calculate_delay_from_parasitics(parasitics);
                self.analyze_timing(name, delay, constraint)
            })
            .collect()
    }

    /// Get all timing violations from a batch analysis
    ///
    /// # Arguments
    /// * `results` - Timing analysis results
    ///
    /// # Returns
    /// Vector of nets that violate timing constraints
    pub fn get_violations<'a>(&self, results: &'a [TimingResult]) -> Vec<&'a TimingResult> {
        results.iter().filter(|r| !r.meets_timing).collect()
    }

    /// Calculate worst-case slack across all nets
    ///
    /// # Arguments
    /// * `results` - Timing analysis results
    ///
    /// # Returns
    /// Worst (most negative) slack in picoseconds
    pub fn worst_slack(&self, results: &[TimingResult]) -> f64 {
        results
            .iter()
            .map(|r| r.slack_ps)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Calculate total negative slack (TNS)
    ///
    /// TNS is the sum of all negative slacks, indicating overall timing health.
    ///
    /// # Arguments
    /// * `results` - Timing analysis results
    ///
    /// # Returns
    /// Total negative slack in picoseconds
    pub fn total_negative_slack(&self, results: &[TimingResult]) -> f64 {
        results
            .iter()
            .filter(|r| r.slack_ps < 0.0)
            .map(|r| r.slack_ps)
            .sum()
    }

    /// Count failing paths
    ///
    /// # Arguments
    /// * `results` - Timing analysis results
    ///
    /// # Returns
    /// Number of nets that violate timing constraints
    pub fn failing_paths_count(&self, results: &[TimingResult]) -> usize {
        results.iter().filter(|r| !r.meets_timing).count()
    }
}
