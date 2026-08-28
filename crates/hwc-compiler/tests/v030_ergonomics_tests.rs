use hwc_compiler::eval::*;
use hwc_parser::{DiagnosticCollector, Lexer, Parser};
use rustc_hash::FxHashMap;
use std::sync::Arc;

fn evaluate_script(source: &str) -> Result<Value, EvalError> {
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexing failed");
    let mut parser = Parser::new(tokens);
    let collector = DiagnosticCollector::new();
    let prog = parser.parse(&collector);
    assert_eq!(collector.error_count(), 0, "Parser errors occurred: {:?}", collector);

    let mut stmts = Vec::new();
    let mut funcs = FxHashMap::default();
    let mut structs = FxHashMap::default();

    for item in prog.items {
        match item {
            hwc_parser::TopLevelItem::Statement(s) => stmts.push(s),
            hwc_parser::TopLevelItem::Function(f) => {
                funcs.insert(f.name.clone(), f);
            }
            hwc_parser::TopLevelItem::Struct(s) => {
                structs.insert(s.name.clone(), s);
            }
            _ => {}
        }
    }

    let chunk = BytecodeCompiler::compile_statements(
        "test_chunk",
        &stmts,
        prog.span,
        None,
        &funcs,
        &structs,
        &FxHashMap::default(),
    )?;

    let emitter = Arc::new(MemoryEmitter::new());
    let mut vm = VM::new(emitter);
    let mut chunk_map = FxHashMap::default();
    chunk_map.insert("test_chunk".into(), chunk);
    vm.eval_chunk("test_chunk", &chunk_map)
}

#[test]
fn test_inplace_array_mutations() {
    let src = r#"
        let mut arr = [1, 2, 3]
        arr.push(4)
        arr.push(5)
        let popped = arr.pop()
        let length = arr.len()
        assert(popped == 5)
        assert(length == 4)
        assert(arr[0] == 1)
        assert(arr[3] == 4)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_compound_assignments() {
    let src = r#"
        let mut a = 10
        a += 5
        assert(a == 15)
        a -= 3
        assert(a == 12)
        a *= 2
        assert(a == 24)
        a /= 4
        assert(a == 6)
        a %= 4
        assert(a == 2)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_loop_break_and_continue() {
    let src = r#"
        let mut sum = 0
        for i in 0..10 {
            if i == 3 {
                continue
            }
            if i == 7 {
                break
            }
            sum += i
        }
        # sum should be 0 + 1 + 2 + 4 + 5 + 6 = 18
        assert(sum == 18)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_expression_oriented_if_and_match() {
    let src = r#"
        let flag = true
        let x = if flag { 42 } else { 0 }
        assert(x == 42)

        let tag = 2
        let y = match tag {
            1 => 100,
            2 => 200,
            _ => 300,
        }
        assert(y == 200)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_tuple_packing_and_destructuring() {
    let src = r#"
        let pair = (10, 20)
        let (first, second) = pair
        assert(first == 10)
        assert(second == 20)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_array_slicing() {
    let src = r#"
        let arr = [10, 20, 30, 40, 50]
        let sub = arr[1..4]
        assert(sub.len() == 3)
        assert(sub[0] == 20)
        assert(sub[1] == 30)
        assert(sub[2] == 40)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_block_tail_expressions() {
    let src = r#"
        let result = {
            let a = 15
            let b = 25
            a + b
        }
        assert(result == 40)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_unit_scalar_extractors() {
    let src = r#"
        let m = 1.5um
        let f = m.to_float()
        let p = m.to_pm()
        let u = m.to_um()
        assert(p == 1500000)
        assert(f > 0.0000014 and f < 0.0000016)
        assert(u > 1.49 and u < 1.51)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_bounding_box_queries() {
    let src = r#"
        let bbox = bbox_from_rect(
            [10um, 10um],
            [4um, 6um]
        )
        let w = bbox.width()
        let h = bbox.height()
        assert(w == 4um)
        assert(h == 6um)

        let bbox2 = bbox_from_rect(
            [12um, 12um],
            [4um, 4um]
        )
        let is_intersect = bbox_intersects(bbox, bbox2)
        assert(is_intersect == true)

        let u_box = bbox_union(bbox, bbox2)
        assert(u_box.width() == 6um)
        assert(u_box.height() == 7um)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_bitwise_operators() {
    let src = r#"
        let mask = 1 << 4
        assert(mask == 16)
        let shifted = 32 >> 2
        assert(shifted == 8)
        let bit_and = 0b1100 & 0b1010
        assert(bit_and == 0b1000)
        let bit_or = 0b1100 | 0b0011
        assert(bit_or == 0b1111)
        let bit_xor = 0b1100 ^ 0b1010
        assert(bit_xor == 0b0110)
        let bit_not = ~0
        assert(bit_not == -1)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}

#[test]
fn test_wildcard_destructuring() {
    let src = r#"
        let tuple = (100, 200, 300)
        let (first, _, third) = tuple
        assert(first == 100)
        assert(third == 300)
    "#;
    let res = evaluate_script(src);
    assert!(res.is_ok(), "Error: {:?}", res.err());
}
