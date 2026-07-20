#[derive(Clone, Debug)]
pub struct AstResult {
    pub file_id: u64,
    pub node_count: usize,
    pub hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct SymbolResult {
    pub file_id: u64,
    pub symbol_count: usize,
    pub hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct PartitionResult {
    pub file_id: u64,
    pub gcell_count: usize,
    pub hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct RouteResult {
    pub file_id: u64,
    pub gcell_id: u32,
    pub segment_count: usize,
    pub hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct VerifyResult {
    pub file_id: u64,
    pub gcell_id: u32,
    pub violation_count: usize,
    pub hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub enum QueryResult {
    ParseAst(AstResult),
    ResolveSymbols(SymbolResult),
    PartitionGcells(PartitionResult),
    RouteGcell(RouteResult),
    VerifyGcell(VerifyResult),
}

impl QueryResult {
    #[inline]
    pub fn as_ast(&self) -> Option<&AstResult> {
        match self {
            QueryResult::ParseAst(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_symbols(&self) -> Option<&SymbolResult> {
        match self {
            QueryResult::ResolveSymbols(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_partition(&self) -> Option<&PartitionResult> {
        match self {
            QueryResult::PartitionGcells(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_route(&self) -> Option<&RouteResult> {
        match self {
            QueryResult::RouteGcell(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_verify(&self) -> Option<&VerifyResult> {
        match self {
            QueryResult::VerifyGcell(r) => Some(r),
            _ => None,
        }
    }
}
