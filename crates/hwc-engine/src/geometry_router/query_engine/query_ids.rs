use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QueryId {
    pub type_hash: u64,
    pub input_hash: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QueryType {
    ParseAst,
    ResolveSymbols,
    PartitionGcells,
    RouteGcell,
    VerifyGcell,
}

impl QueryType {
    #[inline]
    fn tag(&self) -> u64 {
        match self {
            QueryType::ParseAst => 1,
            QueryType::ResolveSymbols => 2,
            QueryType::PartitionGcells => 3,
            QueryType::RouteGcell => 4,
            QueryType::VerifyGcell => 5,
        }
    }
}

#[inline]
fn make_hasher() -> std::collections::hash_map::DefaultHasher {
    std::collections::hash_map::DefaultHasher::new()
}

#[inline]
pub fn compute_query_id(query_type: QueryId, file_id: u64, params: &[u64]) -> QueryId {
    let mut hasher = make_hasher();
    query_type.type_hash.hash(&mut hasher);
    file_id.hash(&mut hasher);
    params.hash(&mut hasher);
    QueryId {
        type_hash: query_type.type_hash,
        input_hash: hasher.finish(),
    }
}

#[inline]
pub fn make_query_id(query_type: QueryType, file_id: u64, params: &[u64]) -> QueryId {
    let mut hasher = make_hasher();
    query_type.tag().hash(&mut hasher);
    file_id.hash(&mut hasher);
    params.hash(&mut hasher);
    QueryId {
        type_hash: query_type.tag(),
        input_hash: hasher.finish(),
    }
}
