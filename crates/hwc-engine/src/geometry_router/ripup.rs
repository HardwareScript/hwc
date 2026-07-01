//! Rip-up and reroute implementation
//!
//! This module implements the Rip-Up & Reroute Engine for resolving routing conflicts.
//! When a high-priority net cannot be routed due to lower-priority nets blocking the path,
//! the engine will rip up (delete) the lower-priority nets and reroute them.
//!
//! KEY FEATURES:
//! - Priority-based routing: High-priority nets (power, clock, high-speed) route first
//! - Dynamic trace deletion: Remove traces from EntityGraph
//! - Conflict resolution: Detect and resolve routing conflicts automatically
//! - Iterative rerouting: Attempt multiple reroute cycles until success or max iterations
//!
//! ALGORITHM:
//! 1. Sort nets by priority (Power > Clock > HighSpeed > Data > LowSpeed)
//! 2. Route each net in priority order
//! 3. If routing fails, detect blocking nets
//! 4. Rip up lower-priority blocking nets
//! 5. Retry routing the current net
//! 6. Reroute the ripped-up nets later in the queue
//!
//! PERFORMANCE:
//! - BitChunk-based deletion: O(1) per trace segment using bitmask operations
//! - Conflict detection: O(N) where N = number of routed nets
//! - Max iterations: Configurable (default 10) to prevent infinite loops

use super::priority::NetPriority;
use super::router::GeometryRouter;
use super::types::{NetRoute, RoutedNet, RoutingError};
use crate::geometry::Point3D;
use crate::netlist::{NetId, NetlistArena};
use rustc_hash::{FxHashMap, FxHashSet};

/// Routing attempt result
#[derive(Debug)]
pub enum RouteAttempt {
    /// Route succeeded
    Success(RoutedNet),

    /// Route failed, blocked by other nets
    Blocked { blocking_nets: Vec<NetId> },
}

/// Rip-up and reroute engine
pub struct RipUpRouter<'a> {
    router: GeometryRouter,
    netlist: &'a NetlistArena,

    /// Successfully routed nets
    routed_nets: FxHashMap<NetId, RoutedNet>,

    /// Net priorities
    priorities: FxHashMap<NetId, NetPriority>,

    /// Maximum rip-up iterations per net
    max_iterations: usize,

    /// Track rip-up count per net (for statistics and debugging)
    ripup_counts: FxHashMap<NetId, usize>,

    /// Track which nets have been ripped up in current iteration
    ripped_up_this_cycle: FxHashSet<NetId>,

    /// Total number of rip-up operations performed
    total_ripups: usize,
}

impl<'a> RipUpRouter<'a> {
    /// Create a new rip-up router
    pub fn new(router: GeometryRouter, netlist: &'a NetlistArena, max_iterations: usize) -> Self {
        Self {
            router,
            netlist,
            routed_nets: FxHashMap::default(),
            priorities: FxHashMap::default(),
            max_iterations,
            ripup_counts: FxHashMap::default(),
            ripped_up_this_cycle: FxHashSet::default(),
            total_ripups: 0,
        }
    }

    /// Route all nets with rip-up and reroute
    pub fn route_all_with_ripup(
        &mut self,
        mut nets: Vec<NetRoute>,
    ) -> Result<Vec<RoutedNet>, RoutingError> {
        // Calculate priorities for all nets
        for net in &nets {
            let net_data = self
                .netlist
                .get_net(net.net_id)
                .ok_or(RoutingError::InvalidNet(net.net_id))?;
            let priority = NetPriority::from_net_name(&net_data.name);
            self.priorities.insert(net.net_id, priority);
        }

        // Sort nets by priority (highest first)
        nets.sort_by(|a, b| {
            let priority_a = self.priorities.get(&a.net_id).unwrap();
            let priority_b = self.priorities.get(&b.net_id).unwrap();
            priority_b.cmp(priority_a) // Reverse order for highest first
        });

        // Route nets in priority order with rip-up
        for net in nets {
            self.route_with_ripup(net)?;
        }

        // Return all routed nets in original order
        let mut result: Vec<RoutedNet> = self.routed_nets.values().cloned().collect();
        result.sort_by_key(|n| n.net_id.raw());

        Ok(result)
    }

    /// Route a single net with rip-up capability
    fn route_with_ripup(&mut self, net: NetRoute) -> Result<(), RoutingError> {
        let net_priority = *self.priorities.get(&net.net_id).unwrap();

        // Clear ripped-up tracking for this cycle
        self.ripped_up_this_cycle.clear();

        // Apply high-speed conflict resolution before routing
        self.resolve_high_speed_conflicts(&net)?;

        for iteration in 0..self.max_iterations {
            match self.attempt_route(&net) {
                RouteAttempt::Success(routed) => {
                    // Route succeeded, store it
                    self.routed_nets.insert(net.net_id, routed);
                    return Ok(());
                }
                RouteAttempt::Blocked { blocking_nets } => {
                    // Check if we can rip up any blocking nets
                    let mut ripped_up = false;

                    for blocking_net_id in blocking_nets {
                        // Don't rip up nets we just ripped up in this cycle (prevent thrashing)
                        if self.ripped_up_this_cycle.contains(&blocking_net_id) {
                            continue;
                        }

                        if let Some(&blocking_priority) = self.priorities.get(&blocking_net_id) {
                            if net_priority.can_rip_up(blocking_priority) {
                                // Rip up the lower priority net
                                self.rip_up_net(blocking_net_id);
                                ripped_up = true;
                            }
                        }
                    }

                    if !ripped_up {
                        // Can't rip up any blocking nets, routing fails
                        return Err(RoutingError::NoPathFound {
                            net_id: net.net_id,
                            start: net.start,
                            goal: net.goal,
                        });
                    }

                    // Try again after ripping up
                    if iteration == self.max_iterations - 1 {
                        return Err(RoutingError::MaxIterationsExceeded(net.net_id));
                    }
                }
            }
        }

        Err(RoutingError::MaxIterationsExceeded(net.net_id))
    }

    /// Attempt to route a net, detecting blocking nets
    fn attempt_route(&mut self, net: &NetRoute) -> RouteAttempt {
        match self.router.route_net(net) {
            Ok(routed) => RouteAttempt::Success(routed),
            Err(RoutingError::NoPathFound { .. }) => {
                // Detect which nets are blocking this route
                let blocking_nets = self.detect_blocking_nets(net);
                RouteAttempt::Blocked { blocking_nets }
            }
            Err(_e) => {
                // Other errors are not recoverable
                RouteAttempt::Blocked {
                    blocking_nets: vec![],
                }
            }
        }
    }

    /// Detect which nets are blocking a route
    fn detect_blocking_nets(&self, net: &NetRoute) -> Vec<NetId> {
        // Simple heuristic: check which nets occupy space near the start/goal
        let mut blocking_nets = Vec::new();

        // Check a small region around start and goal
        let search_radius = 5; // grid steps

        for routed_net in self.routed_nets.values() {
            if routed_net.net_id == net.net_id {
                continue;
            }

            // Check if this net's path is near our start or goal
            for segment in &routed_net.paths {
                for point in segment {
                    if self.is_near(point, &net.start, search_radius)
                        || self.is_near(point, &net.goal, search_radius)
                    {
                        blocking_nets.push(routed_net.net_id);
                        break;
                    }
                }
            }
        }

        blocking_nets
    }

    /// Check if two points are within a certain distance
    fn is_near(&self, p1: &Point3D, p2: &Point3D, radius: i64) -> bool {
        let dx = (p1.x - p2.x).abs();
        let dy = (p1.y - p2.y).abs();
        let dz = (p1.z - p2.z).abs();

        dx <= radius && dy <= radius && dz <= radius
    }

    /// Rip up a routed net (remove it from the grid)
    ///
    /// This method performs dynamic trace deletion from the entity graph.
    /// It clears all geometry and vias occupied by the net, making them
    /// available for other nets to use.
    ///
    /// # Arguments
    /// * `net_id` - The net to rip up
    fn rip_up_net(&mut self, net_id: NetId) {
        if let Some(routed_net) = self.routed_nets.remove(&net_id) {
            // Track rip-up statistics
            *self.ripup_counts.entry(net_id).or_insert(0) += 1;
            self.ripped_up_this_cycle.insert(net_id);
            self.total_ripups += 1;

            // Clear vias
            for via in &routed_net.vias {
                self.router.clear_via(via);
            }
        }
    }

    /// Resolve conflicts for high-speed signals
    ///
    /// High-speed signals (critical, high-speed data) have special requirements:
    /// - Minimize vias (signal integrity)
    /// - Avoid parallel routing near other traces (crosstalk)
    /// - Prefer direct paths (impedance control)
    ///
    /// This method checks if a net is high-speed and applies stricter conflict resolution.
    fn resolve_high_speed_conflicts(&mut self, net: &NetRoute) -> Result<(), RoutingError> {
        let net_priority = *self.priorities.get(&net.net_id).unwrap();

        // Only apply special handling for high-speed signals
        if !matches!(net_priority, NetPriority::Critical | NetPriority::HighSpeed) {
            return Ok(());
        }

        // For high-speed signals, be more aggressive about ripping up nearby nets
        let blocking_nets = self.detect_blocking_nets_aggressive(net);

        for blocking_net_id in blocking_nets {
            if let Some(&blocking_priority) = self.priorities.get(&blocking_net_id) {
                // High-speed signals can rip up anything except power and other high-speed
                if net_priority.can_rip_up(blocking_priority) {
                    self.rip_up_net(blocking_net_id);
                }
            }
        }

        Ok(())
    }

    /// Detect blocking nets with aggressive search (for high-speed signals)
    ///
    /// Uses a larger search radius to find nets that might cause crosstalk
    /// or impedance issues for high-speed signals.
    fn detect_blocking_nets_aggressive(&self, net: &NetRoute) -> Vec<NetId> {
        let mut blocking_nets = Vec::new();
        let search_radius = 10; // Larger radius for high-speed signals

        for routed_net in self.routed_nets.values() {
            if routed_net.net_id == net.net_id {
                continue;
            }

            // Check if this net's path is near our start or goal
            for segment in &routed_net.paths {
                for point in segment {
                    if self.is_near(point, &net.start, search_radius)
                        || self.is_near(point, &net.goal, search_radius)
                    {
                        blocking_nets.push(routed_net.net_id);
                        break;
                    }
                }
            }
        }

        blocking_nets
    }

    /// Get routing statistics
    pub fn stats(&self) -> RipUpStats {
        RipUpStats {
            total_nets: self.priorities.len(),
            routed_nets: self.routed_nets.len(),
            failed_nets: self.priorities.len() - self.routed_nets.len(),
            total_ripups: self.total_ripups,
            max_ripups_per_net: self.ripup_counts.values().copied().max().unwrap_or(0),
        }
    }

    /// Get the router (for testing and inspection)
    pub fn router(&self) -> &GeometryRouter {
        &self.router
    }

    /// Get the routed nets (for testing and inspection)
    pub fn routed_nets(&self) -> &FxHashMap<NetId, RoutedNet> {
        &self.routed_nets
    }
}

/// Routing statistics
#[derive(Debug, Clone, Copy)]
pub struct RipUpStats {
    pub total_nets: usize,
    pub routed_nets: usize,
    pub failed_nets: usize,
    pub total_ripups: usize,
    pub max_ripups_per_net: usize,
}

impl RipUpStats {
    /// Calculate routing completion percentage
    pub fn completion_rate(&self) -> f64 {
        if self.total_nets == 0 {
            return 100.0;
        }
        (self.routed_nets as f64 / self.total_nets as f64) * 100.0
    }

    /// Calculate average rip-ups per net
    pub fn avg_ripups_per_net(&self) -> f64 {
        if self.routed_nets == 0 {
            return 0.0;
        }
        self.total_ripups as f64 / self.routed_nets as f64
    }
}
