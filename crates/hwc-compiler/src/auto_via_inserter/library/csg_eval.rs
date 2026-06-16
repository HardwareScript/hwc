use clipper2_rust::{boolean_op_64, ClipType, FillRule, Path64, Point64};
use hwc_parser::{CsgExpression, CsgPrimitive};

use super::math_parser::evaluate_nm_expr;

pub fn evaluate_csg_expression(expr: &CsgExpression, params: &[(&str, i64)]) -> Path64 {
    evaluate_csg_expression_with_vars(expr, params, &rustc_hash::FxHashMap::default())
}

fn evaluate_csg_expression_with_vars(
    expr: &CsgExpression,
    params: &[(&str, i64)],
    local_vars: &rustc_hash::FxHashMap<String, Path64>,
) -> Path64 {
    match expr {
        CsgExpression::Primitive(CsgPrimitive::ShapeRef(name)) => {
            if let Some(path) = local_vars.get(name) {
                return path.clone();
            }
            evaluate_csg_primitive(&CsgPrimitive::ShapeRef(name.clone()), params)
        }
        CsgExpression::Primitive(prim) => evaluate_csg_primitive(prim, params),
        CsgExpression::Union(left, right) => {
            let left_path = evaluate_csg_expression_with_vars(left, params, local_vars);
            let right_path = evaluate_csg_expression_with_vars(right, params, local_vars);
            clipper_union(&left_path, &right_path)
        }
        CsgExpression::Difference(left, right) => {
            let left_path = evaluate_csg_expression_with_vars(left, params, local_vars);
            let right_path = evaluate_csg_expression_with_vars(right, params, local_vars);
            clipper_difference(&left_path, &right_path)
        }
        CsgExpression::Intersection(left, right) => {
            let left_path = evaluate_csg_expression_with_vars(left, params, local_vars);
            let right_path = evaluate_csg_expression_with_vars(right, params, local_vars);
            clipper_intersection(&left_path, &right_path)
        }
        CsgExpression::Transformed {
            expr,
            rotation,
            translation,
        } => {
            let mut path = evaluate_csg_expression_with_vars(expr, params, local_vars);
            if let Some(deg) = rotation {
                path = rotate_path(&path, *deg);
            }
            if let Some((dx, dy)) = translation {
                path = translate_path(&path, *dx as i64, *dy as i64);
            }
            path
        }
        CsgExpression::LetBinding { name, value, body } => {
            let value_path = evaluate_csg_expression_with_vars(value, params, local_vars);
            let mut new_vars = local_vars.clone();
            new_vars.insert(name.clone(), value_path);
            evaluate_csg_expression_with_vars(body, params, &new_vars)
        }
    }
}

fn evaluate_csg_primitive(prim: &CsgPrimitive, params: &[(&str, i64)]) -> Path64 {
    match prim {
        CsgPrimitive::Rectangle { width, height } => {
            let w = evaluate_nm_expr(width, params);
            let h = evaluate_nm_expr(height, params);
            let half_w = w / 2;
            let half_h = h / 2;
            vec![
                Point64::new(-half_w, -half_h),
                Point64::new(half_w, -half_h),
                Point64::new(half_w, half_h),
                Point64::new(-half_w, half_h),
            ]
        }
        CsgPrimitive::Circle { diameter } => {
            let d = evaluate_nm_expr(diameter, params);
            crate::shape_generators::circle_contour(d, 16)
        }
        CsgPrimitive::ShapeRef(name) => {
            eprintln!(
                "Warning: ShapeRef '{}' not yet implemented in CSG, using default circle",
                name
            );
            crate::shape_generators::circle_contour(1000, 16)
        }
    }
}

fn clipper_union(left: &Path64, right: &Path64) -> Path64 {
    let subjects = vec![left.clone()];
    let clips = vec![right.clone()];
    let result = boolean_op_64(ClipType::Union, FillRule::NonZero, &subjects, &clips);
    result.first().cloned().unwrap_or_else(|| left.clone())
}

fn clipper_difference(left: &Path64, right: &Path64) -> Path64 {
    let subjects = vec![left.clone()];
    let clips = vec![right.clone()];
    let result = boolean_op_64(ClipType::Difference, FillRule::NonZero, &subjects, &clips);
    result.first().cloned().unwrap_or_else(|| left.clone())
}

fn clipper_intersection(left: &Path64, right: &Path64) -> Path64 {
    let subjects = vec![left.clone()];
    let clips = vec![right.clone()];
    let result = boolean_op_64(ClipType::Intersection, FillRule::NonZero, &subjects, &clips);
    result.first().cloned().unwrap_or_else(|| left.clone())
}

fn rotate_path(path: &Path64, degrees: f64) -> Path64 {
    let rad = degrees * std::f64::consts::PI / 180.0;
    let cos = rad.cos();
    let sin = rad.sin();
    let mut result = Path64::new();
    for pt in path.iter() {
        let x = pt.x as f64;
        let y = pt.y as f64;
        let new_x = (x * cos - y * sin) as i64;
        let new_y = (x * sin + y * cos) as i64;
        result.push(Point64::new(new_x, new_y));
    }
    result
}

fn translate_path(path: &Path64, dx: i64, dy: i64) -> Path64 {
    let mut result = Path64::new();
    for pt in path.iter() {
        result.push(Point64::new(pt.x + dx, pt.y + dy));
    }
    result
}
