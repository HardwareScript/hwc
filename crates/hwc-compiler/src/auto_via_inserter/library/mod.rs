mod csg_eval;
mod eval_env;
mod math_parser;
mod shape_eval;

pub use shape_eval::{evaluate_geometry_blocks, evaluate_shape_points, is_conductive_material};

use clipper2_rust::Path64;
use compact_str::CompactString;

use shape_eval::is_conductive_material as _is_conductive_material;

/// Via type definition for standard via library.
/// The compiler only understands polygons (Path64 contours), not named shapes.
#[derive(Debug, Clone)]
pub struct ViaType {
    pub name: CompactString,
    pub material: CompactString,
    pub from_layer: usize,
    pub to_layer: usize,
    pub diameter_mm: f64,
    pub min_enclosure_mm: f64,
    pub z_start_nm: i64,
    pub z_end_nm: i64,
    pub contour: Path64,
}

impl ViaType {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: CompactString,
        material: CompactString,
        from_layer: usize,
        to_layer: usize,
        diameter_mm: f64,
        min_enclosure_mm: f64,
        z_start_nm: i64,
        z_end_nm: i64,
        contour: Path64,
    ) -> Self {
        Self {
            name,
            material,
            from_layer,
            to_layer,
            diameter_mm,
            min_enclosure_mm,
            z_start_nm,
            z_end_nm,
            contour,
        }
    }
}

/// Standard via library with common via types.
pub struct ViaLibrary {
    pub(crate) vias: Vec<ViaType>,
}

impl ViaLibrary {
    /// Create a via library from a profile definition.
    ///
    /// If the profile defines explicit `via:` blocks, those are used directly.
    /// For ASIC profiles with no explicit vias, adjacent-layer vias are
    /// auto-generated from the stackup (e.g., m1→m2, m2→m3, m3→m4).
    /// This reflects the physical reality that ASIC vias are process-defined
    /// layer-to-layer interconnects, not user-specified components.
    pub fn from_profile(
        profile: Option<&hwc_parser::ProfileDefinition>,
        stackup_manager: &crate::ir::stackup_manager::StackupManager,
        _fabrication: Option<&hwc_engine::constraint_manager::FabricationConstraints>,
        symbol_table: Option<&crate::SymbolTable>,
    ) -> Self {
        let mut vias = Vec::new();

        if let Some(profile) = profile {
            let min_diameter_mm = profile
                .via
                .as_ref()
                .map(|v| Self::measurement_to_mm(&v.min_diameter))
                .unwrap_or(0.3);
            let size_nm = (min_diameter_mm * 1_000_000.0) as i64;

            let default_contour = if let Some(shape_def) =
                profile.via.as_ref().and_then(|v| v.shape.as_ref())
            {
                let resolved = symbol_table.and_then(|st| st.get_shape(shape_def.name.as_str()));
                if let Some(def) = resolved {
                    let constants = symbol_table
                        .map(|st| st.get_all_constants())
                        .unwrap_or_default();
                    evaluate_shape_points(def, size_nm, &constants)
                } else {
                    match shape_def.name.as_str() {
                        "square" => crate::shape_generators::square_contour(size_nm),
                        "hexagon" => crate::shape_generators::hexagon_contour(size_nm),
                        "cylinder" => crate::shape_generators::circle_contour(size_nm, 16),
                        _ => panic!(
                            "Via shape '{}' is not defined. Declare it in the profile or stdlib.",
                            shape_def.name
                        ),
                    }
                }
            } else {
                panic!(
                    "Profile '{}': via.shape must be explicitly declared. No implicit shape defaults permitted.",
                    profile.name
                )
            };

            for via_def in &profile.vias {
                let from_layer = stackup_manager.get_index_for_layer(via_def.from_layer.as_str());
                let to_layer = stackup_manager.get_index_for_layer(via_def.to_layer.as_str());

                if let (Some(from), Some(to)) = (from_layer, to_layer) {
                    let z_start = stackup_manager.get_z_start_nm_for_layer_index(from);
                    let z_end = stackup_manager.get_z_start_nm_for_layer_index(to);

                    vias.push(ViaType::new(
                        via_def.name.name.clone(),
                        via_def
                            .material
                            .as_ref()
                            .map(|material| material.name.clone())
                            .unwrap_or_else(|| panic!(
                                "Via '{}' in profile '{}' has no material. Declare via.material in the profile.",
                                via_def.name.name, profile.name
                            )),
                        from,
                        to,
                        Self::measurement_to_mm(&via_def.diameter),
                        Self::measurement_to_mm(&via_def.annular_ring),
                        z_start,
                        z_end,
                        default_contour.clone(),
                    ));
                }
            }

            if vias.is_empty() {
                if let Some(stackup) = &profile.stackup {
                    let min_diameter_mm = profile
                        .via
                        .as_ref()
                        .map(|v| Self::measurement_to_mm(&v.min_diameter))
                        .unwrap_or(0.3);

                    let min_annular_ring_mm = profile
                        .via
                        .as_ref()
                        .map(|v| Self::measurement_to_mm(&v.min_annular_ring))
                        .unwrap_or(0.15);

                    let conductive_indices: Vec<usize> = if profile.is_asic() {
                        // v0.1.8: ASIC profiles need vias between ALL stackup layers,
                        // not just conductive ones. The via stack must connect through
                        // all intermediate layers (active→poly→metal1, etc.).
                        // Exclude only the substrate layer (index 0, routable: false).
                        stackup
                            .layers
                            .iter()
                            .enumerate()
                            .skip(1) // Skip substrate
                            .map(|(i, _)| i)
                            .collect()
                    } else {
                        stackup
                            .layers
                            .iter()
                            .enumerate()
                            .filter(|(_, layer)| _is_conductive_material(&layer.material))
                            .map(|(i, _)| i)
                            .collect()
                    };

                    if profile.is_asic() {
                        // ASIC: auto-generate adjacent-layer vias (m1→m2, m2→m3, etc.)
                        for window in conductive_indices.windows(2) {
                            let from_idx = window[0];
                            let to_idx = window[1];
                            let from_layer = &stackup.layers[from_idx];
                            let to_layer = &stackup.layers[to_idx];

                            let from_stackup_idx =
                                stackup_manager.get_index_for_layer(&from_layer.name.name);
                            let to_stackup_idx =
                                stackup_manager.get_index_for_layer(&to_layer.name.name);

                            if let (Some(from), Some(to)) = (from_stackup_idx, to_stackup_idx) {
                                let z_start =
                                    stackup_manager.get_z_start_nm_for_layer_index(from);
                                let z_end = stackup_manager.get_z_start_nm_for_layer_index(to);

                                let via_name = format!(
                                    "via_{}_{}",
                                    from_layer.name.name, to_layer.name.name
                                );

                                // println!(
                                //     "   │  [LIB] Auto-gen via '{}': L{}→L{}, z {}nm→{}nm, \
                                //      dia {:.3}mm, ring {:.3}mm",
                                //     via_name, from, to, z_start, z_end, min_diameter_mm,
                                //     min_annular_ring_mm
                                // );

                                vias.push(ViaType::new(
                                    via_name.into(),
                                    from_layer.material.clone(),
                                    from,
                                    to,
                                    min_diameter_mm,
                                    min_annular_ring_mm,
                                    z_start,
                                    z_end,
                                    default_contour.clone(),
                                ));
                            }
                        }
                    } else if conductive_indices.len() >= 2 {
                        // PCB: auto-generate a single through-hole via spanning
                        // from the bottom conductive layer to the top conductive layer.
                        // This reflects the physical reality that PCB through-holes are
                        // drilled through the entire board, not layer-by-layer.
                        let bottom_idx = conductive_indices[0];
                        let top_idx = *conductive_indices.last().unwrap();
                        let bottom_layer = &stackup.layers[bottom_idx];
                        let top_layer = &stackup.layers[top_idx];

                        let bottom_stackup_idx =
                            stackup_manager.get_index_for_layer(&bottom_layer.name.name);
                        let top_stackup_idx =
                            stackup_manager.get_index_for_layer(&top_layer.name.name);

                        if let (Some(bottom), Some(top)) =
                            (bottom_stackup_idx, top_stackup_idx)
                        {
                            let z_start =
                                stackup_manager.get_z_start_nm_for_layer_index(bottom);
                            let z_end = stackup_manager.get_z_start_nm_for_layer_index(top);

                            let via_name = format!(
                                "via_through_hole_{}_{}",
                                bottom_layer.name.name, top_layer.name.name
                            );

                            // println!(
                            //     "   │  [LIB] Auto-gen through-hole via '{}': L{}→L{}, \
                            //      z {}nm→{}nm, dia {:.3}mm, ring {:.3}mm",
                            //     via_name, bottom, top, z_start, z_end, min_diameter_mm,
                            //     min_annular_ring_mm
                            // );

                            vias.push(ViaType::new(
                                via_name.into(),
                                bottom_layer.material.clone(),
                                bottom,
                                top,
                                min_diameter_mm,
                                min_annular_ring_mm,
                                z_start,
                                z_end,
                                default_contour.clone(),
                            ));
                        }
                    }
                }
            }
        }

        Self { vias }
    }

    fn measurement_to_mm(measurement: &hwc_parser::Measurement) -> f64 {
        match measurement.unit {
            hwc_parser::Unit::Millimeter => measurement.value,
            hwc_parser::Unit::Micrometer => measurement.value / 1000.0,
            hwc_parser::Unit::Nanometer => measurement.value / 1_000_000.0,
            hwc_parser::Unit::Centimeter => measurement.value * 10.0,
            _ => panic!(
                "measurement_to_mm: cannot convert {:?} to millimeters (not a length unit)",
                measurement.unit
            ),
        }
    }

    /// Find the appropriate via type for a layer pair.
    pub fn find_via_for_layers(
        &self,
        from_layer: usize,
        to_layer: usize,
        prefer_large: bool,
    ) -> Option<&ViaType> {
        let (start, end) = if from_layer < to_layer {
            (from_layer, to_layer)
        } else {
            (to_layer, from_layer)
        };

        let mut matches: Vec<&ViaType> = self
            .vias
            .iter()
            .filter(|via| {
                let exact = via.from_layer == start && via.to_layer == end;
                let spanning_through_hole =
                    via.from_layer == 0 && via.to_layer >= end && start >= via.from_layer;
                exact || spanning_through_hole
            })
            .collect();

        if matches.is_empty() {
            return None;
        }

        matches.sort_by(|a, b| a.diameter_mm.partial_cmp(&b.diameter_mm).unwrap());

        if prefer_large {
            matches.last().copied()
        } else {
            matches.first().copied()
        }
    }

    /// Find a via type by its exact Z-span.
    pub fn find_via_by_z_span(&self, z_start_nm: i64, z_end_nm: i64) -> Option<&ViaType> {
        let (start, end) = if z_start_nm < z_end_nm {
            (z_start_nm, z_end_nm)
        } else {
            (z_end_nm, z_start_nm)
        };

        self.vias.iter().find(|via| {
            let (v_start, v_end) = if via.z_start_nm < via.z_end_nm {
                (via.z_start_nm, via.z_end_nm)
            } else {
                (via.z_end_nm, via.z_start_nm)
            };
            v_start == start && v_end == end
        })
    }
}

#[cfg(test)]
mod tests {
    use super::math_parser::*;

    fn params(width_nm: i64) -> Vec<(&'static str, i64)> {
        vec![("width", width_nm)]
    }

    fn params_custom(name: &str, val: i64) -> Vec<(&str, i64)> {
        vec![(name, val)]
    }

    #[test]
    fn test_basic_integer() {
        assert_eq!(evaluate_pure_math("42"), 42);
        assert_eq!(evaluate_pure_math("0"), 0);
    }

    #[test]
    fn test_measurement_nm() {
        assert_eq!(evaluate_pure_math("100nm"), 100);
        assert_eq!(evaluate_pure_math("1000nm"), 1000);
    }

    #[test]
    fn test_measurement_um() {
        assert_eq!(evaluate_pure_math("1um"), 1000);
        assert_eq!(evaluate_pure_math("5.5um"), 5500);
    }

    #[test]
    fn test_measurement_mm() {
        assert_eq!(evaluate_pure_math("1mm"), 1_000_000);
        assert_eq!(evaluate_pure_math("0.5mm"), 500_000);
    }

    #[test]
    fn test_unary_negation() {
        assert_eq!(evaluate_pure_math("-42"), -42);
        assert_eq!(evaluate_pure_math("-100nm"), -100);
    }

    #[test]
    fn test_unary_plus() {
        assert_eq!(evaluate_pure_math("+42"), 42);
        assert_eq!(evaluate_pure_math("+100nm"), 100);
    }

    #[test]
    fn test_division() {
        assert_eq!(evaluate_pure_math("100 / 4"), 25);
        assert_eq!(evaluate_pure_math("500000 / 2.5"), 200000);
    }

    #[test]
    fn test_multiplication() {
        assert_eq!(evaluate_pure_math("100 * 3"), 300);
    }

    #[test]
    fn test_binary_addition() {
        assert_eq!(evaluate_pure_math("100 + 200"), 300);
        assert_eq!(evaluate_pure_math("500000 + 100000"), 600000);
    }

    #[test]
    fn test_binary_subtraction() {
        assert_eq!(evaluate_pure_math("500 - 200"), 300);
        assert_eq!(evaluate_pure_math("500000 - 100000"), 400000);
    }

    #[test]
    fn test_complex_expression() {
        let p = params(500_000);
        assert_eq!(evaluate_nm_expr("-width / 2", &p), -250_000);
    }

    #[test]
    fn test_parameter_substitution() {
        let p = params(500_000);
        assert_eq!(evaluate_nm_expr("width", &p), 500_000);
        assert_eq!(evaluate_nm_expr("width / 2", &p), 250_000);
        assert_eq!(evaluate_nm_expr("width / 2.5", &p), 200_000);
    }

    #[test]
    fn test_custom_parameter_name() {
        let p = params_custom("r", 400_000);
        assert_eq!(evaluate_nm_expr("r", &p), 400_000);
        assert_eq!(evaluate_nm_expr("r / 2", &p), 200_000);
        assert_eq!(evaluate_nm_expr("-r / 4", &p), -100_000);
    }

    #[test]
    fn test_division_by_zero() {
        assert_eq!(evaluate_pure_math("100 / 0"), 0);
    }

    #[test]
    fn test_complex_nested_expressions() {
        let p = params(500_000);
        assert_eq!(evaluate_nm_expr("width / 2 + width / 4", &p), 375_000);
        assert_eq!(evaluate_nm_expr("width - width / 4", &p), 375_000);
    }

    #[test]
    fn test_measurement_in_expressions() {
        let p = params(500_000);
        assert_eq!(evaluate_nm_expr("width / 2 + 100nm", &p), 250_100);
    }

    #[test]
    fn test_negative_results() {
        let p = params(500_000);
        assert_eq!(evaluate_nm_expr("-width / 2", &p), -250_000);
        assert_eq!(evaluate_nm_expr("0 - width", &p), -500_000);
    }

    #[test]
    fn test_deg_suffix() {
        assert_eq!(evaluate_pure_math("90deg"), 90);
        assert_eq!(evaluate_pure_math("180deg"), 180);
        assert_eq!(evaluate_pure_math("11.25deg"), 11);
    }

    #[test]
    fn test_measurement_deg() {
        assert_eq!(evaluate_pure_math("11.25deg"), 11);
    }

    #[test]
    fn test_if_else_expression() {
        assert_eq!(evaluate_pure_math("if 1 = 1: 100 else: 200"), 100);
        assert_eq!(evaluate_pure_math("if 1 = 2: 100 else: 200"), 200);
    }

    #[test]
    fn test_mod_operator() {
        assert_eq!(evaluate_pure_math("5 mod 2"), 1);
        assert_eq!(evaluate_pure_math("10 mod 3"), 1);
        assert_eq!(evaluate_pure_math("6 mod 2"), 0);
    }

    #[test]
    fn test_if_with_mod() {
        assert_eq!(evaluate_pure_math("if 4 mod 2 = 0: 100 else: 200"), 100);
        assert_eq!(evaluate_pure_math("if 5 mod 2 = 0: 100 else: 200"), 200);
    }

    #[test]
    fn test_evaluate_expr_literal() {
        use super::eval_env::*;
        use hwc_parser::expr::Expr;
        let env = EvalEnv {
            params: vec![],
            vars: rustc_hash::FxHashMap::default(),
        };
        assert_eq!(evaluate_expr(&Expr::Literal(42.0), &env), 42.0);
    }

    #[test]
    fn test_evaluate_expr_identifier() {
        use super::eval_env::*;
        use hwc_parser::expr::Expr;
        let env = EvalEnv {
            params: vec![("width".to_string(), 500_000.0)],
            vars: rustc_hash::FxHashMap::default(),
        };
        assert_eq!(
            evaluate_expr(&Expr::Identifier("width".to_string()), &env),
            500_000.0
        );
    }

    #[test]
    fn test_evaluate_expr_binop() {
        use super::eval_env::*;
        use hwc_parser::expr::{BinOp, Expr};
        let env = EvalEnv {
            params: vec![("width".to_string(), 500_000.0)],
            vars: rustc_hash::FxHashMap::default(),
        };
        let expr = Expr::BinOp {
            op: BinOp::Div,
            left: Box::new(Expr::Identifier("width".to_string())),
            right: Box::new(Expr::Literal(2.0)),
        };
        assert_eq!(evaluate_expr(&expr, &env), 250_000.0);
    }

    #[test]
    fn test_evaluate_expr_trig() {
        use super::eval_env::*;
        use hwc_parser::expr::Expr;
        let env = EvalEnv {
            params: vec![],
            vars: rustc_hash::FxHashMap::default(),
        };
        let expr = Expr::Call {
            name: "sin".to_string(),
            args: vec![Expr::Literal(90.0)],
        };
        assert!((evaluate_expr(&expr, &env) - 1.0).abs() < 0.001);

        let expr = Expr::Call {
            name: "cos".to_string(),
            args: vec![Expr::Literal(0.0)],
        };
        assert!((evaluate_expr(&expr, &env) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_evaluate_expr_if_else() {
        use super::eval_env::*;
        use hwc_parser::expr::Expr;
        let env = EvalEnv {
            params: vec![],
            vars: rustc_hash::FxHashMap::default(),
        };
        let expr = Expr::If {
            cond: Box::new(Expr::Literal(1.0)),
            then_branch: Box::new(Expr::Literal(100.0)),
            else_branch: Box::new(Expr::Literal(200.0)),
        };
        assert_eq!(evaluate_expr(&expr, &env), 100.0);

        let expr = Expr::If {
            cond: Box::new(Expr::Literal(0.0)),
            then_branch: Box::new(Expr::Literal(100.0)),
            else_branch: Box::new(Expr::Literal(200.0)),
        };
        assert_eq!(evaluate_expr(&expr, &env), 200.0);
    }

    #[test]
    fn test_evaluate_expr_mod_equals() {
        use super::eval_env::*;
        use hwc_parser::expr::Expr;
        let env = EvalEnv {
            params: vec![],
            vars: rustc_hash::FxHashMap::default(),
        };
        let expr = Expr::ModEquals {
            dividend: Box::new(Expr::Literal(4.0)),
            divisor: Box::new(Expr::Literal(2.0)),
            remainder: Box::new(Expr::Literal(0.0)),
        };
        assert_eq!(evaluate_expr(&expr, &env), 1.0);

        let expr = Expr::ModEquals {
            dividend: Box::new(Expr::Literal(5.0)),
            divisor: Box::new(Expr::Literal(2.0)),
            remainder: Box::new(Expr::Literal(0.0)),
        };
        assert_eq!(evaluate_expr(&expr, &env), 0.0);
    }
}
