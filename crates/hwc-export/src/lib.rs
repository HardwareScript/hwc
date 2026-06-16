pub mod bom;
pub mod contour_tracer; // v0.1.6: Voxel-to-vector conversion with anti-aliasing
pub mod device_extractor;
pub mod dxf;
pub mod excellon;
pub mod exporter;
pub mod geometry_union;
pub mod glb;
pub mod mesh_extrusion;
pub mod netlist;
pub mod physical_z;
pub mod scene_graph;
pub mod solder_layers;

pub use excellon::{export_hdi_vias, DrillVia, ExcellonExporter, ViaTypeCategory};
pub use exporter::{CompiledOutput, ExportFormat, Exporter};
pub use physical_z::{board_z_extent, dxf_layer_name, grid_index_from_z, z_mm};
pub use scene_graph::{SceneGraph, SceneGraphError};
pub use solder_layers::{export as export_solder_layers, LayerType};

// Re-export device extractor types
pub use device_extractor::{format_spice, DeviceExtractionError, DeviceExtractor};

// Re-export contour tracer types (v0.1.6)
pub use contour_tracer::{Contour, ContourConfig, ContourTracer, Point};
