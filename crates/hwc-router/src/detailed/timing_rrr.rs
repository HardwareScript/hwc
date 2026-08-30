//! Timing-Slack-Weighted Negotiated Congestion Rip-Up & Repair (Timing RRR)
//!
//! Weighs routing congestion penalties by timing criticalities (WNS/TNS) so
//! timing-critical nets take shortest planar paths while non-critical nets detour.

use hwc_engine::netlist::NetId;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Default)]
pub struct TimingSlackMap {
    /// Maps NetId -> Timing Slack in picoseconds (negative = setup violation)
    pub slacks_ps: FxHashMap<NetId, f32>,
}

impl TimingSlackMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_slack(&mut self, net_id: NetId, slack_ps: f32) {
        self.slacks_ps.insert(net_id, slack_ps);
    }

    /// Computes criticality weight in [1.0, 5.0] for rip-up and repair priority.
    pub fn get_criticality(&self, net_id: NetId) -> f32 {
        if let Some(&slack) = self.slacks_ps.get(&net_id) {
            if slack < 0.0 {
                // Critical path: give higher priority (up to 5.0x)
                (1.0 + (-slack / 500.0)).min(5.0)
            } else {
                1.0
            }
        } else {
            1.0
        }
    }
}
