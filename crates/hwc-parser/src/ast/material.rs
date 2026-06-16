//! Material definition types

use serde::{Deserialize, Serialize};

use super::common::{Identifier, Property};
use crate::lexer::Span;
use compact_str::CompactString;

/// Material definition: `material Name:` (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialDefinition {
    pub name: Identifier,
    pub category: MaterialCategory,
    pub process: ManufacturingProcess, // v0.1.7: Physical process behavior
    pub symbol: Option<CompactString>,
    pub description: Option<CompactString>,
    pub properties: Vec<Property>,
    pub span: Span,

    // Visual properties for PBR rendering (v0.1.6 God-Tier Visual API)
    // All optional with sensible defaults for backward compatibility
    pub color: Option<CompactString>, // HEX color (default: #808080)
    pub opacity: Option<f64>,         // 0.0-1.0 (default: 1.0)
    pub outline_opacity: Option<f64>, // 0.0-1.0 (default: 0.0)
    pub roughness: Option<f64>,       // 0.0-1.0 (default: 0.5)
    pub metallic: Option<f64>,        // 0.0-1.0 (default: 0.0)
    pub ior: Option<f64>,             // Index of Refraction (default: 1.5)
    pub clearcoat: Option<f64>,       // 0.0-1.0 (default: 0.0)
    pub clearcoat_roughness: Option<f64>, // 0.0-1.0 (default: 0.0)
    pub subsurface: Option<f64>,      // 0.0-1.0 (default: 0.0)
    pub anisotropy: Option<f64>,      // 0.0-1.0 (v0.1.7)
    pub anisotropy_rotation: Option<f64>, // 0.0-2PI (v0.1.7)
    pub texture: Option<CompactString>, // Procedural texture name (v0.1.7)
}

/// Manufacturing process behavior for Z-axis placement (v0.1.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ManufacturingProcess {
    /// Drilled and plated through the substrate (PCB style)
    DrilledPlated,
    /// Deposited/Plotted into the grid (CMOS/3D-Print style)
    #[default]
    Deposited,
    /// Etched away from existing material (MEMS style)
    Etched,
}

/// Material alias definition: `material_alias Name: Target` (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialAliasDefinition {
    pub name: Identifier,
    pub target: Identifier,
    pub span: Span,
}

/// Material category
///
/// The three fundamental categories (Conductor, Insulator, Semiconductor) cover
/// bulk materials. The bridge categories below are specializations for materials
/// used at material transition interfaces (see BRIDGE-IMPLEMENTATION.md).
///
/// All bridge categories are electrically conductive, but carry distinct semantic
/// meaning that enables bridge-specific DRC rules and profile lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MaterialCategory {
    // === Fundamental categories ===
    Conductor,
    Insulator,
    Semiconductor,

    // === Bridge categories (Phase 1 - BRIDGE-IMPLEMENTATION.md) ===
    /// Silicon-to-metal ohmic contacts (silicides: TiSi₂, CoSi₂, NiSi, etc.)
    OhmicContact,
    /// Die-to-die interconnects (gold bumps, copper pillars, micro-bumps)
    DieInterconnect,
    /// PCB-level solders (SAC305, SnPb eutectic, bismuth alloys)
    PcbSolder,
    /// Diffusion barrier layers (TiN, TaN, tungsten liners)
    BarrierLayer,
    /// Conductive adhesives (silver epoxy, ACF, conductive films)
    Adhesive,
}

impl MaterialCategory {
    /// Returns true if this category is electrically conductive.
    /// All bridge categories conduct electricity.
    pub fn is_conductive(&self) -> bool {
        matches!(
            self,
            MaterialCategory::Conductor
                | MaterialCategory::OhmicContact
                | MaterialCategory::DieInterconnect
                | MaterialCategory::PcbSolder
                | MaterialCategory::BarrierLayer
                | MaterialCategory::Adhesive
        )
    }

    /// Returns true if this is a bridge category (not a fundamental category).
    pub fn is_bridge(&self) -> bool {
        matches!(
            self,
            MaterialCategory::OhmicContact
                | MaterialCategory::DieInterconnect
                | MaterialCategory::PcbSolder
                | MaterialCategory::BarrierLayer
                | MaterialCategory::Adhesive
        )
    }
}

impl std::fmt::Display for MaterialCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaterialCategory::Conductor => write!(f, "conductor"),
            MaterialCategory::Insulator => write!(f, "insulator"),
            MaterialCategory::Semiconductor => write!(f, "semiconductor"),
            MaterialCategory::OhmicContact => write!(f, "ohmic_contact"),
            MaterialCategory::DieInterconnect => write!(f, "die_interconnect"),
            MaterialCategory::PcbSolder => write!(f, "pcb_solder"),
            MaterialCategory::BarrierLayer => write!(f, "barrier_layer"),
            MaterialCategory::Adhesive => write!(f, "adhesive"),
        }
    }
}

impl MaterialDefinition {
    /// Get color with default fallback
    pub fn get_color(&self) -> CompactString {
        self.color.clone().unwrap_or_else(|| "#808080".into())
    }

    /// Get opacity with default fallback
    pub fn get_opacity(&self) -> f64 {
        self.opacity.unwrap_or(1.0)
    }

    /// Get outline opacity with default fallback
    pub fn get_outline_opacity(&self) -> f64 {
        self.outline_opacity.unwrap_or(0.0)
    }

    /// Get roughness with default fallback
    pub fn get_roughness(&self) -> f64 {
        self.roughness.unwrap_or(0.5)
    }

    /// Get metallic with default fallback
    pub fn get_metallic(&self) -> f64 {
        self.metallic.unwrap_or(0.0)
    }

    /// Get IOR with default fallback (v0.1.7)
    pub fn get_ior(&self) -> f64 {
        self.ior.unwrap_or(1.5)
    }

    /// Get clearcoat with default fallback (v0.1.7)
    pub fn get_clearcoat(&self) -> f64 {
        self.clearcoat.unwrap_or(0.0)
    }

    /// Get clearcoat roughness with default fallback (v0.1.7)
    pub fn get_clearcoat_roughness(&self) -> f64 {
        self.clearcoat_roughness.unwrap_or(0.0)
    }

    /// Get subsurface with default fallback (v0.1.7)
    pub fn get_subsurface(&self) -> f64 {
        self.subsurface.unwrap_or(0.0)
    }

    /// Get anisotropy with default fallback (v0.1.7)
    pub fn get_anisotropy(&self) -> f64 {
        self.anisotropy.unwrap_or(0.0)
    }

    /// Get anisotropy rotation with default fallback (v0.1.7)
    pub fn get_anisotropy_rotation(&self) -> f64 {
        self.anisotropy_rotation.unwrap_or(0.0)
    }

    /// Get procedural texture name (v0.1.7)
    pub fn get_texture(&self) -> Option<CompactString> {
        self.texture.clone()
    }
}
