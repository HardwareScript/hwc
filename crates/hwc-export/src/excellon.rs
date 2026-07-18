//! Excellon Drill File Export
//!
//! Generates .drl files for PCB manufacturing.
//! Excellon format is the industry standard for drill hole locations.
//!
//! **GAP1 Section 3.1, 4.2: Drill File Export**
//! **GAP1 Section 5.3: HDI Via Support**

use crate::physical_z::{board_z_extent, grid_index_from_z};
use compact_str::CompactString;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Via information for drill file generation.
#[derive(Debug, Clone)]
pub struct DrillVia {
    /// Position in nanometers (X, Y)
    pub position: (i64, i64),

    /// Drill diameter in nanometers
    pub diameter_nm: i64,

    /// Bottom Z of the via span in nanometers (source of truth)
    pub from_z_nm: i64,

    /// Top Z of the via span in nanometers
    pub to_z_nm: i64,

    /// 0-based grid slab indices derived at export time (Excellon layer-pair naming only)
    pub from_layer: u8,

    /// 0-based grid slab index for the top of the span (display only)
    pub to_layer: u8,

    /// Via type (through-hole, blind, buried, microvia)
    pub via_type: ViaTypeCategory,
}

impl DrillVia {
    /// Build a drill via from physical Z with derived grid indices for file naming.
    pub fn from_physical_z(
        position: (i64, i64),
        from_z_nm: i64,
        to_z_nm: i64,
        diameter_nm: i64,
        via_type: ViaTypeCategory,
        from_layer: u8,
        to_layer: u8,
    ) -> Self {
        Self {
            position,
            diameter_nm,
            from_z_nm,
            to_z_nm,
            from_layer,
            to_layer,
            via_type,
        }
    }
}

/// Via type category for drill file separation.
///
/// Different via types require different manufacturing processes
/// and are exported to separate drill files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ViaTypeCategory {
    /// Plated through-hole (spans all layers)
    PlatedThroughHole,

    /// Non-plated through-hole (mechanical holes)
    NonPlatedThroughHole,

    /// Blind via (outer to inner layer)
    Blind { from_layer: u8, to_layer: u8 },

    /// Buried via (inner to inner layer)
    Buried { from_layer: u8, to_layer: u8 },

    /// Microvia (laser-drilled, <150µm)
    Microvia { from_layer: u8, to_layer: u8 },
}

impl ViaTypeCategory {
    /// Get the drill file suffix for this via type.
    ///
    /// # Returns
    /// File suffix (e.g., "PTH", "1-2", "3-4")
    pub fn file_suffix(&self) -> CompactString {
        match self {
            ViaTypeCategory::PlatedThroughHole => "PTH".into(),
            ViaTypeCategory::NonPlatedThroughHole => "NPTH".into(),
            ViaTypeCategory::Blind {
                from_layer,
                to_layer,
            } => format!("{}-{}", from_layer + 1, to_layer + 1).into(),
            ViaTypeCategory::Buried {
                from_layer,
                to_layer,
            } => format!("{}-{}", from_layer + 1, to_layer + 1).into(),
            ViaTypeCategory::Microvia {
                from_layer,
                to_layer,
            } => format!("micro-{}-{}", from_layer + 1, to_layer + 1).into(),
        }
    }

    /// Get a human-readable description of this via type.
    pub fn description(&self) -> CompactString {
        match self {
            ViaTypeCategory::PlatedThroughHole => "Plated Through-Hole".into(),
            ViaTypeCategory::NonPlatedThroughHole => "Non-Plated Through-Hole".into(),
            ViaTypeCategory::Blind {
                from_layer,
                to_layer,
            } => format!("Blind Via (Layer {} to {})", from_layer + 1, to_layer + 1).into(),
            ViaTypeCategory::Buried {
                from_layer,
                to_layer,
            } => format!("Buried Via (Layer {} to {})", from_layer + 1, to_layer + 1).into(),
            ViaTypeCategory::Microvia {
                from_layer,
                to_layer,
            } => format!("Microvia (Layer {} to {})", from_layer + 1, to_layer + 1).into(),
        }
    }
}

/// Excellon drill file exporter.
pub struct ExcellonExporter {
    /// Board name for header
    board_name: CompactString,

    /// Vias to export
    vias: Vec<DrillVia>,
}

impl ExcellonExporter {
    /// Create a new Excellon exporter.
    ///
    /// # Arguments
    /// * `board_name` - Name of the board (for header comment)
    pub fn new(board_name: CompactString) -> Self {
        Self {
            board_name,
            vias: Vec::new(),
        }
    }

    /// Add a via to the drill file.
    ///
    /// # Arguments
    /// * `via` - Via to add
    pub fn add_via(&mut self, via: DrillVia) {
        self.vias.push(via);
    }

    /// Export drill file to writer.
    ///
    /// Generates Excellon format drill file with:
    /// - Header with format specification
    /// - Tool definitions (one per unique diameter)
    /// - Drill coordinates in inches (industry standard)
    ///
    /// # Arguments
    /// * `writer` - Output writer
    ///
    /// # Returns
    /// Result indicating success or I/O error
    pub fn export<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Group vias by diameter
        let mut vias_by_diameter: FxHashMap<i64, Vec<&DrillVia>> = FxHashMap::default();
        for via in &self.vias {
            vias_by_diameter
                .entry(via.diameter_nm)
                .or_default()
                .push(via);
        }

        // Write header
        writeln!(writer, "M48")?;
        writeln!(
            writer,
            "; DRILL file {{{}}} date {}",
            self.board_name,
            chrono::Local::now().format("%Y-%m-%d")
        )?;
        writeln!(
            writer,
            "; v0.1.7 physical Z (mm) recorded per tool pass where applicable"
        )?;
        writeln!(writer, "; FORMAT={{-:-/ absolute / inch / decimal}}")?;
        writeln!(writer, "FMAT,2")?;
        writeln!(writer, "INCH")?;

        // Write tool definitions
        for (tool_num, (&diameter_nm, _)) in vias_by_diameter.iter().enumerate() {
            let diameter_inches = nm_to_inches(diameter_nm);
            writeln!(writer, "T{}C{:.4}", tool_num + 1, diameter_inches)?;
        }
        writeln!(writer, "%")?;

        // Write drill coordinates for each tool
        for (tool_num, (_, vias)) in vias_by_diameter.iter().enumerate() {
            writeln!(writer, "T{}", tool_num + 1)?;

            for via in vias {
                let x_inches = nm_to_inches(via.position.0);
                let y_inches = nm_to_inches(via.position.1);
                writeln!(writer, "X{:.4}Y{:.4}", x_inches, y_inches)?;
            }
        }

        // Write end of file
        writeln!(writer, "M30")?;

        Ok(())
    }
}

/// Convert nanometers to inches.
///
/// PCB industry standard uses inches for drill files.
///
/// # Arguments
/// * `nm` - Value in nanometers
///
/// # Returns
/// Value in inches
fn nm_to_inches(nm: i64) -> f64 {
    // 1 inch = 25,400,000 nanometers
    nm as f64 / 25_400_000.0
}

/// Export drill file from hardware space.
///
/// Extracts all vias from the routed space and generates an Excellon drill file.
///
/// # Arguments
/// * `space` - Hardware space with routed nets and vias
/// * `output_dir` - Output directory for drill file
///
/// # Returns
/// Result indicating success or error
pub fn export(space: &HardwareSpace, output_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Create drill/ subdirectory for clean organization
    let drill_dir = output_dir.join("drill");
    std::fs::create_dir_all(&drill_dir)?;

    // Extract vias from space
    let vias = &space.vias;

    // If no vias, create empty drill file
    if vias.is_empty() {
        let exporter = ExcellonExporter::new(space.name.clone());
        let drill_path = drill_dir.join(format!("{}.drl", space.name));
        let mut file = File::create(drill_path)?;
        exporter.export(&mut file)?;
        return Ok(());
    }

    let (board_min_z_nm, board_max_z_nm) = board_z_extent(space);

    let drill_vias: Vec<DrillVia> = vias
        .iter()
        .map(|via| {
            let via_type = if via.is_through_hole(board_min_z_nm, board_max_z_nm) {
                ViaTypeCategory::PlatedThroughHole
            } else if via.is_blind(board_min_z_nm, board_max_z_nm) {
                let from_layer =
                    grid_index_from_z(via.from_z_nm.min(via.to_z_nm), space.resolution_nm);
                let to_layer =
                    grid_index_from_z(via.from_z_nm.max(via.to_z_nm), space.resolution_nm);
                ViaTypeCategory::Blind {
                    from_layer,
                    to_layer,
                }
            } else if via.is_buried(board_min_z_nm, board_max_z_nm) {
                let from_layer =
                    grid_index_from_z(via.from_z_nm.min(via.to_z_nm), space.resolution_nm);
                let to_layer =
                    grid_index_from_z(via.from_z_nm.max(via.to_z_nm), space.resolution_nm);
                ViaTypeCategory::Buried {
                    from_layer,
                    to_layer,
                }
            } else {
                ViaTypeCategory::PlatedThroughHole
            };

            let from_layer = grid_index_from_z(via.from_z_nm.min(via.to_z_nm), space.resolution_nm);
            let to_layer = grid_index_from_z(via.from_z_nm.max(via.to_z_nm), space.resolution_nm);
            DrillVia::from_physical_z(
                via.position,
                via.from_z_nm,
                via.to_z_nm,
                via.diameter_nm,
                via_type,
                from_layer,
                to_layer,
            )
        })
        .collect();

    // Export using the standard via export function
    export_vias(&space.name, &drill_vias, &drill_dir)?;

    Ok(())
}

/// Export drill file from a list of vias.
///
/// Generates an Excellon drill file from a vector of vias.
///
/// # Arguments
/// * `board_name` - Name of the board
/// * `vias` - Vector of vias to export
/// * `output_dir` - Output directory for drill file
///
/// # Returns
/// Result indicating success or error
pub fn export_vias(
    board_name: &str,
    vias: &[DrillVia],
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut exporter = ExcellonExporter::new(board_name.into());

    for via in vias {
        exporter.add_via(via.clone());
    }

    // Write drill file
    let drill_path = output_dir.join(format!("{}.drl", board_name));
    let mut file = File::create(drill_path)?;
    exporter.export(&mut file)?;

    Ok(())
}

/// Export HDI drill files with via type separation.
///
/// **GAP1 Section 5.3: HDI Via Support**
///
/// Generates separate drill files for different via types:
/// - board-PTH.drl: Plated through-holes
/// - board-NPTH.drl: Non-plated through-holes
/// - board-1-2.drl: Blind vias from layer 1 to 2
/// - board-3-4.drl: Buried vias from layer 3 to 4
/// - board-micro-1-2.drl: Microvias
///
/// # Arguments
/// * `board_name` - Name of the board
/// * `vias` - Vector of vias to export
/// * `output_dir` - Output directory for drill files
///
/// # Returns
/// Result with count of drill files generated or error
pub fn export_hdi_vias(
    board_name: &str,
    vias: &[DrillVia],
    output_dir: &Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    // Group vias by type
    let mut vias_by_type: FxHashMap<ViaTypeCategory, Vec<DrillVia>> = FxHashMap::default();

    for via in vias {
        vias_by_type
            .entry(via.via_type)
            .or_default()
            .push(via.clone());
    }

    let mut file_count = 0;

    // Generate separate drill file for each via type
    for (via_type, type_vias) in vias_by_type {
        let mut exporter = ExcellonExporter::new(board_name.into());

        for via in &type_vias {
            exporter.add_via(via.clone());
        }

        // Generate filename with via type suffix
        let suffix = via_type.file_suffix();
        let filename = format!("{}-{}.drl", board_name, suffix);
        let drill_path = output_dir.join(&filename);

        let mut file = File::create(&drill_path)?;

        // Add via type description to header
        writeln!(file, "M48")?;
        writeln!(
            file,
            "; DRILL file {{{}}} - {}",
            board_name,
            via_type.description()
        )?;
        writeln!(file, "; date {}", chrono::Local::now().format("%Y-%m-%d"))?;
        writeln!(file, "; FORMAT={{-:-/ absolute / inch / decimal}}")?;
        writeln!(file, "FMAT,2")?;
        writeln!(file, "INCH")?;

        // Group by diameter and write tool definitions
        let mut vias_by_diameter: FxHashMap<i64, Vec<&DrillVia>> = FxHashMap::default();
        for via in &type_vias {
            vias_by_diameter
                .entry(via.diameter_nm)
                .or_default()
                .push(via);
        }

        // Write tool definitions
        for (tool_num, (&diameter_nm, _)) in vias_by_diameter.iter().enumerate() {
            let diameter_inches = nm_to_inches(diameter_nm);
            writeln!(file, "T{}C{:.4}", tool_num + 1, diameter_inches)?;
        }
        writeln!(file, "%")?;

        // Write drill coordinates for each tool
        for (tool_num, (_, vias)) in vias_by_diameter.iter().enumerate() {
            writeln!(file, "T{}", tool_num + 1)?;

            for via in vias {
                let x_inches = nm_to_inches(via.position.0);
                let y_inches = nm_to_inches(via.position.1);
                writeln!(file, "X{:.4}Y{:.4}", x_inches, y_inches)?;
            }
        }

        // Write end of file
        writeln!(file, "M30")?;

        file_count += 1;
    }

    Ok(file_count)
}
