//! Strongly-Typed Physical Layer Classification
//!
//! **Architectural Law: ZERO HEURISTICS**
//! Layer classification is derived PURELY from MaterialCategory (declared in .hw files).
//! NO string matching, NO name inspection, NO guessing.
//!
//! # Design Philosophy
//!
//! Every material already declares its category:
//! ```hw
//! material N_Plus_Implant_Mask:
//!     category: mask  # ← MaterialCategory::Mask
//!
//! material Silicon_Dioxide:
//!     category: insulator  # ← MaterialCategory::Insulator
//!
//! material N_Plus_Diffusion:
//!     category: semiconductor  # ← MaterialCategory::Semiconductor
//!
//! material Aluminum:
//!     category: conductor  # ← MaterialCategory::Conductor
//! ```
//!
//! The compiler maps MaterialCategory → LayerKind → RoutingSurfacePolicy without
//! inspecting layer names like "nsdm", "poly", or "metal1" AT ALL.

use hwc_parser::MaterialCategory;
use serde::{Deserialize, Serialize};

use super::routing_surface::RoutingSurfacePolicy;

/// **Physical layer behavior derived DIRECTLY from typed MaterialCategory**
///
/// This enum represents the PHYSICAL behavior of a stackup layer in the
/// compilation and routing pipeline. It is derived purely from the material's
/// category declaration, with NO string matching or layer name inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerKind {
    /// 2D Lithographic Mask (from MaterialCategory::Mask)
    ///
    /// Pure 2D fabrication instruction; 0nm physical thickness.
    /// Used for ion implants (NSDM, PSDM), oxide windows (TAP), silicide block (RPM).
    ///
    /// Routing Policy: NonRoutable
    /// Physical Z: Anchor plane only (no volume)
    /// 3D Mesh: Excluded (zero thickness)
    LithoMask,

    /// Dielectric / Insulating Substrate (from MaterialCategory::Insulator)
    ///
    /// Provides mechanical support and capacitive isolation.
    /// Examples: SiO₂, Si₃N₄, low-k dielectrics, FR4, polyimide.
    ///
    /// Routing Policy: NonRoutable
    /// Physical Z: Full volumetric layer
    /// 3D Mesh: Included (cuts via holes)
    Dielectric,

    /// Semiconductor Active Bed (from MaterialCategory::Semiconductor)
    ///
    /// Planar wafer surface where channels and junctions are formed.
    /// Examples: N+ diffusion, P+ diffusion, intrinsic silicon.
    ///
    /// Routing Policy: SurfaceTop (contacts land from above)
    /// Physical Z: Bulk layer with active top surface
    /// 3D Mesh: Included (conductive regions)
    SemiconductorActive,

    /// Conductive Interconnect / Electrode (from MaterialCategory::Conductor)
    ///
    /// Metal planes, polysilicon gates, silicide local interconnect.
    /// Examples: Aluminum, Copper, Polysilicon, Tungsten.
    ///
    /// Routing Policy: LayerCenterline (vias land from either direction)
    /// Physical Z: Bulk conductor layer
    /// 3D Mesh: Included (primary routing medium)
    ConductiveInterconnect,
}

impl LayerKind {
    /// **PURE DERIVATION FROM MATERIAL'S STRONGLY-TYPED CATEGORY**
    ///
    /// # Architectural Law: Zero String Matching
    ///
    /// This function maps MaterialCategory → LayerKind using ONLY the explicit
    /// category declared in the .hw material definition. It does NOT inspect:
    /// - Layer names ("nsdm", "poly", "metal1", etc.)
    /// - Layer indices or counts
    /// - String patterns or substrings
    ///
    /// # Why This Works Universally
    ///
    /// Because materials are declared with explicit semantic categories:
    /// - TSMC's "OD" (oxide diffusion) → MaterialCategory::Semiconductor
    /// - Intel's "DIFF" → MaterialCategory::Semiconductor
    /// - Generic "active" → MaterialCategory::Semiconductor
    /// - TSMC's "PO" (polysilicon) → MaterialCategory::Conductor
    /// - PCB's "RF_Strip" → MaterialCategory::Conductor
    /// - Any foundry's "M1", "L1_Top", etc. → MaterialCategory::Conductor
    ///
    /// The compiler doesn't care what the layer is CALLED. It only cares about
    /// the material's PHYSICAL CATEGORY.
    ///
    /// # Arguments
    /// * `category` - The MaterialCategory from the material definition
    ///
    /// # Returns
    /// The LayerKind that governs routing and 3D mesh behavior
    pub fn from_material_category(category: MaterialCategory) -> Self {
        match category {
            MaterialCategory::Mask => Self::LithoMask,
            MaterialCategory::Insulator => Self::Dielectric,
            MaterialCategory::Semiconductor => Self::SemiconductorActive,
            MaterialCategory::Conductor => Self::ConductiveInterconnect,
            // Bridge categories are all conductive interconnects
            MaterialCategory::OhmicContact
            | MaterialCategory::DieInterconnect
            | MaterialCategory::PcbSolder
            | MaterialCategory::BarrierLayer
            | MaterialCategory::Adhesive => Self::ConductiveInterconnect,
        }
    }

    /// Determines routing surface policy from physics and stackup declaration
    ///
    /// # Arguments
    /// * `is_routable_in_stackup` - Whether the stackup declares `routable: true`
    ///
    /// # Physical Reasoning
    ///
    /// - **ConductiveInterconnect**: Route on centerline
    ///   Bulk metal planes. Vias can land from either direction.
    ///   Routing Z = (z_bottom + z_top) / 2
    ///
    /// - **SemiconductorActive**: Route on top surface
    ///   Planar 2D layers at wafer surface. Contacts land from above.
    ///   Routing Z = z_top
    ///
    /// - **LithoMask / Dielectric**: Non-routable
    ///   Zero-thickness masks and insulators cannot carry current.
    ///   Routing Z = undefined (error if queried)
    ///
    /// The `is_routable_in_stackup` flag allows the user to explicitly disable
    /// routing on a layer even if the material is conductive (e.g., a metal layer
    /// reserved for passive structures only).
    #[inline]
    pub fn routing_policy(&self, is_routable_in_stackup: bool) -> RoutingSurfacePolicy {
        if !is_routable_in_stackup {
            return RoutingSurfacePolicy::NonRoutable;
        }

        match self {
            Self::ConductiveInterconnect => RoutingSurfacePolicy::LayerCenterline,
            Self::SemiconductorActive => RoutingSurfacePolicy::SurfaceTop,
            Self::LithoMask | Self::Dielectric => RoutingSurfacePolicy::NonRoutable,
        }
    }

    /// Whether this layer represents a 2D zero-thickness lithographic mask
    #[inline]
    pub fn is_zero_thickness_mask(&self) -> bool {
        matches!(self, Self::LithoMask)
    }

    /// Whether this layer can conduct electrical current
    #[inline]
    pub fn is_conductive(&self) -> bool {
        matches!(
            self,
            Self::ConductiveInterconnect | Self::SemiconductorActive
        )
    }

    /// Get a human-readable description of this layer kind
    pub fn description(&self) -> &'static str {
        match self {
            Self::LithoMask => "Lithographic Mask (0nm, fabrication instruction)",
            Self::Dielectric => "Dielectric Insulator (capacitive isolation)",
            Self::SemiconductorActive => "Semiconductor Active Surface (planar wafer surface)",
            Self::ConductiveInterconnect => "Conductive Interconnect (bulk metal/poly)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_kind_from_material_category() {
        // Test fundamental categories
        assert_eq!(
            LayerKind::from_material_category(MaterialCategory::Mask),
            LayerKind::LithoMask
        );

        assert_eq!(
            LayerKind::from_material_category(MaterialCategory::Insulator),
            LayerKind::Dielectric
        );

        assert_eq!(
            LayerKind::from_material_category(MaterialCategory::Semiconductor),
            LayerKind::SemiconductorActive
        );

        assert_eq!(
            LayerKind::from_material_category(MaterialCategory::Conductor),
            LayerKind::ConductiveInterconnect
        );

        // Test bridge categories (all map to ConductiveInterconnect)
        assert_eq!(
            LayerKind::from_material_category(MaterialCategory::OhmicContact),
            LayerKind::ConductiveInterconnect
        );

        assert_eq!(
            LayerKind::from_material_category(MaterialCategory::DieInterconnect),
            LayerKind::ConductiveInterconnect
        );
    }

    #[test]
    fn test_routing_policy() {
        let active = LayerKind::SemiconductorActive;
        assert_eq!(
            active.routing_policy(true),
            RoutingSurfacePolicy::SurfaceTop
        );
        assert_eq!(
            active.routing_policy(false),
            RoutingSurfacePolicy::NonRoutable
        );

        let metal = LayerKind::ConductiveInterconnect;
        assert_eq!(
            metal.routing_policy(true),
            RoutingSurfacePolicy::LayerCenterline
        );
        assert_eq!(
            metal.routing_policy(false),
            RoutingSurfacePolicy::NonRoutable
        );

        let mask = LayerKind::LithoMask;
        assert_eq!(
            mask.routing_policy(true),
            RoutingSurfacePolicy::NonRoutable
        );
        assert_eq!(
            mask.routing_policy(false),
            RoutingSurfacePolicy::NonRoutable
        );
    }

    #[test]
    fn test_conductivity() {
        assert!(LayerKind::ConductiveInterconnect.is_conductive());
        assert!(LayerKind::SemiconductorActive.is_conductive());
        assert!(!LayerKind::Dielectric.is_conductive());
        assert!(!LayerKind::LithoMask.is_conductive());
    }

    #[test]
    fn test_zero_thickness() {
        assert!(LayerKind::LithoMask.is_zero_thickness_mask());
        assert!(!LayerKind::Dielectric.is_zero_thickness_mask());
        assert!(!LayerKind::SemiconductorActive.is_zero_thickness_mask());
        assert!(!LayerKind::ConductiveInterconnect.is_zero_thickness_mask());
    }
}
