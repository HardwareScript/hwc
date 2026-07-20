use super::config::AutoRouter;

impl<'a> AutoRouter<'a> {
    /// v0.1.8: Set the memoized query store for per-G-cell routing cache.
    pub fn set_query_store(
        &mut self,
        query_store: hwc_engine::geometry_router::query_engine::QueryStore,
    ) {
        self.query_store = Some(query_store);
    }

    /// v0.1.8: Retrieve the memoized query store after routing completes.
    pub fn take_query_store(
        &mut self,
    ) -> Option<hwc_engine::geometry_router::query_engine::QueryStore> {
        self.query_store.take()
    }

    /// v0.1.8: Invalidate memoized routing results for specific G-cells.
    pub fn invalidate_gcells(&mut self, file_id: u64, affected_gcell_ids: &[u32]) {
        if let Some(ref mut qs) = self.query_store {
            for &gcell_id in affected_gcell_ids {
                qs.invalidate_gcell(file_id, gcell_id);
            }
        }
    }

    /// v0.1.8: Invalidate memoized routing results for boundary port relocations.
    pub fn invalidate_boundary_port(&mut self, file_id: u64, adjacent_cell_ids: (u32, u32)) {
        if let Some(ref mut qs) = self.query_store {
            qs.invalidate_boundary_port(file_id, adjacent_cell_ids);
        }
    }
}
