//! Solder mask and paste layer generation for Gerber export.
//!
//! This module generates the four additional manufacturing layers required for PCB assembly:
//! - Top Solder Mask (.gts) - Defines where the green/black coating is removed
//! - Bottom Solder Mask (.gbs) - Same for bottom layer
//! - Top Solder Paste (.gtp) - Defines stencil openings for solder paste application
//! - Bottom Solder Paste (.gbp) - Same for bottom layer
//!
//! ## Manufacturing Requirements
//!
//! - Solder mask openings are typically 0.05-0.1mm LARGER than the copper pad
//! - Solder paste openings are typically 10-20% SMALLER than the copper pad
//! - These layers are critical for automated assembly (pick-and-place machines)

use crate::physical_z::{board_z_extent, is_on_board_face, z_mm};
use compact_str::CompactString;
use hwc_engine::{HardwareSpace, PadShape};
use std::path::Path;

/// Solder mask expansion in nanometers (0.075mm = 75µm)
const SOLDER_MASK_EXPANSION_NM: i64 = 75_000;

/// Solder paste reduction factor (15% smaller)
const SOLDER_PASTE_REDUCTION: f64 = 0.85;

/// Layer type for solder mask/paste generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerType {
    TopSolderMask,
    BottomSolderMask,
    TopSolderPaste,
    BottomSolderPaste,
}

impl LayerType {
    /// Get the file extension for this layer type
    pub fn file_extension(&self) -> &'static str {
        match self {
            LayerType::TopSolderMask => "gts",
            LayerType::BottomSolderMask => "gbs",
            LayerType::TopSolderPaste => "gtp",
            LayerType::BottomSolderPaste => "gbp",
        }
    }

    /// Get the Gerber file function attribute
    pub fn file_function(&self) -> &'static str {
        match self {
            LayerType::TopSolderMask => "Soldermask,Top",
            LayerType::BottomSolderMask => "Soldermask,Bot",
            LayerType::TopSolderPaste => "Paste,Top",
            LayerType::BottomSolderPaste => "Paste,Bot",
        }
    }

    /// Check if this is a top layer
    pub fn is_top(&self) -> bool {
        matches!(self, LayerType::TopSolderMask | LayerType::TopSolderPaste)
    }

    /// Check if this is a solder mask layer (vs paste)
    pub fn is_mask(&self) -> bool {
        matches!(self, LayerType::TopSolderMask | LayerType::BottomSolderMask)
    }
}

/// Gerber emitter for solder mask and paste layers
struct SolderLayerEmitter {
    buffer: CompactString,
    aperture_counter: u32,
}

impl SolderLayerEmitter {
    fn new(layer_type: LayerType) -> Self {
        let mut buffer = String::with_capacity(64 * 1024);

        // Gerber X3 header
        buffer.push_str("G04 #@! TF.GenerationSoftware,Hardware Script,hwc,0.1.4*\n");
        buffer.push_str(&format!(
            "G04 #@! TF.CreationDate,{}*\n",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S")
        ));
        buffer.push_str(&format!(
            "G04 #@! TF.FileFunction,{}*\n",
            layer_type.file_function()
        ));
        buffer.push_str("G04 #@! TF.FilePolarity,Positive*\n");

        // Format specification
        buffer.push_str("%FSLAX36Y36*%\n"); // 3.6 format (nanometer precision)
        buffer.push_str("%MOMM*%\n"); // Units: millimeters
        buffer.push_str("%LPD*%\n"); // Layer polarity: dark
        buffer.push_str("G01*\n"); // Linear interpolation mode

        Self {
            buffer: buffer.into(),
            aperture_counter: 10,
        }
    }

    /// Define a circular aperture
    fn define_circle_aperture(&mut self, diameter_nm: i64) -> u32 {
        let id = self.aperture_counter;
        self.aperture_counter += 1;

        let diameter_mm = diameter_nm as f64 / 1_000_000.0;
        self.buffer
            .push_str(&format!("%ADD{}C,{:.6}*%\n", id, diameter_mm));

        id
    }

    /// Define a rectangular aperture
    fn define_rect_aperture(&mut self, width_nm: i64, height_nm: i64) -> u32 {
        let id = self.aperture_counter;
        self.aperture_counter += 1;

        let width_mm = width_nm as f64 / 1_000_000.0;
        let height_mm = height_nm as f64 / 1_000_000.0;
        self.buffer
            .push_str(&format!("%ADD{}R,{:.6}X{:.6}*%\n", id, width_mm, height_mm));

        id
    }

    /// Define an obround (oval) aperture
    fn define_obround_aperture(&mut self, width_nm: i64, height_nm: i64) -> u32 {
        let id = self.aperture_counter;
        self.aperture_counter += 1;

        let width_mm = width_nm as f64 / 1_000_000.0;
        let height_mm = height_nm as f64 / 1_000_000.0;
        self.buffer
            .push_str(&format!("%ADD{}O,{:.6}X{:.6}*%\n", id, width_mm, height_mm));

        id
    }

    /// Flash aperture at position
    fn flash_at(&mut self, aperture_id: u32, x_nm: i64, y_nm: i64) {
        self.buffer.push_str(&format!("D{}*\n", aperture_id));
        self.buffer.push_str(&format!("X{}Y{}D03*\n", x_nm, y_nm));
    }

    /// Finish and return Gerber content
    fn finish(mut self) -> CompactString {
        self.buffer.push_str("M02*\n"); // End of file
        self.buffer
    }
}

/// Pad with position and shape for layer generation
#[derive(Debug, Clone)]
struct PadInfo {
    x_nm: i64,
    y_nm: i64,
    z_nm: i64,
    shape: PadShape,
}

/// Export solder mask and paste layers
pub fn export(space: &HardwareSpace, output_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Collect all pads from the netlist
    let pads = collect_pads(space)?;

    let (board_min_z_nm, board_max_z_nm) = board_z_extent(space);
    let voxel_z_nm = space.voxel_size.z_nm.max(1);

    export_layer(
        &pads,
        LayerType::TopSolderMask,
        board_max_z_nm,
        voxel_z_nm,
        output_dir,
    )?;
    export_layer(
        &pads,
        LayerType::BottomSolderMask,
        board_min_z_nm,
        voxel_z_nm,
        output_dir,
    )?;
    export_layer(
        &pads,
        LayerType::TopSolderPaste,
        board_max_z_nm,
        voxel_z_nm,
        output_dir,
    )?;
    export_layer(
        &pads,
        LayerType::BottomSolderPaste,
        board_min_z_nm,
        voxel_z_nm,
        output_dir,
    )?;

    Ok(())
}

/// Collect all pads from components in the netlist
fn collect_pads(space: &HardwareSpace) -> Result<Vec<PadInfo>, Box<dyn std::error::Error>> {
    let mut pads = Vec::new();

    // Iterate through all components
    for comp_idx in 0..space.netlist.component_count() {
        let comp_id = hwc_engine::ComponentId::new(comp_idx as u32);
        if let Some(component) = space.netlist.get_component(comp_id) {
            let comp_pos = component.position_nm;

            // Get all pins for this component
            let pins = space.netlist.get_component_pins(comp_id);
            for pin_id in pins {
                if let Some(pin) = space.netlist.get_pin(pin_id) {
                    // Only process pins with pad shapes
                    if let Some(pad_shape) = &pin.pad_shape {
                        // Calculate absolute pin position
                        let x_nm = comp_pos.0 + pin.local_offset_nm.0;
                        let y_nm = comp_pos.1 + pin.local_offset_nm.1;
                        let z_nm = comp_pos.2 + pin.local_offset_nm.2;

                        pads.push(PadInfo {
                            x_nm,
                            y_nm,
                            z_nm,
                            shape: pad_shape.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(pads)
}

/// Export a single solder mask or paste layer
fn export_layer(
    pads: &[PadInfo],
    layer_type: LayerType,
    target_face_z_nm: i64,
    voxel_z_nm: i64,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut emitter = SolderLayerEmitter::new(layer_type);

    // Filter pads on the target board face by physical Z
    let layer_pads: Vec<_> = pads
        .iter()
        .filter(|pad| is_on_board_face(pad.z_nm, target_face_z_nm, voxel_z_nm))
        .collect();

    if layer_pads.is_empty() {
        println!(
            "   ⚠️  No pads found at z = {:.4}mm for {}",
            z_mm(target_face_z_nm),
            layer_type.file_extension()
        );
        // Still generate an empty file for completeness
    }

    // Process each pad
    let pad_count = layer_pads.len();
    for pad in &layer_pads {
        let aperture_id = define_pad_aperture(&mut emitter, &pad.shape, layer_type)?;
        emitter.flash_at(aperture_id, pad.x_nm, pad.y_nm);
    }

    // Write file
    let filename = format!("board.{}", layer_type.file_extension());
    let path = output_dir.join(&filename);
    let content = emitter.finish();
    std::fs::write(&path, content)?;

    println!(
        "   ✅ {} Layer: {} ({} pads)",
        if layer_type.is_mask() {
            "Solder Mask"
        } else {
            "Solder Paste"
        },
        filename,
        pad_count
    );

    Ok(())
}

/// Define aperture for a pad shape with appropriate sizing
fn define_pad_aperture(
    emitter: &mut SolderLayerEmitter,
    shape: &PadShape,
    layer_type: LayerType,
) -> Result<u32, Box<dyn std::error::Error>> {
    let aperture_id = match shape {
        PadShape::Circle { diameter_nm } => {
            let adjusted_diameter = adjust_size(*diameter_nm, layer_type);
            emitter.define_circle_aperture(adjusted_diameter)
        }
        PadShape::Rectangle {
            width_nm,
            height_nm,
        } => {
            let adjusted_width = adjust_size(*width_nm, layer_type);
            let adjusted_height = adjust_size(*height_nm, layer_type);
            emitter.define_rect_aperture(adjusted_width, adjusted_height)
        }
        PadShape::Obround {
            width_nm,
            height_nm,
        } => {
            let adjusted_width = adjust_size(*width_nm, layer_type);
            let adjusted_height = adjust_size(*height_nm, layer_type);
            emitter.define_obround_aperture(adjusted_width, adjusted_height)
        }
        PadShape::RoundedRect {
            width_nm,
            height_nm,
            corner_radius_nm: _,
        } => {
            // Treat rounded rect as regular rect for now
            // TODO: Implement proper rounded rectangle aperture macro
            let adjusted_width = adjust_size(*width_nm, layer_type);
            let adjusted_height = adjust_size(*height_nm, layer_type);
            emitter.define_rect_aperture(adjusted_width, adjusted_height)
        }
        PadShape::Polygon { points: _ } => {
            // Polygons require aperture macros - use bounding box for now
            // TODO: Implement proper polygon aperture macro
            let (width, height) = shape.bounding_box();
            let adjusted_width = adjust_size(width, layer_type);
            let adjusted_height = adjust_size(height, layer_type);
            emitter.define_rect_aperture(adjusted_width, adjusted_height)
        }
    };

    Ok(aperture_id)
}

/// Adjust pad size based on layer type
fn adjust_size(size_nm: i64, layer_type: LayerType) -> i64 {
    if layer_type.is_mask() {
        // Solder mask: expand by 0.075mm
        size_nm + SOLDER_MASK_EXPANSION_NM
    } else {
        // Solder paste: reduce by 15%
        (size_nm as f64 * SOLDER_PASTE_REDUCTION) as i64
    }
}
