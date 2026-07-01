use clipper2_rust::{Path64, Point64};

use super::csg_eval::evaluate_csg_expression;
use super::eval_env::{evaluate_expr, EvalEnv};
use super::math_parser::evaluate_nm_expr;

pub fn evaluate_shape_points(
    shape_def: &hwc_parser::ShapeDefinition,
    via_diameter_nm: i64,
    constants: &rustc_hash::FxHashMap<compact_str::CompactString, f64>,
) -> Path64 {
    if let Some(ref csg) = shape_def.csg {
        let params: Vec<(&str, i64)> = shape_def
            .parameters
            .iter()
            .map(|p| {
                let val = via_diameter_nm;
                (p.name.name.as_str(), val)
            })
            .collect();

        return evaluate_csg_expression(csg, &params);
    }

    if let Some(ref generator) = shape_def.generator {
        return evaluate_generator(generator, via_diameter_nm);
    }

    if let Some(ref geometry) = shape_def.geometry {
        let params: Vec<(&str, i64)> = shape_def
            .parameters
            .iter()
            .map(|p| {
                let val = via_diameter_nm;
                (p.name.name.as_str(), val)
            })
            .collect();

        let generated_points = evaluate_geometry_blocks(geometry, &params, constants);
        let mut contour = Path64::new();
        for point in generated_points {
            contour.push(point);
        }
        return contour;
    }

    let params: Vec<(&str, i64)> = shape_def
        .parameters
        .iter()
        .map(|p| {
            let val = via_diameter_nm;
            (p.name.name.as_str(), val)
        })
        .collect();

    let mut contour = Path64::new();
    for point in &shape_def.points {
        let x = evaluate_nm_expr(&point.x_expr, &params);
        let y = evaluate_nm_expr(&point.y_expr, &params);
        contour.push(Point64::new(x, y));
    }
    contour
}

fn evaluate_generator(generator: &hwc_parser::ShapeGenerator, via_diameter_nm: i64) -> Path64 {
    let params: Vec<(&str, i64)> = vec![("width", via_diameter_nm), ("w", via_diameter_nm)];

    match generator.name.as_str() {
        "StarGenerator" => {
            let points =
                evaluate_generator_param(generator.params.get("points"), &params, 16) as usize;
            let outer = evaluate_generator_param(
                generator.params.get("outer"),
                &params,
                via_diameter_nm / 2,
            );
            let inner = evaluate_generator_param(
                generator.params.get("inner"),
                &params,
                via_diameter_nm / 4,
            );
            crate::shape_generators::star_generator_contour(outer, inner, points)
        }
        "GearGenerator" => {
            let teeth =
                evaluate_generator_param(generator.params.get("teeth"), &params, 12) as usize;
            let outer = evaluate_generator_param(
                generator.params.get("outer"),
                &params,
                via_diameter_nm / 2,
            );
            let inner = evaluate_generator_param(
                generator.params.get("inner"),
                &params,
                (outer as f64 * 0.7) as i64,
            );
            crate::shape_generators::gear_generator_contour(outer, inner, teeth)
        }
        _ => crate::shape_generators::circle_contour(via_diameter_nm, 16),
    }
}

fn evaluate_generator_param(expr: Option<&String>, params: &[(&str, i64)], default: i64) -> i64 {
    match expr {
        Some(e) => evaluate_nm_expr(e, params),
        None => default,
    }
}

pub fn evaluate_geometry_blocks(
    geometry: &[hwc_parser::GeometryBlock],
    params: &[(&str, i64)],
    constants: &rustc_hash::FxHashMap<compact_str::CompactString, f64>,
) -> Vec<Point64> {
    let mut points = Vec::new();
    let mut env = EvalEnv {
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), *v as f64))
            .collect(),
        vars: rustc_hash::FxHashMap::default(),
    };
    for (name, value) in constants {
        env.params.push((name.to_string(), *value));
    }

    for block in geometry {
        match block {
            hwc_parser::GeometryBlock::ForLoop {
                variable,
                start,
                end,
                body,
            } => {
                let start_val = evaluate_expr(start, &env) as i64;
                let end_val = evaluate_expr(end, &env) as i64;
                for i in start_val..end_val {
                    env.vars.insert(variable.clone(), i as f64);
                    evaluate_geometry_statements(body, &mut env, &mut points);
                }
            }
            hwc_parser::GeometryBlock::LetBinding { name, value } => {
                let val = evaluate_expr(value, &env);
                env.vars.insert(name.clone(), val);
            }
        }
    }
    points
}

fn evaluate_geometry_statements(
    stmts: &[hwc_parser::GeometryStatement],
    env: &mut EvalEnv,
    points: &mut Vec<Point64>,
) {
    for stmt in stmts {
        match stmt {
            hwc_parser::GeometryStatement::LetBinding { name, value } => {
                let val = evaluate_expr(value, env);
                env.vars.insert(name.clone(), val);
            }
            hwc_parser::GeometryStatement::Point { x, y } => {
                let x_val = evaluate_expr(x, env);
                let y_val = evaluate_expr(y, env);
                points.push(Point64::new(x_val.round() as i64, y_val.round() as i64));
            }
            hwc_parser::GeometryStatement::GeneratorCall { .. } => {
                // Generator calls are evaluated during shape construction, not here
            }
        }
    }
}
