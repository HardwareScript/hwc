use hwc_engine::{
    geometry::{BoundingBox, Point3D},
    space::ContactMetadata,
};
use hwc_parser::{ContactPlacement, Coordinate, Expression, Span, Unit};

use super::{AutoViaInserter, LayerTransition, OverlapRegion, ViaType};
use crate::ir::errors::IrError;

pub(crate) struct ViaPlacementParams<'a> {
    pub(crate) x_nm: i64,
    pub(crate) y_nm: i64,
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) bridge_stack: &'a crate::bridge_resolver::BridgeStack,
}

impl AutoViaInserter {
    pub(crate) fn create_via_placement_at(
        &self,
        transition: &LayerTransition,
        via_type: &ViaType,
        params: &ViaPlacementParams<'_>,
    ) -> ContactPlacement {
        let via_name = if params.row == 0 && params.col == 0 {
            format!(
                "AutoVia_{}_{}_{}",
                transition.net_name, transition.from_layer, transition.to_layer
            )
        } else {
            format!(
                "AutoVia_{}_{}_{}_r{}c{}",
                transition.net_name,
                transition.from_layer,
                transition.to_layer,
                params.row,
                params.col
            )
        };

        let span = Span::new(0, 0);
        let x_mm = params.x_nm as f64 / 1_000_000.0;
        let y_mm = params.y_nm as f64 / 1_000_000.0;
        let z_mm = transition.from_z_nm as f64 / 1_000_000.0;

        let from_elevation = if let Some(ref name) = transition.from_layer_name {
            hwc_parser::ast::Elevation::Semantic(hwc_parser::ast::Identifier::new(
                name.clone(),
                span,
            ))
        } else {
            crate::ir::stackup_manager::StackupManager::elevation_from_z_nm(
                transition.from_z_nm,
                span,
            )
        };
        let to_elevation = if let Some(ref name) = transition.to_layer_name {
            hwc_parser::ast::Elevation::Semantic(hwc_parser::ast::Identifier::new(
                name.clone(),
                span,
            ))
        } else {
            crate::ir::stackup_manager::StackupManager::elevation_from_z_nm(
                transition.to_z_nm,
                span,
            )
        };

        ContactPlacement {
            material: params.bridge_stack.fill_material.clone(),
            name: Some(hwc_parser::ComponentName::simple(via_name.into(), span)),
            position: Coordinate::Declarative {
                x: Expression::Measurement {
                    value: x_mm,
                    unit: Unit::Millimeter,
                    span,
                },
                y: Expression::Measurement {
                    value: y_mm,
                    unit: Unit::Millimeter,
                    span,
                },
                z: Expression::Measurement {
                    value: z_mm,
                    unit: Unit::Millimeter,
                    span,
                },
                span,
            },
            from_elevation,
            to_elevation,
            net: Some(hwc_parser::NetName::simple(
                transition.net_name.clone(),
                span,
            )),
            properties: {
                let mut props = rustc_hash::FxHashMap::default();
                props.insert(
                    "diameter".into(),
                    Expression::Measurement {
                        value: via_type.diameter_mm,
                        unit: Unit::Millimeter,
                        span,
                    },
                );
                if params.bridge_stack.interface_material != params.bridge_stack.fill_material {
                    props.insert(
                        "bridge".into(),
                        Expression::Variable {
                            name: params.bridge_stack.interface_material.clone(),
                            span,
                        },
                    );
                }
                props
            },
            contour: Some(via_type.contour.clone()),
            span,
        }
    }

    pub(crate) fn create_via_placement(
        &self,
        transition: &LayerTransition,
        overlap: &OverlapRegion,
        via_type: &ViaType,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> ContactPlacement {
        self.create_via_placement_at(
            transition,
            via_type,
            &ViaPlacementParams {
                x_nm: overlap.center_x_nm,
                y_nm: overlap.center_y_nm,
                row: 0,
                col: 0,
                bridge_stack,
            },
        )
    }

    pub(crate) fn create_contact_metadata_for_via(
        &self,
        via: &ContactPlacement,
        transition: &LayerTransition,
    ) -> Result<ContactMetadata, IrError> {
        let (x_mm, y_mm) = self.placement_xy_mm(via, transition);
        let x_nm = (x_mm * 1_000_000.0) as i64;
        let y_nm = (y_mm * 1_000_000.0) as i64;

        let diameter_nm = via
            .properties
            .get("diameter")
            .and_then(|expr| expr.evaluate_const().ok())
            .and_then(|val| val.to_nanometers().ok())
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: format!(
                    "Via '{}' has no explicit diameter — the via definition is malformed.",
                    via.name.as_ref().map(|n| n.to_string()).unwrap_or_else(|| "unnamed".into())
                ),
                hint: "Ensure all via definitions include a valid 'diameter' property with a numeric value.".into(),
            })?;

        let radius_nm = diameter_nm / 2;

        let bridge = via.properties.get("bridge").and_then(|expr| match expr {
            Expression::Variable { name, .. } => Some(name.clone()),
            _ => None,
        });

        Ok(ContactMetadata {
            name: via
                .name
                .as_ref()
                .map(|name| name.to_string())
                .unwrap_or_else(|| "AutoVia".into()),
            material_name: via.material.clone(),
            z_start_nm: transition.from_z_nm,
            z_end_nm: transition.to_z_nm,
            net: via.net.as_ref().map(|net| net.to_string()),
            bridge,
            bbox: Some(BoundingBox::new(
                Point3D::new(x_nm - radius_nm, y_nm - radius_nm, transition.from_z_nm),
                Point3D::new(x_nm + radius_nm, y_nm + radius_nm, transition.to_z_nm),
            )),
            drill_diameter_nm: Some(diameter_nm),
            voxels: Vec::new(),
            is_tented: false,
            mask_clearance_diameter_nm: None,
        })
    }

    fn placement_xy_mm(&self, via: &ContactPlacement, transition: &LayerTransition) -> (f64, f64) {
        match &via.position {
            Coordinate::Declarative { x, y, .. } => {
                let x_mm = match x {
                    Expression::Measurement { value, .. } => *value,
                    Expression::Literal { value, .. } => *value as f64,
                    _ => transition.from_bbox.min.x as f64 / 1_000_000.0,
                };
                let y_mm = match y {
                    Expression::Measurement { value, .. } => *value,
                    Expression::Literal { value, .. } => *value as f64,
                    _ => transition.from_bbox.min.y as f64 / 1_000_000.0,
                };
                (x_mm, y_mm)
            }
            _ => {
                let overlap_min_x = transition.from_bbox.min.x.max(transition.to_bbox.min.x);
                let overlap_max_x = transition.from_bbox.max.x.min(transition.to_bbox.max.x);
                let overlap_min_y = transition.from_bbox.min.y.max(transition.to_bbox.min.y);
                let overlap_max_y = transition.from_bbox.max.y.min(transition.to_bbox.max.y);

                (
                    (overlap_min_x + overlap_max_x) as f64 / 2_000_000.0,
                    (overlap_min_y + overlap_max_y) as f64 / 2_000_000.0,
                )
            }
        }
    }
}
