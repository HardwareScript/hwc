use std::collections::HashMap;

use super::query_ids::QueryId;
use super::results::QueryResult;

#[derive(Clone, Debug)]
pub(crate) struct MemoEntry {
    pub result: QueryResult,
    pub timestamp: u64,
}

#[derive(Debug)]
pub struct QueryStore {
    pub(super) results: HashMap<QueryId, MemoEntry>,
    pub(super) dependencies: HashMap<QueryId, Vec<QueryId>>,
    pub(super) timestamps: HashMap<QueryId, u64>,
    pub(super) current_time: u64,
    pub(super) invalidation_times: HashMap<QueryId, u64>,
}

impl QueryStore {
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
            dependencies: HashMap::new(),
            timestamps: HashMap::new(),
            current_time: 0,
            invalidation_times: HashMap::new(),
        }
    }

    #[inline]
    pub fn execute_query<F>(&mut self, query_id: QueryId, compute: F) -> &QueryResult
    where
        F: FnOnce() -> QueryResult,
    {
        let needs_compute = !self.results.contains_key(&query_id);

        if needs_compute {
            self.current_time += 1;
            let now = self.current_time;
            let result = compute();
            let entry = MemoEntry {
                result,
                timestamp: now,
            };
            self.results.insert(query_id, entry);
            self.timestamps.insert(query_id, now);
        }

        &self.results[&query_id].result
    }

    #[inline]
    pub fn register_input(&mut self, query_id: QueryId) {
        self.dependencies.insert(query_id, Vec::new());
        self.timestamps.insert(query_id, self.current_time);
    }

    #[inline]
    pub fn invalidate_input(&mut self, query_id: QueryId) {
        self.current_time += 1;
        self.invalidation_times.insert(query_id, self.current_time);
        self.mark_stale(query_id);
    }

    #[inline]
    pub fn is_stale(&self, query_id: QueryId) -> bool {
        if let Some(dep_list) = self.dependencies.get(&query_id) {
            if let Some(&query_time) = self.timestamps.get(&query_id) {
                for dep in dep_list {
                    if let Some(&inv_time) = self.invalidation_times.get(dep) {
                        if inv_time > query_time {
                            return true;
                        }
                    }
                }
            }
        }
        !self.results.contains_key(&query_id)
    }

    pub fn mark_stale(&mut self, query_id: QueryId) {
        let dependents: Vec<QueryId> = self
            .dependencies
            .iter()
            .filter(|(_, deps)| deps.contains(&query_id))
            .map(|(&qid, _)| qid)
            .collect();

        for dep_id in dependents {
            self.results.remove(&dep_id);
            self.timestamps.remove(&dep_id);
            self.mark_stale(dep_id);
        }
    }

    #[inline]
    pub fn record_dependency(&mut self, query_id: QueryId, dependency: QueryId) {
        self.dependencies
            .entry(query_id)
            .or_default()
            .push(dependency);
    }

    #[inline]
    pub fn record_dependencies(&mut self, query_id: QueryId, deps: Vec<QueryId>) {
        self.dependencies.entry(query_id).or_default().extend(deps);
    }

    #[inline]
    pub fn get_result(&self, query_id: QueryId) -> Option<&QueryResult> {
        if let Some(entry) = self.results.get(&query_id) {
            if let Some(&inv_time) = self.invalidation_times.get(&query_id) {
                if inv_time > entry.timestamp {
                    return None;
                }
            }
            Some(&entry.result)
        } else {
            None
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.results.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    pub fn clear(&mut self) {
        self.results.clear();
        self.dependencies.clear();
        self.timestamps.clear();
        self.invalidation_times.clear();
        self.current_time = 0;
    }
}

impl Default for QueryStore {
    fn default() -> Self {
        Self::new()
    }
}
