use crate::geometry::Point3D;
use rustc_hash::FxHashMap;

/// Congestion state for a single routing resource (grid cell or track).
#[derive(Clone, Debug)]
pub struct ResourceState {
    pub base_cost: i64,
    pub historical_cost: i64,
    pub present_count: usize,
}

impl ResourceState {
    pub fn new(base_cost: i64) -> Self {
        Self {
            base_cost,
            historical_cost: 0,
            present_count: 0,
        }
    }

    #[inline]
    pub fn total_cost(&self) -> i64 {
        let present_penalty = if self.present_count <= 1 {
            1
        } else {
            self.present_count as i64
        };
        (self.base_cost + self.historical_cost) * present_penalty
    }
}

/// The negotiated congestion engine — PathFinder-style iterative routing.
pub struct NegotiatedCongestionEngine {
    pub iteration: usize,
    pub max_iterations: usize,
    present_usage: FxHashMap<Point3D, usize>,
    historical_usage: FxHashMap<Point3D, i64>,
    pub cost_history: Vec<i64>,
}

impl NegotiatedCongestionEngine {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            iteration: 0,
            max_iterations,
            present_usage: FxHashMap::default(),
            historical_usage: FxHashMap::default(),
            cost_history: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.iteration = 0;
        self.present_usage.clear();
        self.historical_usage.clear();
        self.cost_history.clear();
    }

    #[inline]
    pub fn resource_cost(&self, pos: Point3D, base_cost: i64) -> i64 {
        let historical = self.historical_usage.get(&pos).copied().unwrap_or(0);
        let present = self.present_usage.get(&pos).copied().unwrap_or(0);
        let present_penalty = if present <= 1 { 1 } else { present as i64 };
        (base_cost + historical) * present_penalty
    }

    #[inline]
    pub fn use_resource(&mut self, pos: Point3D) {
        *self.present_usage.entry(pos).or_insert(0) += 1;
    }

    pub fn commit_iteration(&mut self) {
        for (pos, count) in &self.present_usage {
            if *count > 1 {
                let historical = self.historical_usage.entry(*pos).or_insert(0);
                *historical += (*count - 1) as i64 * 10;
            }
        }
        self.present_usage.clear();
        self.iteration += 1;
    }

    #[inline]
    pub fn is_complete(&self) -> bool {
        self.present_usage.values().all(|&count| count <= 1)
    }

    #[inline]
    pub fn is_exhausted(&self) -> bool {
        self.iteration >= self.max_iterations
    }

    #[inline]
    pub fn total_cost(&self) -> i64 {
        self.present_usage.values().map(|&c| c as i64).sum()
    }

    pub fn negotiate<F>(&mut self, mut route_all_nets: F) -> (usize, bool)
    where
        F: FnMut(&NegotiatedCongestionEngine) -> i64,
    {
        self.reset();
        loop {
            let cost = route_all_nets(self);
            self.cost_history.push(cost);
            if self.is_complete() || self.is_exhausted() {
                break;
            }
            self.commit_iteration();
        }
        (self.iteration, self.is_complete())
    }
}

impl Default for NegotiatedCongestionEngine {
    fn default() -> Self {
        Self::new(50)
    }
}
