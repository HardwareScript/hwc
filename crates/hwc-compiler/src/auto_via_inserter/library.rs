use clipper2_rust::{boolean_op_64, ClipType, FillRule, Path64, Point64};
use compact_str::CompactString;
use hwc_parser::expr::{BinOp, Expr, UnaryOp};
use hwc_parser::{CsgExpression, CsgPrimitive};

pub struct EvalEnv {
    pub params: Vec<(String, f64)>,
    pub vars: rustc_hash::FxHashMap<String, f64>,
}

impl EvalEnv {
    pub fn get(&self, name: &str) -> Option<f64> {
        self.vars
            .get(name)
            .copied()
            .or_else(|| self.params.iter().find(|(k, _)| k == name).map(|(_, v)| *v))
    }
}

pub fn evaluate_expr(expr: &Expr, env: &EvalEnv) -> f64 {
    match expr {
        Expr::Literal(val) => *val,
        Expr::Identifier(name) => env.get(name).unwrap_or(0.0),
        Expr::UnaryOp { op, expr } => {
            let val = evaluate_expr(expr, env);
            match op {
                UnaryOp::Pos => val,
                UnaryOp::Neg => -val,
            }
        }
        Expr::BinOp { op, left, right } => {
            let l = evaluate_expr(left, env);
            let r = evaluate_expr(right, env);
            match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => {
                    if r == 0.0 {
                        0.0
                    } else {
                        l / r
                    }
                }
                BinOp::Mod => {
                    if r == 0.0 {
                        0.0
                    } else {
                        l % r
                    }
                }
                BinOp::Eq => {
                    if l == r {
                        1.0
                    } else {
                        0.0
                    }
                }
                BinOp::Ne => {
                    if l != r {
                        1.0
                    } else {
                        0.0
                    }
                }
                BinOp::Lt => {
                    if l < r {
                        1.0
                    } else {
                        0.0
                    }
                }
                BinOp::Gt => {
                    if l > r {
                        1.0
                    } else {
                        0.0
                    }
                }
            }
        }
        Expr::Call { name, args } => {
            let arg_val = args.first().map(|a| evaluate_expr(a, env)).unwrap_or(0.0);
            match name.as_str() {
                "sin" => arg_val.sin(),
                "cos" => arg_val.cos(),
                "tan" => arg_val.tan(),
                _ => 0.0,
            }
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let cond_val = evaluate_expr(cond, env);
            if cond_val != 0.0 {
                evaluate_expr(then_branch, env)
            } else {
                evaluate_expr(else_branch, env)
            }
        }
        Expr::ModEquals {
            dividend,
            divisor,
            remainder,
        } => {
            let d = evaluate_expr(dividend, env);
            let m = evaluate_expr(divisor, env);
            let r = evaluate_expr(remainder, env);
            if m != 0.0 && (d % m) == r {
                1.0
            } else {
                0.0
            }
        }
    }
}

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

            // Generate contour from shape definition or defaults
            let default_contour = if let Some(shape_def) = profile.via.as_ref().and_then(|v| v.shape.as_ref()) {
                // Try to resolve shape name to a ShapeDefinition from the symbol table
                let resolved = symbol_table.and_then(|st| st.get_shape(shape_def.name.as_str()));
                if let Some(def) = resolved {
                    // Evaluate the shape's points to produce a Path64 contour
                    let constants = symbol_table.map(|st| st.get_all_constants()).unwrap_or_default();
                    evaluate_shape_points(def, size_nm, &constants)
                } else {
                    // Shape name not found in symbol table — fall back to built-in generators
                    match shape_def.name.as_str() {
                        "square" => crate::shape_generators::square_contour(size_nm),
                        "hexagon" => crate::shape_generators::hexagon_contour(size_nm),
                        "cylinder" => crate::shape_generators::circle_contour(size_nm, 16),
                        _ => {
                            // Unknown shape name: fall back to default
                            if profile.is_asic() {
                                crate::shape_generators::square_contour(size_nm)
                            } else {
                                crate::shape_generators::circle_contour(size_nm, 16)
                            }
                        }
                    }
                }
            } else if profile.is_asic() {
                crate::shape_generators::square_contour(size_nm)
            } else {
                crate::shape_generators::circle_contour(size_nm, 16)
            };

            // Load user-defined vias first
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
                            .unwrap_or_else(|| "Copper".into()),
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

            // Auto-generate adjacent-layer vias for ASIC profiles when none defined
            if vias.is_empty() && profile.is_asic() {
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

                    // Identify conductive layers (metals) by material
                    let conductive_indices: Vec<usize> = stackup
                        .layers
                        .iter()
                        .enumerate()
                        .filter(|(_, layer)| is_conductive_material(&layer.material))
                        .map(|(i, _)| i)
                        .collect();

                    // Generate via for each adjacent conductive layer pair
                    for window in conductive_indices.windows(2) {
                        let from_idx = window[0];
                        let to_idx = window[1];
                        let from_layer = &stackup.layers[from_idx];
                        let to_layer = &stackup.layers[to_idx];

                        let from_stackup_idx = stackup_manager.get_index_for_layer(&from_layer.name.name);
                        let to_stackup_idx = stackup_manager.get_index_for_layer(&to_layer.name.name);

                        if let (Some(from), Some(to)) = (from_stackup_idx, to_stackup_idx) {
                            let z_start = stackup_manager.get_z_start_nm_for_layer_index(from);
                            let z_end = stackup_manager.get_z_start_nm_for_layer_index(to);

                            let via_name = format!("via_{}_{}", from_layer.name.name, to_layer.name.name);

                            println!("   │  [LIB] Auto-gen via '{}': L{}→L{}, z {}nm→{}nm, dia {:.3}mm, ring {:.3}mm",
                                via_name, from, to, z_start, z_end, min_diameter_mm, min_annular_ring_mm);

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

/// Check if a material name represents a conductive (metal) layer.
/// Used to identify which stackup layers need via connections.
fn is_conductive_material(material: &str) -> bool {
    let lower = material.to_lowercase();
    lower.contains("copper")
        || lower.contains("metal")
        || lower.contains("aluminum")
        || lower.contains("tungsten")
        || lower.contains("gold")
        || lower.contains("silver")
        || lower.contains("tungsten")
        || lower.contains("titanium")
        || lower.contains("ti")
}

/// Evaluate shape points to a Path64 contour.
///
/// Takes the full shape definition so parameter names from the user's
/// `shape Foo(r: Measurement):` declaration are respected.
/// If the shape uses a `geometry:` generator, dispatches to the appropriate generator function.
/// If the shape uses `geometry:` blocks (Mode B), unrolls loops and evaluates expressions.
/// If the shape uses a CSG expression (Mode C), evaluates the CSG expression tree.
pub fn evaluate_shape_points(
    shape_def: &hwc_parser::ShapeDefinition,
    via_diameter_nm: i64,
    constants: &rustc_hash::FxHashMap<compact_str::CompactString, f64>,
) -> Path64 {
    // Check for CSG expression first (Mode C)
    if let Some(ref csg) = shape_def.csg {
        let params: Vec<(&str, i64)> = shape_def
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let val = if i == 0 {
                    via_diameter_nm
                } else {
                    via_diameter_nm
                };
                (p.name.name.as_str(), val)
            })
            .collect();

        return evaluate_csg_expression(csg, &params);
    }

    // Check for procedural generator first
    if let Some(ref generator) = shape_def.generator {
        return evaluate_generator(generator, via_diameter_nm);
    }

    // Check for Mode B geometry blocks
    if let Some(ref geometry) = shape_def.geometry {
        let params: Vec<(&str, i64)> = shape_def
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let val = if i == 0 {
                    via_diameter_nm
                } else {
                    via_diameter_nm
                };
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

    // Build parameter map: name → via_diameter_nm (first param gets the diameter)
    let params: Vec<(&str, i64)> = shape_def
        .parameters
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let val = if i == 0 {
                via_diameter_nm
            } else {
                // Additional parameters default to via_diameter_nm for now
                via_diameter_nm
            };
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

/// Evaluate a procedural generator call to a Path64 contour.
///
/// Builds a parameter map from the shape definition's parameters (first param gets the via diameter)
/// and evaluates each generator argument expression against that map.
fn evaluate_generator(
    generator: &hwc_parser::ShapeGenerator,
    via_diameter_nm: i64,
) -> Path64 {
    // For now, all parameters are mapped to via_diameter_nm.
    // A future enhancement could expose additional shape parameters.
    let params: Vec<(&str, i64)> = vec![("width", via_diameter_nm), ("w", via_diameter_nm)];

    match generator.name.as_str() {
        "StarGenerator" => {
            let points = evaluate_generator_param(generator.params.get("points"), &params, 16) as usize;
            let outer = evaluate_generator_param(generator.params.get("outer"), &params, via_diameter_nm / 2);
            let inner = evaluate_generator_param(generator.params.get("inner"), &params, via_diameter_nm / 4);
            crate::shape_generators::star_generator_contour(outer, inner, points)
        }
        "GearGenerator" => {
            let teeth = evaluate_generator_param(generator.params.get("teeth"), &params, 12) as usize;
            let outer = evaluate_generator_param(generator.params.get("outer"), &params, via_diameter_nm / 2);
            let inner = evaluate_generator_param(generator.params.get("inner"), &params, (outer as f64 * 0.7) as i64);
            crate::shape_generators::gear_generator_contour(outer, inner, teeth)
        }
        _ => {
            // Unknown generator, fall back to circle
            crate::shape_generators::circle_contour(via_diameter_nm, 16)
        }
    }
}

/// Evaluate a single generator parameter expression, returning `default` if the parameter is missing.
fn evaluate_generator_param(
    expr: Option<&String>,
    params: &[(&str, i64)],
    default: i64,
) -> i64 {
    match expr {
        Some(e) => evaluate_nm_expr(e, params),
        None => default,
    }
}

/// Evaluate an expression to a nanometer value.
///
/// Handles:
/// - Concrete values: "0nm", "-100nm", "50nm", "1um", "0.5mm"
/// - Parameter references: any user-defined name (e.g. "width", "r", "d")
/// - Simple arithmetic: "-width / 2", "width / 4", "width * 0.433", "r - 100nm"
fn evaluate_nm_expr(expr: &str, params: &[(&str, i64)]) -> i64 {
    let mut substituted = expr.trim().to_string();

    // Substitute ALL parameter names (longest-first to avoid partial matches)
    // e.g. "width_inner" before "width"
    let mut sorted_params: Vec<(&str, i64)> = params.to_vec();
    sorted_params.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (name, val) in &sorted_params {
        substituted = substituted.replace(name, &val.to_string());
    }

    evaluate_pure_math(&substituted)
}

/// Evaluate a pure math expression (no parameters).
///
/// Supports: unary -, binary +/-, *, /, measurement strings, integers, floats.
/// Also supports: sin(expr), cos(expr) trigonometric functions, and if/else expressions.
/// Operator precedence: unary sign > * / > + -
fn evaluate_pure_math(expr: &str) -> i64 {
    let trimmed = expr.trim();

    // Handle trigonometric functions: sin(expr), cos(expr)
    // Returns unit-less value (-1 to 1) so that r * sin(angle) works correctly
    if let Some(inner) = trimmed.strip_prefix("sin(") {
        if let Some(inner) = inner.strip_suffix(')') {
            let angle_deg = evaluate_pure_math(inner) as f64;
            let angle_rad = angle_deg * std::f64::consts::PI / 180.0;
            // Return raw value: sin(90deg) = 1, sin(0deg) = 0
            // When multiplied by r (in nm), result is in nm
            return angle_rad.sin() as i64;
        }
    }
    if let Some(inner) = trimmed.strip_prefix("cos(") {
        if let Some(inner) = inner.strip_suffix(')') {
            let angle_deg = evaluate_pure_math(inner) as f64;
            let angle_rad = angle_deg * std::f64::consts::PI / 180.0;
            // Return raw value: cos(0deg) = 1, cos(90deg) = 0
            // When multiplied by r (in nm), result is in nm
            return angle_rad.cos() as i64;
        }
    }

    // Handle if/else expressions: if condition: value else: value
    // Also handles parser output with spaces: "if ... else : ..."
    if let Some(rest) = trimmed.strip_prefix("if ") {
        let else_result = find_top_level_keyword(rest, "else:")
            .map(|pos| (pos, 5usize));
        let else_result = else_result.or_else(|| {
            find_top_level_keyword(rest, "else :").map(|pos| (pos, 6usize))
        });
        if let Some((else_pos, else_len)) = else_result {
            let condition_str = rest[..else_pos].trim();
            let after_else = rest[else_pos + else_len..].trim();
            // Split condition_str on first ':' to get condition and true branch
            if let Some(colon_pos) = condition_str.find(':') {
                let condition = condition_str[..colon_pos].trim();
                let true_val_str = condition_str[colon_pos + 1..].trim();
                let cond_result = evaluate_condition(condition);
                if cond_result {
                    return evaluate_pure_math(true_val_str);
                } else {
                    return evaluate_pure_math(after_else);
                }
            }
        }
    }

    // Handle unary negation
    if let Some(rest) = trimmed.strip_prefix('-') {
        return -evaluate_pure_math(rest);
    }

    // Handle unary plus
    if let Some(rest) = trimmed.strip_prefix('+') {
        return evaluate_pure_math(rest);
    }

    // Handle parenthesized expressions: (expr)
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        return evaluate_pure_math(&trimmed[1..trimmed.len() - 1]);
    }

    // Handle addition and subtraction (lowest precedence)
    // Find the rightmost top-level + or - that isn't a unary sign
    if let Some((pos, op)) = find_top_level_add_sub(trimmed) {
        let left_val = evaluate_pure_math(&trimmed[..pos]);
        let right_val = evaluate_pure_math(&trimmed[pos + 1..]);
        return if op == '+' { left_val + right_val } else { left_val - right_val };
    }

    // Handle modulo: a mod b (same precedence as * /)
    if let Some(mod_pos) = find_top_level_mod(trimmed) {
        let left_val = evaluate_pure_math(&trimmed[..mod_pos]);
        let right_val = evaluate_pure_math(&trimmed[mod_pos + 4..].trim());
        if right_val == 0 {
            return 0;
        }
        return left_val % right_val;
    }

    // Handle multiplication and division (higher precedence)
    // Find the rightmost top-level * or /
    if let Some((pos, op)) = find_top_level_mul_div(trimmed) {
        let left_val = evaluate_pure_math(&trimmed[..pos]);
        let right_str = trimmed[pos + 1..].trim();
        // For the right operand of * and /, parse as float to preserve precision
        // Try plain float first (handles "2.5"), then measurement (handles "100nm")
        let right_val: f64 = if let Ok(v) = right_str.parse::<f64>() {
            v
        } else if let Some(v) = parse_measurement_nm(right_str) {
            v as f64
        } else {
            right_str.parse::<i64>().unwrap_or(1) as f64
        };
        let left_f = left_val as f64;
        return if op == '*' {
            (left_f * right_val) as i64
        } else {
            if right_val == 0.0 { 0 } else { (left_f / right_val) as i64 }
        };
    }

    // Handle concrete measurement values
    if let Some(val) = parse_measurement_nm(trimmed) {
        return val;
    }

    // Handle plain integer
    if let Ok(val) = trimmed.parse::<i64>() {
        return val;
    }

    // Handle float literal
    if let Ok(val) = trimmed.parse::<f64>() {
        return val as i64;
    }

    0
}

/// Evaluate a condition expression (used in if/else).
///
/// Supports: = (equality), mod (modulo), and comparisons.
fn evaluate_condition(expr: &str) -> bool {
    let trimmed = expr.trim();

    // Handle modulo comparison: "a mod b = c"
    if let Some(mod_pos) = find_top_level_keyword(trimmed, "mod ") {
        let left_str = trimmed[..mod_pos].trim();
        let rest = trimmed[mod_pos + 4..].trim(); // skip "mod "
        // Find '=' in the rest
        if let Some(eq_pos) = rest.find('=') {
            let mod_arg = rest[..eq_pos].trim();
            let right_str = rest[eq_pos + 1..].trim();
            let left_val = evaluate_pure_math(left_str);
            let mod_val = evaluate_pure_math(mod_arg);
            let right_val = evaluate_pure_math(right_str);
            if mod_val == 0 {
                return false;
            }
            return (left_val % mod_val) == right_val;
        }
    }

    // Handle simple equality: "a = b"
    if let Some(eq_pos) = find_top_level_equals(trimmed) {
        let left_str = trimmed[..eq_pos].trim();
        let right_str = trimmed[eq_pos + 1..].trim();
        let left_val = evaluate_pure_math(left_str);
        let right_val = evaluate_pure_math(right_str);
        return left_val == right_val;
    }

    // Handle plain integer (truthy if non-zero)
    if let Some(val) = parse_measurement_nm(trimmed) {
        return val != 0;
    }
    if let Ok(val) = trimmed.parse::<i64>() {
        return val != 0;
    }

    false
}

/// Find the top-level position of a keyword in an expression.
fn find_top_level_keyword(s: &str, keyword: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    for i in 0..s.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && s[i..].starts_with(keyword) {
            // Make sure it's not part of a larger word
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_pos = i + keyword.len();
            let after_ok = after_pos >= s.len() || !bytes[after_pos].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return Some(i);
            }
        }
    }
    None
}

/// Find the top-level '=' (not '==' or '!=' or '<=' or '>=').
fn find_top_level_equals(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    for i in 1..s.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'=' if depth == 0 => {
                // Make sure this isn't ==, !=, <=, >=
                if i > 0 {
                    match bytes[i - 1] {
                        b'!' | b'<' | b'>' | b'=' => continue,
                        _ => {}
                    }
                }
                if i + 1 < s.len() && bytes[i + 1] == b'=' {
                    continue;
                }
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

/// Find the rightmost top-level + or - that isn't a unary sign.
/// Returns (byte_index, character) or None.
fn find_top_level_add_sub(s: &str) -> Option<(usize, char)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    // Start from index 1 since index 0 would be a unary sign
    for i in (1..s.len()).rev() {
        match bytes[i] {
            b'(' => depth -= 1,
            b')' => depth += 1,
            b'+' | b'-' if depth == 0 => {
                // Make sure this isn't right after an operator (e.g. "1 * -2")
                if i > 0 {
                    match bytes[i - 1] {
                        b'+' | b'-' | b'*' | b'/' | b'(' => continue,
                        _ => {}
                    }
                }
                return Some((i, bytes[i] as char));
            }
            _ => {}
        }
    }
    None
}

/// Find the rightmost top-level * or /.
/// Returns (byte_index, character) or None.
fn find_top_level_mul_div(s: &str) -> Option<(usize, char)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    for i in (0..s.len()).rev() {
        match bytes[i] {
            b'(' => depth -= 1,
            b')' => depth += 1,
            b'*' | b'/' if depth == 0 => {
                // Make sure there's a non-empty left side
                if i > 0 {
                    return Some((i, bytes[i] as char));
                }
            }
            _ => {}
        }
    }
    None
}

/// Find the rightmost top-level 'mod' keyword.
/// Returns byte_index of the start of 'mod' or None.
fn find_top_level_mod(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    // Search from right to left
    for i in (0..s.len()).rev() {
        match bytes[i] {
            b'(' => depth -= 1,
            b')' => depth += 1,
            _ => {}
        }
        if depth == 0 && i + 4 <= s.len() && &s[i..i + 4] == "mod " {
            // Make sure it's not part of a larger word
            if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
                continue;
            }
            return Some(i);
        }
    }
    None
}

/// Parse a measurement string to nanometers.
/// Also handles degree suffix (deg) which returns the raw value for trig functions.
fn parse_measurement_nm(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(s) = s.strip_suffix("nm") {
        s.parse::<i64>().ok()
    } else if let Some(s) = s.strip_suffix("um") {
        s.parse::<f64>().ok().map(|v| (v * 1000.0) as i64)
    } else if let Some(s) = s.strip_suffix("mm") {
        s.parse::<f64>().ok().map(|v| (v * 1_000_000.0) as i64)
    } else if let Some(s) = s.strip_suffix("cm") {
        s.parse::<f64>().ok().map(|v| (v * 10_000_000.0) as i64)
    } else if let Some(s) = s.strip_suffix("deg") {
        // Degree suffix: return raw value (used for trig function arguments)
        s.parse::<f64>().ok().map(|v| v as i64)
    } else if let Ok(val) = s.parse::<i64>() {
        Some(val)
    } else if let Ok(val) = s.parse::<f64>() {
        Some(val as i64)
    } else {
        None
    }
}

/// Evaluate geometry blocks to produce a list of points.
///
/// Unrolls for loops and evaluates variable declarations and point expressions
/// to generate the final point list for the shape contour.
pub fn evaluate_geometry_blocks(
    geometry: &[hwc_parser::GeometryBlock],
    params: &[(&str, i64)],
    constants: &rustc_hash::FxHashMap<compact_str::CompactString, f64>,
) -> Vec<Point64> {
    let mut points = Vec::new();
    let mut env = EvalEnv {
        params: params.iter().map(|(k, v)| (k.to_string(), *v as f64)).collect(),
        vars: rustc_hash::FxHashMap::default(),
    };
    // Load global constants (DEG_TO_RAD, PI, etc.) into the environment
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
                for i in *start..*end {
                    env.vars.insert(variable.clone(), i as f64);
                    evaluate_geometry_statements(body, &mut env, &mut points);
                }
            }
            hwc_parser::GeometryBlock::Variable { name, value } => {
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
            hwc_parser::GeometryStatement::Variable { name, value } => {
                let val = evaluate_expr(value, env);
                env.vars.insert(name.clone(), val);
            }
            hwc_parser::GeometryStatement::Point { x, y } => {
                let x_val = evaluate_expr(x, env);
                let y_val = evaluate_expr(y, env);
                points.push(Point64::new(x_val.round() as i64, y_val.round() as i64));
            }
        }
    }
}

/// Evaluate a CSG expression to a Path64 contour.
///
/// This implements Mode C: 2D CSG boolean operations using clipper2.
/// Supports union (+), difference (-), and intersection (*) operations,
/// as well as rotation and translation transformations.
pub fn evaluate_csg_expression(
    expr: &CsgExpression,
    params: &[(&str, i64)],
) -> Path64 {
    evaluate_csg_expression_with_vars(expr, params, &rustc_hash::FxHashMap::default())
}

/// Evaluate a CSG expression with local variables (for let bindings).
fn evaluate_csg_expression_with_vars(
    expr: &CsgExpression,
    params: &[(&str, i64)],
    local_vars: &rustc_hash::FxHashMap<String, Path64>,
) -> Path64 {
    match expr {
        CsgExpression::Primitive(CsgPrimitive::ShapeRef(name)) => {
            // Look up shape reference in local variables (from let bindings)
            if let Some(path) = local_vars.get(name) {
                return path.clone();
            }
            // Fall back to built-in shape lookup
            evaluate_csg_primitive(&CsgPrimitive::ShapeRef(name.clone()), params)
        }
        CsgExpression::Primitive(prim) => {
            evaluate_csg_primitive(prim, params)
        }
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
        CsgExpression::Transformed { expr, rotation, translation } => {
            let mut path = evaluate_csg_expression_with_vars(expr, params, local_vars);
            if let Some(deg) = rotation {
                path = rotate_path(&path, *deg);
            }
            if let Some((dx, dy)) = translation {
                // Convert from double to i64 (values are in nm)
                path = translate_path(&path, *dx as i64, *dy as i64);
            }
            path
        }
        CsgExpression::LetBinding { name, value, body } => {
            // Evaluate the value expression
            let value_path = evaluate_csg_expression_with_vars(value, params, local_vars);
            // Add to local variables
            let mut new_vars = local_vars.clone();
            new_vars.insert(name.clone(), value_path);
            // Evaluate the body with the new variable
            evaluate_csg_expression_with_vars(body, params, &new_vars)
        }
    }
}

/// Evaluate a CSG primitive shape to a Path64 contour.
fn evaluate_csg_primitive(
    prim: &CsgPrimitive,
    params: &[(&str, i64)],
) -> Path64 {
    match prim {
        CsgPrimitive::Rectangle { width, height } => {
            let w = evaluate_nm_expr(width, params);
            let h = evaluate_nm_expr(height, params);
            let half_w = w / 2;
            let half_h = h / 2;
            let mut contour = Path64::new();
            contour.push(Point64::new(-half_w, -half_h));
            contour.push(Point64::new(half_w, -half_h));
            contour.push(Point64::new(half_w, half_h));
            contour.push(Point64::new(-half_w, half_h));
            contour
        }
        CsgPrimitive::Circle { diameter } => {
            let d = evaluate_nm_expr(diameter, params);
            crate::shape_generators::circle_contour(d, 16)
        }
        CsgPrimitive::ShapeRef(name) => {
            // Look up the shape reference in the symbol table or built-in shapes
            // For now, fall back to a circle with default size
            eprintln!("Warning: ShapeRef '{}' not yet implemented in CSG, using default circle", name);
            crate::shape_generators::circle_contour(1000, 16) // Default 1um circle
        }
    }
}

/// Perform a union operation on two paths using clipper2.
fn clipper_union(left: &Path64, right: &Path64) -> Path64 {
    let subjects = vec![left.clone()];
    let clips = vec![right.clone()];
    let result = boolean_op_64(ClipType::Union, FillRule::NonZero, &subjects, &clips);
    result.first().cloned().unwrap_or_else(|| left.clone())
}

/// Perform a difference operation on two paths using clipper2.
fn clipper_difference(left: &Path64, right: &Path64) -> Path64 {
    let subjects = vec![left.clone()];
    let clips = vec![right.clone()];
    let result = boolean_op_64(ClipType::Difference, FillRule::NonZero, &subjects, &clips);
    result.first().cloned().unwrap_or_else(|| left.clone())
}

/// Perform an intersection operation on two paths using clipper2.
fn clipper_intersection(left: &Path64, right: &Path64) -> Path64 {
    let subjects = vec![left.clone()];
    let clips = vec![right.clone()];
    let result = boolean_op_64(ClipType::Intersection, FillRule::NonZero, &subjects, &clips);
    result.first().cloned().unwrap_or_else(|| left.clone())
}

/// Rotate a path around the origin [0, 0] by the given angle in degrees.
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

/// Translate a path by the given offset.
fn translate_path(path: &Path64, dx: i64, dy: i64) -> Path64 {
    let mut result = Path64::new();
    for pt in path.iter() {
        result.push(Point64::new(pt.x + dx, pt.y + dy));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(width_nm: i64) -> Vec<(&'static str, i64)> {
        vec![("width", width_nm)]
    }

    fn params_custom<'a>(name: &'a str, val: i64) -> Vec<(&'a str, i64)> {
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
        // -width / 2
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
        // (width / 2) + (width / 4) = 250000 + 125000 = 375000
        assert_eq!(evaluate_nm_expr("width / 2 + width / 4", &p), 375_000);
        // width - width / 4 = 500000 - 125000 = 375000
        assert_eq!(evaluate_nm_expr("width - width / 4", &p), 375_000);
    }

    #[test]
    fn test_measurement_in_expressions() {
        let p = params(500_000);
        // width / 2 + 100nm = 250000 + 100 = 250100
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
        // deg suffix returns the raw numeric value
        assert_eq!(evaluate_pure_math("90deg"), 90);
        assert_eq!(evaluate_pure_math("180deg"), 180);
        assert_eq!(evaluate_pure_math("11.25deg"), 11);
    }

    #[test]
    fn test_measurement_deg() {
        // 11.25deg should parse as 11 (truncated)
        assert_eq!(evaluate_pure_math("11.25deg"), 11);
    }

    #[test]
    fn test_if_else_expression() {
        // if 1 = 1: 100 else: 200 should evaluate to 100
        assert_eq!(evaluate_pure_math("if 1 = 1: 100 else: 200"), 100);
        // if 1 = 2: 100 else: 200 should evaluate to 200
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
        // if 4 mod 2 = 0: 100 else: 200 should evaluate to 100 (even)
        assert_eq!(evaluate_pure_math("if 4 mod 2 = 0: 100 else: 200"), 100);
        // if 5 mod 2 = 0: 100 else: 200 should evaluate to 200 (odd)
        assert_eq!(evaluate_pure_math("if 5 mod 2 = 0: 100 else: 200"), 200);
    }

    #[test]
    fn test_evaluate_expr_literal() {
        use hwc_parser::expr::Expr;
        let env = EvalEnv {
            params: vec![],
            vars: rustc_hash::FxHashMap::default(),
        };
        assert_eq!(evaluate_expr(&Expr::Literal(42.0), &env), 42.0);
    }

    #[test]
    fn test_evaluate_expr_identifier() {
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
