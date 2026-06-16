use crate::ir::stackup_manager::StackupManager;
use crate::SymbolTable;
use hwc_engine::geometry::Point3D;

pub struct PlacementContext<'a> {
    pub symbol_table: &'a SymbolTable,
    pub eval_context: &'a hwc_parser::EvaluationContext,
    pub stackup_manager: &'a StackupManager,
    pub collector: &'a hwc_diagnostics::DiagnosticCollector,
    pub profile: Option<&'a hwc_parser::ProfileDefinition>,
    pub origin: hwc_parser::OriginPoint,
}

pub struct ComponentPlacementData<'a> {
    pub component: &'a hwc_parser::ComponentPlacement,
    pub name: String,
    pub position: Point3D,
    pub rotation_deg: f64,
    pub mount_side: hwc_parser::MountingSide,
}

pub struct ValidationParams {
    pub untransformed_origin: Point3D,
    pub position: Point3D,
    pub rotation_deg: f64,
    pub body_min_z: i64,
    pub body_max_z: i64,
}
