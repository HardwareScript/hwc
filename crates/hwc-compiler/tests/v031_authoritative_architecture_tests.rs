use hwc_compiler::eval::*;
use hwc_parser::{DiagnosticCollector, Lexer, Parser};
use hwc_types::UnitRegistry;
use std::sync::Arc;

fn evaluate_v031_script(source: &str) -> Result<Value, EvalError> {
    let lexer = Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => return Err(EvalError::General { message: format!("Lexing failed: {:?}", e) }),
    };
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new(source, 100);
    let prog = parser.parse(&collector);
    if collector.error_count() > 0 {
        return Err(EvalError::General {
            message: format!("Parser errors occurred: {:?}", collector.summary()),
        });
    }

    let mut ctx = EvaluationContext::new();
    ctx.unit_registry = Some(Arc::new(UnitRegistry::standard()));
    run_script(&prog, &mut ctx, None).map(|v| v.unwrap_or(Value::Void))
}

#[test]
fn test_pillar2_impl_blocks_and_methods() {
    let src = r#"
        struct Vector {
            x: int,
            y: int,
        }

        impl Vector {
            fn add(self, other: Vector) -> Vector {
                Vector {
                    x: self.x + other.x,
                    y: self.y + other.y,
                }
            }

            fn dot(self, other: Vector) -> int {
                self.x * other.x + self.y * other.y
            }
        }

        let v1 = Vector { x: 3, y: 4 }
        let v2 = Vector { x: 1, y: 2 }
        let v3 = v1.add(v2)
        assert(v3.x == 4)
        assert(v3.y == 6)

        let d = v1.dot(v2)
        assert(d == 11)
    "#;
    let res = evaluate_v031_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_pillar2_static_method_calls() {
    let src = r#"
        struct Point {
            x: int,
            y: int,
        }

        impl Point {
            fn origin() -> Point {
                Point { x: 0, y: 0 }
            }
        }

        let p = Point::origin()
        assert(p.x == 0)
        assert(p.y == 0)
    "#;
    let res = evaluate_v031_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_pillar4_si_dimensional_algebra() {
    let src = r#"
        # 1. Base length dimensions
        let w = 10um
        let l = 20um
        
        # 2. Area = L * L -> L^2
        let a = w * l
        
        # 3. Area / Length -> Length
        let l_recovered = a / w
        assert(l_recovered == 20um)

        # 4. Compound units
        let sheet_r = 100Ohm_sq
        let cap_dens = 2fF_um2
    "#;
    let res = evaluate_v031_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}
