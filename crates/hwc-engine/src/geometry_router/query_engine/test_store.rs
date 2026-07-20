use super::query_ids::{make_query_id, QueryType};
use super::results::*;
use super::store::QueryStore;

#[test]
fn test_memoization_same_query_returns_same_result() {
    let mut store = QueryStore::new();
    let file_id = 42;

    let result1 = store.parse_ast(file_id);
    let hash1 = result1.hash;
    let file1 = result1.file_id;

    let result2 = store.parse_ast(file_id);
    let hash2 = result2.hash;

    assert_eq!(file1, 42);
    assert_eq!(hash1, hash2, "Same query must return same hash");
    assert_eq!(store.len(), 1, "Only one result should be memoized");
}

#[test]
fn test_staleness_invalidating_input_marks_dependents_stale() {
    let mut store = QueryStore::new();
    let file_id = 1;

    let _ = store.parse_ast(file_id);
    assert!(!store.is_stale(make_query_id(QueryType::ParseAst, file_id, &[])));

    let input_qid = make_query_id(QueryType::ParseAst, file_id, &[]);
    store.register_input(input_qid);
    let sym_qid = make_query_id(QueryType::ResolveSymbols, file_id, &[]);
    store.record_dependency(sym_qid, input_qid);

    let _ = store.resolve_symbols(file_id);
    assert!(!store.is_stale(sym_qid));

    store.invalidate_input(input_qid);

    assert!(store.is_stale(sym_qid));
}

#[test]
fn test_re_evaluation_stale_query_is_recomputed() {
    let mut store = QueryStore::new();
    let file_id = 7;

    let result_a = store.parse_ast(file_id);
    let hash_a = result_a.hash;

    let input_qid = make_query_id(QueryType::ParseAst, file_id, &[]);
    store.register_input(input_qid);
    store.invalidate_input(input_qid);

    let result_b = store.parse_ast(file_id);
    let hash_b = result_b.hash;

    assert_eq!(hash_a, hash_b);
    assert!(!store.is_stale(make_query_id(QueryType::ParseAst, file_id, &[])));
}

#[test]
fn test_boundary_port_invalidation_only_2_adjacent_cells() {
    let mut store = QueryStore::new();
    let file_id = 5;

    for gcell_id in 0..4u32 {
        let _ = store.route_gcell(file_id, gcell_id);
        let _ = store.verify_gcell(file_id, gcell_id);
    }
    assert_eq!(store.len(), 8);

    store.invalidate_boundary_port(file_id, (1, 2));

    let route0_qid = make_query_id(QueryType::RouteGcell, file_id, &[0]);
    let route3_qid = make_query_id(QueryType::RouteGcell, file_id, &[3]);
    let verify0_qid = make_query_id(QueryType::VerifyGcell, file_id, &[0]);
    let verify3_qid = make_query_id(QueryType::VerifyGcell, file_id, &[3]);

    assert!(
        store.get_result(route0_qid).is_some(),
        "Cell 0 route should still be cached"
    );
    assert!(
        store.get_result(route3_qid).is_some(),
        "Cell 3 route should still be cached"
    );
    assert!(
        store.get_result(verify0_qid).is_some(),
        "Cell 0 verify should still be cached"
    );
    assert!(
        store.get_result(verify3_qid).is_some(),
        "Cell 3 verify should still be cached"
    );

    let route1_qid = make_query_id(QueryType::RouteGcell, file_id, &[1]);
    let route2_qid = make_query_id(QueryType::RouteGcell, file_id, &[2]);
    let verify1_qid = make_query_id(QueryType::VerifyGcell, file_id, &[1]);
    let verify2_qid = make_query_id(QueryType::VerifyGcell, file_id, &[2]);

    assert!(
        store.get_result(route1_qid).is_none(),
        "Cell 1 route should be invalidated"
    );
    assert!(
        store.get_result(route2_qid).is_none(),
        "Cell 2 route should be invalidated"
    );
    assert!(
        store.get_result(verify1_qid).is_none(),
        "Cell 1 verify should be invalidated"
    );
    assert!(
        store.get_result(verify2_qid).is_none(),
        "Cell 2 verify should be invalidated"
    );
}

#[test]
fn test_file_invalidation_invalidates_all_queries_for_file() {
    let mut store = QueryStore::new();
    let file_id = 3;

    let _ = store.parse_ast(file_id);
    let _ = store.resolve_symbols(file_id);
    let _ = store.partition_gcells(file_id);
    let _ = store.route_gcell(file_id, 0);
    let _ = store.verify_gcell(file_id, 0);

    assert_eq!(store.len(), 5);

    store.invalidate_file(file_id);

    let ast_qid = make_query_id(QueryType::ParseAst, file_id, &[]);
    let sym_qid = make_query_id(QueryType::ResolveSymbols, file_id, &[]);
    let part_qid = make_query_id(QueryType::PartitionGcells, file_id, &[]);
    let route_qid = make_query_id(QueryType::RouteGcell, file_id, &[0]);
    let verify_qid = make_query_id(QueryType::VerifyGcell, file_id, &[0]);

    assert!(store.get_result(ast_qid).is_none());
    assert!(store.get_result(sym_qid).is_none());
    assert!(store.get_result(part_qid).is_none());
    assert!(store.get_result(route_qid).is_none());
    assert!(store.get_result(verify_qid).is_none());
}

#[test]
fn test_incremental_performance_recompute_faster_than_full() {
    let mut store = QueryStore::new();
    let file_id = 99;

    let full_start = std::time::Instant::now();
    for gcell_id in 0..100u32 {
        let _ = store.parse_ast(file_id);
        let _ = store.resolve_symbols(file_id);
        let _ = store.partition_gcells(file_id);
        let _ = store.route_gcell(file_id, gcell_id);
        let _ = store.verify_gcell(file_id, gcell_id);
    }
    let full_elapsed = full_start.elapsed();

    store.invalidate_gcell(file_id, 42);

    let incr_start = std::time::Instant::now();
    let _ = store.route_gcell(file_id, 42);
    let _ = store.verify_gcell(file_id, 42);
    let incr_elapsed = incr_start.elapsed();

    assert!(
        incr_elapsed < full_elapsed,
        "Incremental recompute ({:?}) should be faster than full ({:?})",
        incr_elapsed,
        full_elapsed
    );

    let route0_qid = make_query_id(QueryType::RouteGcell, file_id, &[0]);
    assert!(
        store.get_result(route0_qid).is_some(),
        "G-cell 0 route should still be cached"
    );
}

#[test]
fn test_execute_query_caches_result() {
    let mut store = QueryStore::new();
    let qid = make_query_id(QueryType::ParseAst, 1, &[]);

    let call_count = std::cell::Cell::new(0u32);
    store.execute_query(qid, || {
        call_count.set(call_count.get() + 1);
        QueryResult::ParseAst(AstResult {
            file_id: 1,
            node_count: 10,
            hash: [0u8; 32],
        })
    });

    store.execute_query(qid, || {
        call_count.set(call_count.get() + 1);
        QueryResult::ParseAst(AstResult {
            file_id: 1,
            node_count: 10,
            hash: [0u8; 32],
        })
    });

    assert_eq!(
        call_count.get(),
        1,
        "Compute function should only be called once"
    );
}

#[test]
fn test_register_and_check_staleness_chain() {
    let mut store = QueryStore::new();
    let file_id = 5;

    let input_qid = make_query_id(QueryType::ParseAst, file_id, &[]);
    let a_qid = make_query_id(QueryType::ResolveSymbols, file_id, &[]);
    let b_qid = make_query_id(QueryType::PartitionGcells, file_id, &[]);

    store.register_input(input_qid);
    store.record_dependency(a_qid, input_qid);
    store.record_dependency(b_qid, a_qid);

    let _ = store.resolve_symbols(file_id);
    let _ = store.partition_gcells(file_id);

    assert!(!store.is_stale(a_qid));
    assert!(!store.is_stale(b_qid));

    store.invalidate_input(input_qid);

    assert!(store.is_stale(a_qid));
    assert!(store.is_stale(b_qid));
}

#[test]
fn test_clear_resets_store() {
    let mut store = QueryStore::new();
    let _ = store.parse_ast(1);
    let _ = store.route_gcell(1, 0);
    assert_eq!(store.len(), 2);

    store.clear();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn test_gcell_invalidation_preserves_partition() {
    let mut store = QueryStore::new();
    let file_id = 8;

    let _ = store.partition_gcells(file_id);
    let _ = store.route_gcell(file_id, 0);
    let _ = store.route_gcell(file_id, 1);

    store.invalidate_gcell(file_id, 0);

    let part_qid = make_query_id(QueryType::PartitionGcells, file_id, &[]);
    assert!(
        store.get_result(part_qid).is_some(),
        "Partition should still be cached after single G-cell invalidation"
    );

    let route1_qid = make_query_id(QueryType::RouteGcell, file_id, &[1]);
    assert!(
        store.get_result(route1_qid).is_some(),
        "G-cell 1 route should still be cached"
    );

    let route0_qid = make_query_id(QueryType::RouteGcell, file_id, &[0]);
    assert!(
        store.get_result(route0_qid).is_none(),
        "G-cell 0 route should be invalidated"
    );
}
