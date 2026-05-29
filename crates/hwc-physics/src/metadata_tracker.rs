//! Metadata Dependency Tracker
//!
//! Tracks changes to material properties, profile settings, and constraints
//! that don't affect voxel positions but require re-validation of physics checks.
//!
//! # The Problem
//!
//! Dirty bits track voxel changes, but not material property changes.
//! If a user changes "Dielectric Strength" of FR4, no voxels moved, so dirty bits are 0.
//! The engine won't re-run voltage checks, leading to incorrect validation.
//!
//! # The Solution
//!
//! Hash-based metadata tracking. Store hashes of:
//! - Profile properties (process node, layer stack, thermal/electrical constraints)
//! - Material properties (dielectric strength, resistivity, thermal conductivity)
//! - Constraint properties (clearance rules, via costs, trace widths)
//!
//! On every compile, check if hashes changed. If yes, set global_dirty_flag
//! for affected physics passes and trigger full re-sweep.
//!
//! # Performance Target
//!
//! Hash check < 1 microsecond

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Metadata change flags indicating which physics passes need re-validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadataChangeFlags {
    /// Material properties changed (resistivity, dielectric strength, etc.)
    pub materials_changed: bool,

    /// Profile constraints changed (thermal, electrical, clearance)
    pub profile_changed: bool,

    /// Manufacturing constraints changed (copper thickness, IPC constants)
    pub manufacturing_changed: bool,

    /// Stackup/layer configuration changed
    pub stackup_changed: bool,
}

impl MetadataChangeFlags {
    /// Create new flags with all set to false
    pub fn none() -> Self {
        Self {
            materials_changed: false,
            profile_changed: false,
            manufacturing_changed: false,
            stackup_changed: false,
        }
    }

    /// Check if any metadata changed
    pub fn any_changed(&self) -> bool {
        self.materials_changed
            || self.profile_changed
            || self.manufacturing_changed
            || self.stackup_changed
    }

    /// Check if electrical validation needs re-run
    pub fn needs_electrical_revalidation(&self) -> bool {
        self.materials_changed || self.profile_changed || self.manufacturing_changed
    }

    /// Check if thermal validation needs re-run
    pub fn needs_thermal_revalidation(&self) -> bool {
        self.materials_changed || self.profile_changed || self.manufacturing_changed
    }

    /// Check if electromagnetic validation needs re-run
    pub fn needs_em_revalidation(&self) -> bool {
        self.materials_changed || self.profile_changed || self.stackup_changed
    }

    /// Check if clearance validation needs re-run
    pub fn needs_clearance_revalidation(&self) -> bool {
        self.profile_changed || self.stackup_changed
    }
}

/// Metadata hashes for tracking changes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MetadataHashes {
    materials_hash: u64,
    profile_hash: u64,
    manufacturing_hash: u64,
    stackup_hash: u64,
}

impl MetadataHashes {
    fn zero() -> Self {
        Self {
            materials_hash: 0,
            profile_hash: 0,
            manufacturing_hash: 0,
            stackup_hash: 0,
        }
    }
}

/// Metadata Dependency Tracker
///
/// Tracks changes to material properties, profile settings, and constraints.
/// Uses hash-based change detection to trigger selective re-validation.
///
/// # Usage
///
/// ```rust,ignore
/// let mut tracker = MetadataTracker::new();
///
/// // On every compile, check for metadata changes
/// let changes = tracker.check_metadata_changed(
///     &materials,
///     &profile,
///     &manufacturing,
///     &stackup,
/// );
///
/// if changes.needs_electrical_revalidation() {
///     // Re-run electrical validation
/// }
/// ```
pub struct MetadataTracker {
    /// Previous metadata hashes
    previous_hashes: MetadataHashes,
}

impl MetadataTracker {
    /// Create a new metadata tracker
    pub fn new() -> Self {
        Self {
            previous_hashes: MetadataHashes::zero(),
        }
    }

    /// Check if metadata changed since last compile
    ///
    /// Computes hashes of current metadata and compares with previous hashes.
    /// Returns flags indicating which physics passes need re-validation.
    ///
    /// # Performance
    ///
    /// Target: < 1 microsecond for hash computation and comparison
    ///
    /// # Arguments
    ///
    /// * `materials` - Material properties (conductors, insulators, semiconductors)
    /// * `profile` - Profile constraints (thermal, electrical, clearance)
    /// * `manufacturing` - Manufacturing constraints (copper thickness, IPC constants)
    /// * `stackup` - Stackup/layer configuration
    pub fn check_metadata_changed<M, P, F, S>(
        &mut self,
        materials: &M,
        profile: &P,
        manufacturing: &F,
        stackup: &S,
    ) -> MetadataChangeFlags
    where
        M: Hash,
        P: Hash,
        F: Hash,
        S: Hash,
    {
        // Compute current hashes
        let current_hashes = MetadataHashes {
            materials_hash: Self::compute_hash(materials),
            profile_hash: Self::compute_hash(profile),
            manufacturing_hash: Self::compute_hash(manufacturing),
            stackup_hash: Self::compute_hash(stackup),
        };

        // Compare with previous hashes
        let changes = MetadataChangeFlags {
            materials_changed: current_hashes.materials_hash != self.previous_hashes.materials_hash,
            profile_changed: current_hashes.profile_hash != self.previous_hashes.profile_hash,
            manufacturing_changed: current_hashes.manufacturing_hash
                != self.previous_hashes.manufacturing_hash,
            stackup_changed: current_hashes.stackup_hash != self.previous_hashes.stackup_hash,
        };

        // Update previous hashes
        self.previous_hashes = current_hashes;

        changes
    }

    /// Force re-validation of all physics passes
    ///
    /// Resets all hashes to zero, causing next check to report all metadata changed.
    pub fn force_revalidation(&mut self) {
        self.previous_hashes = MetadataHashes::zero();
    }

    /// Compute hash of a value
    fn compute_hash<T: Hash>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for MetadataTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Hash)]
    struct MockMaterials {
        copper_resistivity: u64,
        fr4_dielectric_strength: u64,
    }

    #[derive(Hash)]
    struct MockProfile {
        max_temp_rise: u64,
        max_voltage_drop: u64,
    }

    #[derive(Hash)]
    struct MockManufacturing {
        copper_thickness: u64,
        ipc_k_external: u64,
    }

    #[derive(Hash)]
    struct MockStackup {
        layer_count: u64,
        dielectric_height: u64,
    }

    #[test]
    fn test_no_changes_on_first_check() {
        let mut tracker = MetadataTracker::new();

        let materials = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 20,
        };
        let profile = MockProfile {
            max_temp_rise: 30,
            max_voltage_drop: 100,
        };
        let manufacturing = MockManufacturing {
            copper_thickness: 35000,
            ipc_k_external: 48,
        };
        let stackup = MockStackup {
            layer_count: 4,
            dielectric_height: 100000,
        };

        // First check should report changes (from zero state)
        let changes =
            tracker.check_metadata_changed(&materials, &profile, &manufacturing, &stackup);

        assert!(changes.any_changed());
    }

    #[test]
    fn test_no_changes_on_second_check() {
        let mut tracker = MetadataTracker::new();

        let materials = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 20,
        };
        let profile = MockProfile {
            max_temp_rise: 30,
            max_voltage_drop: 100,
        };
        let manufacturing = MockManufacturing {
            copper_thickness: 35000,
            ipc_k_external: 48,
        };
        let stackup = MockStackup {
            layer_count: 4,
            dielectric_height: 100000,
        };

        // First check
        tracker.check_metadata_changed(&materials, &profile, &manufacturing, &stackup);

        // Second check with same data should report no changes
        let changes =
            tracker.check_metadata_changed(&materials, &profile, &manufacturing, &stackup);

        assert!(!changes.any_changed());
    }

    #[test]
    fn test_material_change_detection() {
        let mut tracker = MetadataTracker::new();

        let materials1 = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 20,
        };
        let profile = MockProfile {
            max_temp_rise: 30,
            max_voltage_drop: 100,
        };
        let manufacturing = MockManufacturing {
            copper_thickness: 35000,
            ipc_k_external: 48,
        };
        let stackup = MockStackup {
            layer_count: 4,
            dielectric_height: 100000,
        };

        // First check
        tracker.check_metadata_changed(&materials1, &profile, &manufacturing, &stackup);

        // Change material property
        let materials2 = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 25, // Changed!
        };

        let changes =
            tracker.check_metadata_changed(&materials2, &profile, &manufacturing, &stackup);

        assert!(changes.materials_changed);
        assert!(!changes.profile_changed);
        assert!(!changes.manufacturing_changed);
        assert!(!changes.stackup_changed);
    }

    #[test]
    fn test_profile_change_detection() {
        let mut tracker = MetadataTracker::new();

        let materials = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 20,
        };
        let profile1 = MockProfile {
            max_temp_rise: 30,
            max_voltage_drop: 100,
        };
        let manufacturing = MockManufacturing {
            copper_thickness: 35000,
            ipc_k_external: 48,
        };
        let stackup = MockStackup {
            layer_count: 4,
            dielectric_height: 100000,
        };

        // First check
        tracker.check_metadata_changed(&materials, &profile1, &manufacturing, &stackup);

        // Change profile property
        let profile2 = MockProfile {
            max_temp_rise: 40, // Changed!
            max_voltage_drop: 100,
        };

        let changes =
            tracker.check_metadata_changed(&materials, &profile2, &manufacturing, &stackup);

        assert!(!changes.materials_changed);
        assert!(changes.profile_changed);
        assert!(!changes.manufacturing_changed);
        assert!(!changes.stackup_changed);
    }

    #[test]
    fn test_manufacturing_change_detection() {
        let mut tracker = MetadataTracker::new();

        let materials = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 20,
        };
        let profile = MockProfile {
            max_temp_rise: 30,
            max_voltage_drop: 100,
        };
        let manufacturing1 = MockManufacturing {
            copper_thickness: 35000,
            ipc_k_external: 48,
        };
        let stackup = MockStackup {
            layer_count: 4,
            dielectric_height: 100000,
        };

        // First check
        tracker.check_metadata_changed(&materials, &profile, &manufacturing1, &stackup);

        // Change manufacturing property
        let manufacturing2 = MockManufacturing {
            copper_thickness: 70000, // Changed to 2oz copper!
            ipc_k_external: 48,
        };

        let changes =
            tracker.check_metadata_changed(&materials, &profile, &manufacturing2, &stackup);

        assert!(!changes.materials_changed);
        assert!(!changes.profile_changed);
        assert!(changes.manufacturing_changed);
        assert!(!changes.stackup_changed);
    }

    #[test]
    fn test_stackup_change_detection() {
        let mut tracker = MetadataTracker::new();

        let materials = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 20,
        };
        let profile = MockProfile {
            max_temp_rise: 30,
            max_voltage_drop: 100,
        };
        let manufacturing = MockManufacturing {
            copper_thickness: 35000,
            ipc_k_external: 48,
        };
        let stackup1 = MockStackup {
            layer_count: 4,
            dielectric_height: 100000,
        };

        // First check
        tracker.check_metadata_changed(&materials, &profile, &manufacturing, &stackup1);

        // Change stackup property
        let stackup2 = MockStackup {
            layer_count: 6, // Changed to 6-layer board!
            dielectric_height: 100000,
        };

        let changes =
            tracker.check_metadata_changed(&materials, &profile, &manufacturing, &stackup2);

        assert!(!changes.materials_changed);
        assert!(!changes.profile_changed);
        assert!(!changes.manufacturing_changed);
        assert!(changes.stackup_changed);
    }

    #[test]
    fn test_multiple_changes() {
        let mut tracker = MetadataTracker::new();

        let materials1 = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 20,
        };
        let profile1 = MockProfile {
            max_temp_rise: 30,
            max_voltage_drop: 100,
        };
        let manufacturing = MockManufacturing {
            copper_thickness: 35000,
            ipc_k_external: 48,
        };
        let stackup = MockStackup {
            layer_count: 4,
            dielectric_height: 100000,
        };

        // First check
        tracker.check_metadata_changed(&materials1, &profile1, &manufacturing, &stackup);

        // Change multiple properties
        let materials2 = MockMaterials {
            copper_resistivity: 1700, // Changed!
            fr4_dielectric_strength: 20,
        };
        let profile2 = MockProfile {
            max_temp_rise: 35, // Changed!
            max_voltage_drop: 100,
        };

        let changes =
            tracker.check_metadata_changed(&materials2, &profile2, &manufacturing, &stackup);

        assert!(changes.materials_changed);
        assert!(changes.profile_changed);
        assert!(!changes.manufacturing_changed);
        assert!(!changes.stackup_changed);
    }

    #[test]
    fn test_revalidation_flags() {
        let changes = MetadataChangeFlags {
            materials_changed: true,
            profile_changed: false,
            manufacturing_changed: false,
            stackup_changed: false,
        };

        // Material changes affect electrical and thermal
        assert!(changes.needs_electrical_revalidation());
        assert!(changes.needs_thermal_revalidation());
        assert!(changes.needs_em_revalidation());
        assert!(!changes.needs_clearance_revalidation());
    }

    #[test]
    fn test_force_revalidation() {
        let mut tracker = MetadataTracker::new();

        let materials = MockMaterials {
            copper_resistivity: 1678,
            fr4_dielectric_strength: 20,
        };
        let profile = MockProfile {
            max_temp_rise: 30,
            max_voltage_drop: 100,
        };
        let manufacturing = MockManufacturing {
            copper_thickness: 35000,
            ipc_k_external: 48,
        };
        let stackup = MockStackup {
            layer_count: 4,
            dielectric_height: 100000,
        };

        // First check
        tracker.check_metadata_changed(&materials, &profile, &manufacturing, &stackup);

        // Second check should report no changes
        let changes =
            tracker.check_metadata_changed(&materials, &profile, &manufacturing, &stackup);
        assert!(!changes.any_changed());

        // Force revalidation
        tracker.force_revalidation();

        // Next check should report all changed
        let changes =
            tracker.check_metadata_changed(&materials, &profile, &manufacturing, &stackup);
        assert!(changes.any_changed());
    }
}
