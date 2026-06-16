pub(super) fn evaluate_nm_expr(expr: &str, params: &[(&str, i64)]) -> i64 {
    let mut substituted = expr.trim().to_string();

    let mut sorted_params: Vec<(&str, i64)> = params.to_vec();
    sorted_params.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (name, val) in &sorted_params {
        substituted = substituted.replace(name, &val.to_string());
    }

    evaluate_pure_math(&substituted)
}

pub(super) fn evaluate_pure_math(expr: &str) -> i64 {
    let trimmed = expr.trim();

    if let Some(inner) = trimmed.strip_prefix("sin(") {
        if let Some(inner) = inner.strip_suffix(')') {
            let angle_deg = evaluate_pure_math(inner) as f64;
            let angle_rad = angle_deg * std::f64::consts::PI / 180.0;
            return angle_rad.sin() as i64;
        }
    }
    if let Some(inner) = trimmed.strip_prefix("cos(") {
        if let Some(inner) = inner.strip_suffix(')') {
            let angle_deg = evaluate_pure_math(inner) as f64;
            let angle_rad = angle_deg * std::f64::consts::PI / 180.0;
            return angle_rad.cos() as i64;
        }
    }

    if let Some(rest) = trimmed.strip_prefix("if ") {
        let else_result = find_top_level_keyword(rest, "else:").map(|pos| (pos, 5usize));
        let else_result =
            else_result.or_else(|| find_top_level_keyword(rest, "else :").map(|pos| (pos, 6usize)));
        if let Some((else_pos, else_len)) = else_result {
            let condition_str = rest[..else_pos].trim();
            let after_else = rest[else_pos + else_len..].trim();
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

    if let Some(rest) = trimmed.strip_prefix('-') {
        return -evaluate_pure_math(rest);
    }

    if let Some(rest) = trimmed.strip_prefix('+') {
        return evaluate_pure_math(rest);
    }

    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        return evaluate_pure_math(&trimmed[1..trimmed.len() - 1]);
    }

    if let Some((pos, op)) = find_top_level_add_sub(trimmed) {
        let left_val = evaluate_pure_math(&trimmed[..pos]);
        let right_val = evaluate_pure_math(&trimmed[pos + 1..]);
        return if op == '+' {
            left_val + right_val
        } else {
            left_val - right_val
        };
    }

    if let Some(mod_pos) = find_top_level_mod(trimmed) {
        let left_val = evaluate_pure_math(&trimmed[..mod_pos]);
        let right_val = evaluate_pure_math(trimmed[mod_pos + 4..].trim());
        if right_val == 0 {
            return 0;
        }
        return left_val % right_val;
    }

    if let Some((pos, op)) = find_top_level_mul_div(trimmed) {
        let left_val = evaluate_pure_math(&trimmed[..pos]);
        let right_str = trimmed[pos + 1..].trim();
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
        } else if right_val == 0.0 {
            0
        } else {
            (left_f / right_val) as i64
        };
    }

    if let Some(val) = parse_measurement_nm(trimmed) {
        return val;
    }

    if let Ok(val) = trimmed.parse::<i64>() {
        return val;
    }

    if let Ok(val) = trimmed.parse::<f64>() {
        return val as i64;
    }

    0
}

fn evaluate_condition(expr: &str) -> bool {
    let trimmed = expr.trim();

    if let Some(mod_pos) = find_top_level_keyword(trimmed, "mod ") {
        let left_str = trimmed[..mod_pos].trim();
        let rest = trimmed[mod_pos + 4..].trim();
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

    if let Some(eq_pos) = find_top_level_equals(trimmed) {
        let left_str = trimmed[..eq_pos].trim();
        let right_str = trimmed[eq_pos + 1..].trim();
        let left_val = evaluate_pure_math(left_str);
        let right_val = evaluate_pure_math(right_str);
        return left_val == right_val;
    }

    if let Some(val) = parse_measurement_nm(trimmed) {
        return val != 0;
    }
    if let Ok(val) = trimmed.parse::<i64>() {
        return val != 0;
    }

    false
}

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

fn find_top_level_equals(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    for i in 1..s.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'=' if depth == 0 => {
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

pub(super) fn find_top_level_add_sub(s: &str) -> Option<(usize, char)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    for i in (1..s.len()).rev() {
        match bytes[i] {
            b'(' => depth -= 1,
            b')' => depth += 1,
            b'+' | b'-' if depth == 0 => {
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

pub(super) fn find_top_level_mul_div(s: &str) -> Option<(usize, char)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    for i in (0..s.len()).rev() {
        match bytes[i] {
            b'(' => depth -= 1,
            b')' => depth += 1,
            b'*' | b'/' if depth == 0 => {
                if i > 0 {
                    return Some((i, bytes[i] as char));
                }
            }
            _ => {}
        }
    }
    None
}

fn find_top_level_mod(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;

    for i in (0..s.len()).rev() {
        match bytes[i] {
            b'(' => depth -= 1,
            b')' => depth += 1,
            _ => {}
        }
        if depth == 0 && i + 4 <= s.len() && &s[i..i + 4] == "mod " {
            if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
                continue;
            }
            return Some(i);
        }
    }
    None
}

pub(super) fn parse_measurement_nm(s: &str) -> Option<i64> {
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
        s.parse::<f64>().ok().map(|v| v as i64)
    } else if let Ok(val) = s.parse::<i64>() {
        Some(val)
    } else if let Ok(val) = s.parse::<f64>() {
        Some(val as i64)
    } else {
        None
    }
}
