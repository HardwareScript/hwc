//! Routing database statistics for debugging and reporting.

use super::database::HierarchicalRoutingDatabase;
use std::collections::HashSet;
use std::fmt;

/// Statistics about routing data in the database
#[derive(Debug, Clone, Copy)]
pub struct RoutingStatistics {
    /// Total number of route segments from child instances
    pub total_child_segments: usize,

    /// Total number of route segments from parent interconnects
    pub total_parent_segments: usize,

    /// Number of unique child instances with routing data
    pub unique_child_instances: usize,

    /// Number of unique nets in child instance routes
    pub unique_nets_in_children: usize,

    /// Number of parent-level traces
    pub total_parent_traces: usize,
}

impl fmt::Display for RoutingStatistics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Routing Database Statistics:")?;
        writeln!(
            f,
            "  Child instance segments: {}",
            self.total_child_segments
        )?;
        writeln!(
            f,
            "  Parent interconnect segments: {}",
            self.total_parent_segments
        )?;
        writeln!(
            f,
            "  Unique child instances: {}",
            self.unique_child_instances
        )?;
        writeln!(
            f,
            "  Unique nets in children: {}",
            self.unique_nets_in_children
        )?;
        writeln!(f, "  Parent traces: {}", self.total_parent_traces)?;
        Ok(())
    }
}

impl HierarchicalRoutingDatabase {
    /// Get statistics about routing data (for debugging)
    pub fn get_statistics(&self) -> RoutingStatistics {
        let total_child_segments: usize = self
            .child_instance_routes
            .values()
            .map(|segs| segs.len())
            .sum();

        let total_parent_segments: usize = self
            .parent_interconnects
            .iter()
            .map(|trace| trace.segments.len())
            .sum();

        let unique_child_instances: usize = self
            .child_instance_routes
            .keys()
            .map(|(inst, _)| inst)
            .collect::<HashSet<_>>()
            .len();

        let unique_nets_in_children: usize = self
            .child_instance_routes
            .keys()
            .map(|(_, net)| net)
            .collect::<HashSet<_>>()
            .len();

        RoutingStatistics {
            total_child_segments,
            total_parent_segments,
            unique_child_instances,
            unique_nets_in_children,
            total_parent_traces: self.parent_interconnects.len(),
        }
    }
}
