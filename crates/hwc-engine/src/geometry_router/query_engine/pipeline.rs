use super::query_ids::{make_query_id, QueryId, QueryType};
use super::results::*;
use super::store::QueryStore;

#[inline]
fn compute_content_hash(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

impl QueryStore {
    #[inline]
    pub fn parse_ast(&mut self, file_id: u64) -> &AstResult {
        let qid = make_query_id(QueryType::ParseAst, file_id, &[]);
        let file_id_copy = file_id;
        self.execute_query(qid, || {
            let node_count = 0;
            let hash = compute_content_hash(&file_id_copy.to_le_bytes());
            QueryResult::ParseAst(AstResult {
                file_id: file_id_copy,
                node_count,
                hash,
            })
        })
        .as_ast()
        .expect("parse_ast must store ParseAst result")
    }

    #[inline]
    pub fn resolve_symbols(&mut self, file_id: u64) -> &SymbolResult {
        let qid = make_query_id(QueryType::ResolveSymbols, file_id, &[]);
        let file_id_copy = file_id;
        self.execute_query(qid, || {
            let symbol_count = 0;
            let hash = compute_content_hash(&file_id_copy.to_le_bytes());
            QueryResult::ResolveSymbols(SymbolResult {
                file_id: file_id_copy,
                symbol_count,
                hash,
            })
        })
        .as_symbols()
        .expect("resolve_symbols must store SymbolResult result")
    }

    #[inline]
    pub fn partition_gcells(&mut self, file_id: u64) -> &PartitionResult {
        let qid = make_query_id(QueryType::PartitionGcells, file_id, &[]);
        let file_id_copy = file_id;
        self.execute_query(qid, || {
            let gcell_count = 0;
            let hash = compute_content_hash(&file_id_copy.to_le_bytes());
            QueryResult::PartitionGcells(PartitionResult {
                file_id: file_id_copy,
                gcell_count,
                hash,
            })
        })
        .as_partition()
        .expect("partition_gcells must store PartitionResult result")
    }

    #[inline]
    pub fn route_gcell(&mut self, file_id: u64, gcell_id: u32) -> &RouteResult {
        let qid = make_query_id(QueryType::RouteGcell, file_id, &[gcell_id as u64]);
        let file_id_copy = file_id;
        self.execute_query(qid, || {
            let mut buf = [0u8; 12];
            buf[..8].copy_from_slice(&file_id_copy.to_le_bytes());
            buf[8..12].copy_from_slice(&gcell_id.to_le_bytes());
            let segment_count = 0;
            let hash = compute_content_hash(&buf);
            QueryResult::RouteGcell(RouteResult {
                file_id: file_id_copy,
                gcell_id,
                segment_count,
                hash,
            })
        })
        .as_route()
        .expect("route_gcell must store RouteResult result")
    }

    #[inline]
    pub fn verify_gcell(&mut self, file_id: u64, gcell_id: u32) -> &VerifyResult {
        let qid = make_query_id(QueryType::VerifyGcell, file_id, &[gcell_id as u64]);
        let file_id_copy = file_id;
        self.execute_query(qid, || {
            let mut buf = [0u8; 12];
            buf[..8].copy_from_slice(&file_id_copy.to_le_bytes());
            buf[8..12].copy_from_slice(&gcell_id.to_le_bytes());
            let violation_count = 0;
            let hash = compute_content_hash(&buf);
            QueryResult::VerifyGcell(VerifyResult {
                file_id: file_id_copy,
                gcell_id,
                violation_count,
                hash,
            })
        })
        .as_verify()
        .expect("verify_gcell must store VerifyResult result")
    }

    pub fn invalidate_file(&mut self, file_id: u64) {
        self.current_time += 1;
        let now = self.current_time;

        let qids_to_invalidate: Vec<QueryId> = self.results.keys().copied().collect();

        for qid in qids_to_invalidate {
            self.invalidation_times.insert(qid, now);
            self.results.remove(&qid);
            self.timestamps.remove(&qid);
        }

        let all_keys: Vec<QueryId> = self.dependencies.keys().copied().collect();
        for qid in all_keys {
            self.mark_stale_recursive(qid, now);
        }

        let _ = file_id;
    }

    fn mark_stale_recursive(&mut self, query_id: QueryId, now: u64) {
        if let Some(deps) = self.dependencies.get(&query_id).cloned() {
            let mut should_invalidate = false;
            for dep in &deps {
                if let Some(&inv_time) = self.invalidation_times.get(dep) {
                    let query_time = self.timestamps.get(&query_id).copied().unwrap_or(0);
                    if inv_time > query_time {
                        should_invalidate = true;
                        break;
                    }
                }
            }
            if should_invalidate {
                self.invalidation_times.insert(query_id, now);
                self.results.remove(&query_id);
                self.timestamps.remove(&query_id);
            }
        }
    }

    pub fn invalidate_gcell(&mut self, file_id: u64, gcell_id: u32) {
        self.current_time += 1;
        let now = self.current_time;

        let route_qid = make_query_id(QueryType::RouteGcell, file_id, &[gcell_id as u64]);
        let verify_qid = make_query_id(QueryType::VerifyGcell, file_id, &[gcell_id as u64]);

        self.results.remove(&route_qid);
        self.timestamps.remove(&route_qid);
        self.invalidation_times.insert(route_qid, now);

        self.results.remove(&verify_qid);
        self.timestamps.remove(&verify_qid);
        self.invalidation_times.insert(verify_qid, now);

        self.mark_stale(route_qid);
        self.mark_stale(verify_qid);
    }

    pub fn invalidate_boundary_port(&mut self, file_id: u64, adjacent_cell_ids: (u32, u32)) {
        self.current_time += 1;
        let now = self.current_time;

        let (cell_a, cell_b) = adjacent_cell_ids;

        for &gcell_id in &[cell_a, cell_b] {
            let route_qid = make_query_id(QueryType::RouteGcell, file_id, &[gcell_id as u64]);
            let verify_qid = make_query_id(QueryType::VerifyGcell, file_id, &[gcell_id as u64]);

            self.results.remove(&route_qid);
            self.timestamps.remove(&route_qid);
            self.invalidation_times.insert(route_qid, now);

            self.results.remove(&verify_qid);
            self.timestamps.remove(&verify_qid);
            self.invalidation_times.insert(verify_qid, now);

            self.mark_stale(route_qid);
            self.mark_stale(verify_qid);
        }
    }
}
