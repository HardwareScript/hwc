//! Routing Surface Policy - Explicit Elevation Strategy
//!
//! Architectural Law: Physical Reality
//! Every routable layer has an explicit routing elevation policy that determines
//! the Z-coordinate where route centerlines are placed. NO heuristics, NO guessing.

use serde::{Deserialize, Serialize};

/// Explicit routing and contact landing elevation policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoutingSurfacePolicy {
    /// Base semiconductor layer: vias connect strictly from above -> route on Top surface
    ///
    /// Used for:
    /// - Active diffusion (ndiff, pdiff)
    /// - Gate electrodes (poly)
    ///
    /// Physical reasoning: these are planar 2D layers at the wafer surface.
    /// Contacts land on top, so routing occurs at z_top elevation.
    SurfaceTop,

    /// Interconnect metal layer: vias connect from below or above -> route along Centerline
    ///
    /// Used for:
    /// - Metal layers (M1, M2, M3, ...)
    /// - Local interconnect (LI1, M0)
    ///
    /// Physical reasoning: these are bulk conductors. Vias can land from either
    /// direction, so routing occurs at the geometric centerline: (z_bottom + z_top) / 2.
    LayerCenterline,

    /// Non-routable dielectric or 0nm mask layer -> Cannot be routed
    ///
    /// Used for:
    /// - Dielectric layers (oxide, ILD, IMD)
    /// - Lithographic masks (NSDM, PSDM, TAP)
    /// - Passivation layers
    ///
    /// Physical reasoning: these layers are either non-conductive or have zero
    /// thickness. They cannot support current-carrying routes.
    NonRoutable,
}

impl RoutingSurfacePolicy {
    /// Compute the routing Z-coordinate for a given layer geometry
    ///
    /// # Arguments
    /// * `z_bottom` - Bottom Z elevation of the layer in nanometers
    /// * `z_top` - Top Z elevation of the layer in nanometers
    ///
    /// # Returns
    /// The routing Z-coordinate in nanometers where route centerlines should be placed.
    /// Returns None if the policy is NonRoutable.
    #[inline]
    pub fn compute_routing_z(&self, z_bottom: i64, z_top: i64) -> Option<i64> {
        match self {
            Self::SurfaceTop => Some(z_top),
            Self::LayerCenterline => Some((z_bottom + z_top) / 2),
            Self::NonRoutable => None,
        }
    }

    /// Check if this policy allows routing
    #[inline]
    pub fn is_routable(&self) -> bool {
        !matches!(self, Self::NonRoutable)
    }

    /// Get a human-readable description of the routing policy
    pub fn description(&self) -> &'static str {
        match self {
            Self::SurfaceTop => "Route on top surface (vias land from above)",
            Self::LayerCenterline => "Route on centerline (vias can land from either direction)",
            Self::NonRoutable => "Non-routable (dielectric or zero-thickness mask)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_z_calculation() {
        // SurfaceTop should return z_top
        assert_eq!(
            RoutingSurfacePolicy::SurfaceTop.compute_routing_z(100, 150),
            Some(150)
        );

        // LayerCenterline should return midpoint
        assert_eq!(
            RoutingSurfacePolicy::LayerCenterline.compute_routing_z(100, 150),
            Some(125)
        );

        // NonRoutable should return None
        assert_eq!(
            RoutingSurfacePolicy::NonRoutable.compute_routing_z(100, 150),
            None
        );
    }

    #[test]
    fn test_is_routable() {
        assert!(RoutingSurfacePolicy::SurfaceTop.is_routable());
        assert!(RoutingSurfacePolicy::LayerCenterline.is_routable());
        assert!(!RoutingSurfacePolicy::NonRoutable.is_routable());
    }
}
