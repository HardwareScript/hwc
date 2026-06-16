//! Core data types for the scene graph

use compact_str::CompactString;

/// RGB color (0-255)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert to normalized RGB (0.0-1.0) for 3D formats
    pub fn to_normalized(&self) -> (f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        )
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> CompactString {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b).into()
    }
}

/// Material node in scene graph
#[derive(Debug, Clone)]
pub struct MaterialNode {
    pub name: CompactString,
    pub color: Color,
    pub opacity: f32,                   // 0.0-1.0 (v0.1.6)
    pub outline_opacity: f32,           // 0.0-1.0 (v0.1.6)
    pub metallic: f32,                  // 0.0-1.0
    pub roughness: f32,                 // 0.0-1.0
    pub ior: f32,                       // Index of Refraction (v0.1.7)
    pub clearcoat: f32,                 // 0.0-1.0 (v0.1.7)
    pub clearcoat_roughness: f32,       // 0.0-1.0 (v0.1.7)
    pub subsurface: f32,                // 0.0-1.0 (v0.1.7)
    pub anisotropy: f32,                // 0.0-1.0 (v0.1.7)
    pub anisotropy_rotation: f32,       // 0.0-2PI (v0.1.7)
    pub texture: Option<CompactString>, // Procedural texture name (v0.1.7)
    pub precedence: u8,                 // v0.1.7 Manifold Export (1=High, 4=Low)
}

/// Face culling bitmask for manifold export (v0.1.7)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FaceCulling {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
    pub front: bool,
    pub back: bool,
}

impl FaceCulling {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn all() -> Self {
        Self {
            top: true,
            bottom: true,
            left: true,
            right: true,
            front: true,
            back: true,
        }
    }
}

/// Mesh vertex
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Box mesh parameters
#[derive(Debug, Clone, Copy)]
pub struct BoxParams {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub width: f64,
    pub height: f64,
    pub depth: f64,
}

impl BoxParams {
    pub fn new(x: f64, y: f64, z: f64, width: f64, height: f64, depth: f64) -> Self {
        Self {
            x,
            y,
            z,
            width,
            height,
            depth,
        }
    }
}

/// Mesh face (triangle or quad)
#[derive(Debug, Clone)]
pub struct Face {
    pub vertices: Vec<usize>, // Indices into vertex array
}

/// Mesh node in scene graph
#[derive(Debug, Clone)]
pub struct MeshNode {
    pub name: CompactString,
    pub vertices: Vec<Vertex>,
    pub faces: Vec<Face>,
    pub material_name: CompactString,
}
