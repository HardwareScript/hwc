pub mod vector;
pub mod terminal_geometry;
pub mod device_geometry_context;

pub use vector::{Vector2D, ConductionFlux};
pub use terminal_geometry::TerminalGeometry;
pub use device_geometry_context::DeviceGeometryContext;

/// Fundamental physical constants
pub const EPSILON_0: f64 = 8.854_187_8128e-12;

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_engine::geometry::{BoundingBox, Point3D};
    use hwc_engine::space::PourMetadata;
    use hwc_parser::ast::device::{ManifoldExpr, MetricExpression};
    use rustc_hash::FxHashMap;

    #[test]
    fn test_vector_and_flux_calculus() {
        let v1 = Vector2D::new(0.0, 0.0);
        let v2 = Vector2D::new(1000.0, 0.0);
        let flux = ConductionFlux::from_centroids(v1, v2).unwrap();

        assert_eq!(flux.unit_flux, Vector2D::new(1.0, 0.0));
        assert_eq!(flux.unit_transverse, Vector2D::new(-0.0, 1.0));
    }

    #[test]
    fn test_dogbone_gate_clipping_exact_w_and_l() {
        let source_bbox = BoundingBox::new(
            Point3D::new(9275, 5500, 0),
            Point3D::new(9925, 6500, 150),
        );
        let source_pour = PourMetadata {
            name: "Source_Diff".into(),
            material_name: "N_Plus_Diffusion".into(),
            net: Some("Source".into()),
            bbox: Some(source_bbox),
            z_bottom_nm: 0,
            device_binding: None,
            pour_id: 1,
            is_copper: true,
        };

        let drain_bbox = BoundingBox::new(
            Point3D::new(10075, 5500, 0),
            Point3D::new(10725, 6500, 150),
        );
        let drain_pour = PourMetadata {
            name: "Drain_Diff".into(),
            material_name: "N_Plus_Diffusion".into(),
            net: Some("Drain".into()),
            bbox: Some(drain_bbox),
            z_bottom_nm: 0,
            device_binding: None,
            pour_id: 2,
            is_copper: true,
        };

        let gate_stem_bbox = BoundingBox::new(
            Point3D::new(9925, 4700, 180),
            Point3D::new(10075, 7300, 360),
        );
        let gate_stem_pour = PourMetadata {
            name: "Gate_Poly".into(),
            material_name: "Polysilicon".into(),
            net: Some("Gate".into()),
            bbox: Some(gate_stem_bbox),
            z_bottom_nm: 180,
            device_binding: None,
            pour_id: 3,
            is_copper: true,
        };

        let gate_head_bbox = BoundingBox::new(
            Point3D::new(9800, 6900, 180),
            Point3D::new(10200, 7300, 360),
        );
        let gate_head_pour = PourMetadata {
            name: "Gate_Poly_Head".into(),
            material_name: "Polysilicon".into(),
            net: Some("Gate".into()),
            bbox: Some(gate_head_bbox),
            z_bottom_nm: 180,
            device_binding: None,
            pour_id: 4,
            is_copper: true,
        };

        let mut terminal_pours = FxHashMap::default();
        terminal_pours.insert("S".into(), vec![source_pour]);
        terminal_pours.insert("D".into(), vec![drain_pour]);
        terminal_pours.insert("G".into(), vec![gate_stem_pour, gate_head_pour]);

        let ctx = DeviceGeometryContext::new("M1", &terminal_pours, None).unwrap();

        let mut metrics = FxHashMap::default();
        let channel_manifold = ManifoldExpr::Intersect(
            Box::new(ManifoldExpr::Terminal("G".into())),
            Box::new(ManifoldExpr::Hull(
                Box::new(ManifoldExpr::Terminal("S".into())),
                Box::new(ManifoldExpr::Terminal("D".into())),
            )),
        );

        metrics.insert("L".into(), MetricExpression::SpanAlongFlux {
            manifold: channel_manifold.clone(),
            from: "S".into(),
            to: "D".into(),
        });
        metrics.insert("W".into(), MetricExpression::SpanAlongTransverse {
            manifold: channel_manifold,
            from: "S".into(),
            to: "D".into(),
        });
        metrics.insert("AD".into(), MetricExpression::Area(ManifoldExpr::Difference(
            Box::new(ManifoldExpr::Terminal("D".into())),
            Box::new(ManifoldExpr::Terminal("G".into())),
        )));
        metrics.insert("AS".into(), MetricExpression::Area(ManifoldExpr::Difference(
            Box::new(ManifoldExpr::Terminal("S".into())),
            Box::new(ManifoldExpr::Terminal("G".into())),
        )));
        metrics.insert("PD".into(), MetricExpression::Perimeter(ManifoldExpr::Difference(
            Box::new(ManifoldExpr::Terminal("D".into())),
            Box::new(ManifoldExpr::Terminal("G".into())),
        )));
        metrics.insert("PS".into(), MetricExpression::Perimeter(ManifoldExpr::Difference(
            Box::new(ManifoldExpr::Terminal("S".into())),
            Box::new(ManifoldExpr::Terminal("G".into())),
        )));
        metrics.insert("SA".into(), MetricExpression::SpanAlongFlux {
            manifold: ManifoldExpr::Difference(
                Box::new(ManifoldExpr::Terminal("S".into())),
                Box::new(ManifoldExpr::Terminal("G".into())),
            ),
            from: "S".into(),
            to: "D".into(),
        });
        metrics.insert("SB".into(), MetricExpression::SpanAlongFlux {
            manifold: ManifoldExpr::Difference(
                Box::new(ManifoldExpr::Terminal("D".into())),
                Box::new(ManifoldExpr::Terminal("G".into())),
            ),
            from: "S".into(),
            to: "D".into(),
        });
        metrics.insert("NRD".into(), MetricExpression::Divide(
            Box::new(MetricExpression::Ref("SB".into())),
            Box::new(MetricExpression::Ref("W".into())),
        ));
        metrics.insert("NRS".into(), MetricExpression::Divide(
            Box::new(MetricExpression::Ref("SA".into())),
            Box::new(MetricExpression::Ref("W".into())),
        ));

        let results = ctx.evaluate_all_metrics(&metrics).unwrap();

        assert_eq!(results.get("L").unwrap().to_spice_repr(), "0.15u");
        assert_eq!(results.get("W").unwrap().to_spice_repr(), "1.00u");
        assert_eq!(results.get("AD").unwrap().to_spice_repr(), "0.65p");
        assert_eq!(results.get("AS").unwrap().to_spice_repr(), "0.65p");
        assert_eq!(results.get("PD").unwrap().to_spice_repr(), "3.30u");
        assert_eq!(results.get("PS").unwrap().to_spice_repr(), "3.30u");
        assert_eq!(results.get("SA").unwrap().to_spice_repr(), "0.65u");
        assert_eq!(results.get("SB").unwrap().to_spice_repr(), "0.65u");
        assert_eq!(results.get("NRD").unwrap().to_spice_repr(), "0.65");
        assert_eq!(results.get("NRS").unwrap().to_spice_repr(), "0.65");
    }

    #[test]
    fn test_resistor_span_and_transverse_width() {
        let body_bbox = BoundingBox::new(
            Point3D::new(8000, 4295, 0),
            Point3D::new(12000, 5705, 180),
        );
        let body_pour = PourMetadata {
            name: "Resistor_Body".into(),
            material_name: "Polysilicon".into(),
            net: None,
            bbox: Some(body_bbox),
            z_bottom_nm: 0,
            device_binding: None,
            pour_id: 1,
            is_copper: true,
        };

        let contact_a_bbox = BoundingBox::new(
            Point3D::new(8000, 4295, 180),
            Point3D::new(8400, 5705, 280),
        );
        let contact_a_pour = PourMetadata {
            name: "Contact_A".into(),
            material_name: "Titanium_Silicide".into(),
            net: Some("In".into()),
            bbox: Some(contact_a_bbox),
            z_bottom_nm: 180,
            device_binding: None,
            pour_id: 2,
            is_copper: true,
        };

        let contact_b_bbox = BoundingBox::new(
            Point3D::new(11600, 4295, 180),
            Point3D::new(12000, 5705, 280),
        );
        let contact_b_pour = PourMetadata {
            name: "Contact_B".into(),
            material_name: "Titanium_Silicide".into(),
            net: Some("Out".into()),
            bbox: Some(contact_b_bbox),
            z_bottom_nm: 180,
            device_binding: None,
            pour_id: 3,
            is_copper: true,
        };

        let mut terminal_pours = FxHashMap::default();
        terminal_pours.insert("A".into(), vec![body_pour.clone(), contact_a_pour]);
        terminal_pours.insert("B".into(), vec![body_pour, contact_b_pour]);

        let ctx = DeviceGeometryContext::new("R1", &terminal_pours, None).unwrap();

        let union_ab = ManifoldExpr::Union(
            Box::new(ManifoldExpr::Terminal("A".into())),
            Box::new(ManifoldExpr::Terminal("B".into())),
        );

        let expr_l = MetricExpression::SpanAlongFlux {
            manifold: union_ab.clone(),
            from: "A".into(),
            to: "B".into(),
        };
        let l_qty = ctx.evaluate_metric_expr(&expr_l).unwrap();
        assert_eq!(l_qty.to_spice_repr(), "4.00u");

        let expr_w = MetricExpression::SpanAlongTransverse {
            manifold: union_ab,
            from: "A".into(),
            to: "B".into(),
        };
        let w_qty = ctx.evaluate_metric_expr(&expr_w).unwrap();
        assert_eq!(w_qty.to_spice_repr(), "1.41u");
    }
}
