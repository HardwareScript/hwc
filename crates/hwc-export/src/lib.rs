pub mod bom;
pub mod device_extractor;
pub mod dxf;
pub mod excellon;
pub mod exporter;
pub mod gdsii;
pub mod geometry_union;
pub mod glb;
pub mod hwx;
pub mod mesh_extrusion;
pub mod netlist;
pub mod oasis;
pub mod physical_z;
pub mod scene_graph;
pub mod solder_layers;
pub mod substrate;
pub mod welder;

pub use excellon::{export_hdi_vias, DrillVia, ExcellonExporter, ViaTypeCategory};
pub use exporter::{CompiledOutput, ExportFormat, Exporter};
pub use gdsii::{GdsBoundary, GdsCutMask, GdsiiWriter};
pub use hwx::{HwxContainer, HwxHeader, HWX_MAGIC, HWX_VERSION};
pub use oasis::OasisWriter;
pub use physical_z::{board_z_extent, dxf_layer_name, grid_index_from_z, via_layer_index, z_mm};
pub use scene_graph::{SceneGraph, SceneGraphError};
pub use solder_layers::{export as export_solder_layers, LayerType};
pub use substrate::{triangulate_and_extrude, SubstrateMesh, SubstrateTriangle, SubstrateVertex};
pub use welder::{circle_to_path, rect_to_path, stroke_polyline, trace_segment_to_path, weld_copper_geometry};

// Re-export device extractor types
pub use device_extractor::{format_spice, DeviceExtractionError, DeviceExtractor};
