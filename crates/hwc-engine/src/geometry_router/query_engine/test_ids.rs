use super::query_ids::{compute_query_id, make_query_id, QueryId, QueryType};
use super::results::*;
use super::store::QueryStore;

#[test]
fn test_no_cycles_dependency_graph_acyclic() {
    let mut store = QueryStore::new();
    let file_id = 10;

    let ast_qid = make_query_id(QueryType::ParseAst, file_id, &[]);
    let sym_qid = make_query_id(QueryType::ResolveSymbols, file_id, &[]);
    let part_qid = make_query_id(QueryType::PartitionGcells, file_id, &[]);

    let _ = store.parse_ast(file_id);
    let _ = store.resolve_symbols(file_id);
    let _ = store.partition_gcells(file_id);

    store.record_dependency(sym_qid, ast_qid);
    store.record_dependency(part_qid, sym_qid);

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    queue.push_back(part_qid);
    visited.insert(part_qid);

    while let Some(current) = queue.pop_front() {
        if let Some(deps) = store.dependencies.get(&current) {
            for &dep in deps {
                assert!(
                    !visited.contains(&dep),
                    "Cycle detected: {:?} already visited from {:?}",
                    dep,
                    current
                );
                visited.insert(dep);
                queue.push_back(dep);
            }
        }
    }

    assert!(visited.contains(&ast_qid));
    assert!(visited.contains(&sym_qid));
    assert!(visited.contains(&part_qid));
}

#[test]
fn test_compute_query_id_deterministic() {
    let qid1 = compute_query_id(
        QueryId {
            type_hash: 1,
            input_hash: 0,
        },
        42,
        &[100, 200],
    );
    let qid2 = compute_query_id(
        QueryId {
            type_hash: 1,
            input_hash: 0,
        },
        42,
        &[100, 200],
    );
    assert_eq!(qid1, qid2, "Same inputs must produce same QueryId");

    let qid3 = compute_query_id(
        QueryId {
            type_hash: 1,
            input_hash: 0,
        },
        42,
        &[100, 201],
    );
    assert_ne!(
        qid1, qid3,
        "Different params must produce different QueryId"
    );
}

#[test]
fn test_make_query_id_different_types() {
    let ast = make_query_id(QueryType::ParseAst, 1, &[]);
    let sym = make_query_id(QueryType::ResolveSymbols, 1, &[]);
    let part = make_query_id(QueryType::PartitionGcells, 1, &[]);

    assert_ne!(ast.type_hash, sym.type_hash);
    assert_ne!(sym.type_hash, part.type_hash);
    assert_ne!(ast.type_hash, part.type_hash);
}

#[test]
fn test_query_result_downcast() {
    let ast = AstResult {
        file_id: 1,
        node_count: 42,
        hash: [0xAA; 32],
    };
    let qr = QueryResult::ParseAst(ast);
    assert!(qr.as_ast().is_some());
    assert!(qr.as_symbols().is_none());
    assert!(qr.as_route().is_none());

    let route = RouteResult {
        file_id: 1,
        gcell_id: 0,
        segment_count: 5,
        hash: [0xBB; 32],
    };
    let qr2 = QueryResult::RouteGcell(route);
    assert!(qr2.as_route().is_some());
    assert!(qr2.as_verify().is_none());
}
