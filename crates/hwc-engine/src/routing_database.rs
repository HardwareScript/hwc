//! Hierarchical Routing Database
//!
//! Maintains clear separation between child-instance routes and parent-level
//! interconnects, enabling proper connectivity validation and error reporting
//! in hierarchical designs.
//!
//! # Architecture
//!
//! ```text
//! Parent Space (Inverter_Cell)
//! ├── Child Instance Routes (immutable after flattening)
//! │   ├── PMOS_Inst.VDD: [route segments in parent coords]
//! │   ├── PMOS_Inst.Out: [route segments in parent coords]
//! │   ├── NMOS_Inst.GND: [route segments in parent coords]
//! │   └── NMOS_Inst.Out: [route segments in parent coords]
//! └── Parent Interconnects (created by parent)
//!     ├── Out: PMOS_Inst.Out_Pad → NMOS_Inst.Out_Pad
//!     └── In: PMOS_Inst.Gate_Strip → NMOS_Inst.Gate_Strip
//! ```
//!
//! # Key Principles
//!
//! 1. **Immutable Child Routes**: Once a child space is flattened, its routes
//!    are transformed to parent coordinates and stored immutably.
//!
//! 2. **Parent Interconnects**: Routes created at the parent level to connect
//!    between child instances or to external ports.
//!
//! 3. **Provenance Tracking**: Every route knows its source (child instance
//!    or parent level) for debugging and error reporting.
//!
//! 4. **Lazy Merging**: Child and parent routes are only merged on-demand
//!    during connectivity validation - never stored merged.

use crate::geometry::TraceSegment;
use crate::netlist::NetId;
use crate::space::AnalyticTrace;
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use std::fmt;

/// Unique identifier for a route segment (for provenance tracking)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RouteId(u64);

impl RouteId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Source of a route segment (child instance or parent level)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteSource {
    /// Route originated from a child space instance
    ChildInstance {
        /// Instance name (e.g., "PMOS_Inst")
        instance: CompactString,
        /// Original net name in child space
        original_net: CompactString,
    },
    
    /// Route created at parent level
    ParentLevel {
        /// Source entity name (e.g., "PMOS_Inst.Out_Pad")
        from_entity: CompactString,
        /// Destination entity name (e.g., "NMOS_Inst.Out_Pad")
        to_entity: CompactString,
    },
}

impl fmt::Display for RouteSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouteSource::ChildInstance { instance, original_net } => {
                write!(f, "child instance '{}' (original net: {})", instance, original_net)
            }
            RouteSource::ParentLevel { from_entity, to_entity } => {
                write!(f, "parent-level route: {} → {}", from_entity, to_entity)
            }
        }
    }
}

/// A route segment with full provenance information
#[derive(Debug, Clone)]
pub struct ProvenanceSegment {
    /// Network this segment belongs to
    pub net_id: NetId,
    
    /// Network name (for debugging)
    pub net_name: Option<CompactString>,
    
    /// The actual geometric segment
    pub segment: TraceSegment,
    
    /// Where this segment came from
    pub source: RouteSource,
    
    /// Unique identifier for this segment
    pub route_id: RouteId,
}

/// Hierarchical routing database
///
/// This is the single source of truth for all routing data in a space,
/// maintaining clear separation between child and parent routes.
#[derive(Debug, Clone)]
pub struct HierarchicalRoutingDatabase {
    /// Routes from child space instances (immutable after flattening)
    /// Key: (instance_name, net_id)
    /// Value: Route segments already transformed to parent coordinates
    child_instance_routes: FxHashMap<(CompactString, NetId), Vec<TraceSegment>>,
    
    /// Parent-level interconnect routes
    /// These connect between instances or to external ports
    parent_interconnects: Vec<AnalyticTrace>,
    
    /// Metadata for debugging and error reporting
    /// Maps route_id to source information
    route_provenance: FxHashMap<RouteId, RouteSource>,
    
    /// Counter for generating unique RouteIds
    next_route_id: u64,
}

impl HierarchicalRoutingDatabase {
    /// Create a new empty routing database
    pub fn new() -> Self {
        Self {
            child_instance_routes: FxHashMap::default(),
            parent_interconnects: Vec::new(),
            route_provenance: FxHashMap::default(),
            next_route_id: 0,
        }
    }
    
    /// Register routes from a child instance (called during hierarchical flattening)
    ///
    /// # Parameters
    ///
    /// - `instance_name`: Name of the child instance (e.g., "PMOS_Inst")
    /// - `net_id`: Network ID in the parent space (after remapping)
    /// - `original_net_name`: Original network name in the child space
    /// - `segments`: Route segments already transformed to parent coordinates
    pub fn register_child_routes(
        &mut self,
        instance_name: CompactString,
        net_id: NetId,
        original_net_name: CompactString,
        segments: Vec<TraceSegment>,
    ) {
        let source = RouteSource::ChildInstance {
            instance: instance_name.clone(),
            original_net: original_net_name,
        };
        
        // Store provenance for each segment
        for _ in &segments {
            let route_id = RouteId::new(self.next_route_id);
            self.next_route_id += 1;
            self.route_provenance.insert(route_id, source.clone());
        }
        
        // Store the segments
        let key = (instance_name.clone(), net_id);
        self.child_instance_routes
            .entry(key.clone())
            .or_insert_with(Vec::new)
            .extend(segments);
        
        eprintln!(
            "[ROUTING DB] Registered child routes: instance='{}', net_id={:?}, source={}",
            key.0, key.1, source
        );
    }
    
    /// Register a parent-level interconnect route
    ///
    /// # Parameters
    ///
    /// - `trace`: The analytic trace created by parent-level routing
    /// - `from_entity`: Source entity name
    /// - `to_entity`: Destination entity name
    pub fn register_parent_route(
        &mut self,
        trace: AnalyticTrace,
        from_entity: CompactString,
        to_entity: CompactString,
    ) {
        let source = RouteSource::ParentLevel {
            from_entity: from_entity.clone(),
            to_entity: to_entity.clone(),
        };
        
        // Store provenance for each segment
        for _ in &trace.segments {
            let route_id = RouteId::new(self.next_route_id);
            self.next_route_id += 1;
            self.route_provenance.insert(route_id, source.clone());
        }
        
        eprintln!(
            "[ROUTING DB] Registered parent route: net='{}' (id={:?}), from='{}', to='{}', segments={}",
            trace.net_name, trace.net_id, from_entity, to_entity, trace.segments.len()
        );
        
        self.parent_interconnects.push(trace);
    }
    
    /// Get unified connectivity view for validation
    ///
    /// This merges child and parent routes into a single list for connectivity
    /// checking, while preserving provenance information for error reporting.
    pub fn get_connectivity_view(&self) -> Vec<ProvenanceSegment> {
        let mut segments = Vec::new();
        let mut route_id = 0u64;
        
        // Add child instance routes
        for ((instance, net_id), route_segs) in &self.child_instance_routes {
            for seg in route_segs {
                let id = RouteId::new(route_id);
                route_id += 1;
                
                let source = self.route_provenance
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| RouteSource::ChildInstance {
                        instance: instance.clone(),
                        original_net: "unknown".into(),
                    });
                
                segments.push(ProvenanceSegment {
                    net_id: *net_id,
                    net_name: None, // Will be filled by caller if needed
                    segment: seg.clone(),
                    source,
                    route_id: id,
                });
            }
        }
        
        // Add parent interconnect routes
        for trace in &self.parent_interconnects {
            for seg_line in &trace.segments {
                let id = RouteId::new(route_id);
                route_id += 1;
                
                let source = self.route_provenance
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| RouteSource::ParentLevel {
                        from_entity: "unknown".into(),
                        to_entity: "unknown".into(),
                    });
                
                // Convert LineSegment to TraceSegment
                let trace_seg = TraceSegment::new(
                    seg_line.start,
                    seg_line.end,
                    trace.cross_section.width_nm,
                    trace.material,
                );
                
                segments.push(ProvenanceSegment {
                    net_id: trace.net_id,
                    net_name: Some(trace.net_name.clone()),
                    segment: trace_seg,
                    source,
                    route_id: id,
                });
            }
        }
        
        segments
    }
    
    /// Validate hierarchical connectivity
    ///
    /// Checks that nets appearing in multiple child instances have parent-level
    /// routing to connect them. Returns detailed error information if not.
    pub fn validate_hierarchical_connectivity(&self) -> Result<(), Vec<ConnectivityError>> {
        let mut errors = Vec::new();
        
        // Group child routes by net_id to find nets in multiple instances
        let mut net_to_instances: FxHashMap<NetId, Vec<CompactString>> = FxHashMap::default();
        
        for ((instance, net_id), _) in &self.child_instance_routes {
            net_to_instances
                .entry(*net_id)
                .or_insert_with(Vec::new)
                .push(instance.clone());
        }
        
        // Check each net that appears in multiple child instances
        for (net_id, instances) in &net_to_instances {
            if instances.len() > 1 {
                // Net exists in multiple child instances - check for parent routing
                let has_parent_route = self.parent_interconnects
                    .iter()
                    .any(|trace| trace.net_id == *net_id);
                
                if !has_parent_route {
                    // Get original net names from each instance
                    let instance_details: Vec<_> = instances
                        .iter()
                        .map(|inst| {
                            let original_net = self.child_instance_routes
                                .iter()
                                .find(|((i, n), _)| i == inst && n == net_id)
                                .and_then(|((_, _), _)| {
                                    self.route_provenance
                                        .values()
                                        .find_map(|src| match src {
                                            RouteSource::ChildInstance { instance: i, original_net } 
                                                if i == inst => Some(original_net.clone()),
                                            _ => None,
                                        })
                                });
                            
                            (inst.clone(), original_net)
                        })
                        .collect();
                    
                    errors.push(ConnectivityError::IsolatedChildInstances {
                        net_id: *net_id,
                        instances: instance_details,
                    });
                }
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// Get statistics about routing data (for debugging)
    pub fn get_statistics(&self) -> RoutingStatistics {
        let total_child_segments: usize = self.child_instance_routes
            .values()
            .map(|segs| segs.len())
            .sum();
        
        let total_parent_segments: usize = self.parent_interconnects
            .iter()
            .map(|trace| trace.segments.len())
            .sum();
        
        let unique_child_instances: usize = self.child_instance_routes
            .keys()
            .map(|(inst, _)| inst)
            .collect::<std::collections::HashSet<_>>()
            .len();
        
        let unique_nets_in_children: usize = self.child_instance_routes
            .keys()
            .map(|(_, net)| net)
            .collect::<std::collections::HashSet<_>>()
            .len();
        
        RoutingStatistics {
            total_child_segments,
            total_parent_segments,
            unique_child_instances,
            unique_nets_in_children,
            total_parent_traces: self.parent_interconnects.len(),
        }
    }
    
    /// Export for legacy entity_graph.routed_segments() compatibility
    ///
    /// Returns all routes (child + parent) grouped by net_id.
    /// Used during transition period to maintain compatibility.
    pub fn export_as_routed_segments(&self) -> Vec<(NetId, Vec<TraceSegment>)> {
        let mut net_segments: FxHashMap<NetId, Vec<TraceSegment>> = FxHashMap::default();
        
        // Add child routes
        for ((_, net_id), segments) in &self.child_instance_routes {
            net_segments
                .entry(*net_id)
                .or_insert_with(Vec::new)
                .extend(segments.clone());
        }
        
        // Add parent routes
        for trace in &self.parent_interconnects {
            let segments: Vec<TraceSegment> = trace.segments
                .iter()
                .map(|line_seg| TraceSegment::new(
                    line_seg.start,
                    line_seg.end,
                    trace.cross_section.width_nm,
                    trace.material,
                ))
                .collect();
            
            net_segments
                .entry(trace.net_id)
                .or_insert_with(Vec::new)
                .extend(segments);
        }
        
        net_segments.into_iter().collect()
    }
    
    /// Clear all routing data (used during re-registration)
    pub fn clear(&mut self) {
        self.child_instance_routes.clear();
        self.parent_interconnects.clear();
        self.route_provenance.clear();
        self.next_route_id = 0;
    }
    
    /// Get parent interconnects (for analytic_routes compatibility)
    pub fn get_parent_interconnects(&self) -> &[AnalyticTrace] {
        &self.parent_interconnects
    }
    
    /// Get all child instance names
    pub fn get_child_instances(&self) -> Vec<CompactString> {
        self.child_instance_routes
            .keys()
            .map(|(inst, _)| inst.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }
    
    /// Check if a net has any routing data (child or parent)
    pub fn has_routing_for_net(&self, net_id: NetId) -> bool {
        // Check child routes
        let in_child = self.child_instance_routes
            .keys()
            .any(|(_, n)| *n == net_id);
        
        // Check parent routes
        let in_parent = self.parent_interconnects
            .iter()
            .any(|trace| trace.net_id == net_id);
        
        in_child || in_parent
    }

    /// Register a parent-level route created by the AutoRouter.
    ///
    /// This is called during AutoRouter's route creation, not post-processing.
    /// Validates that this net doesn't already have a parent route.
    pub fn register_autorouter_route(
        &mut self,
        trace: AnalyticTrace,
        from_entity: CompactString,
        to_entity: CompactString,
    ) -> Result<(), String> {
        if self.parent_interconnects.iter().any(|t| t.net_id == trace.net_id) {
            return Err(format!(
                "Duplicate parent route for net {:?}. Parent routes must be registered exactly once.",
                trace.net_id
            ));
        }

        let source = RouteSource::ParentLevel {
            from_entity: from_entity.clone(),
            to_entity: to_entity.clone(),
        };

        let route_id = RouteId::new(self.next_route_id);
        self.next_route_id += 1;
        self.route_provenance.insert(route_id, source);

        self.parent_interconnects.push(trace);
        Ok(())
    }

    /// Build the unified analytic_routes vector from the routing database.
    ///
    /// This is the ONLY way to populate `space.analytic_routes`.
    /// Child routes are converted from TraceSegment to AnalyticTrace format.
    ///
    /// # Arguments
    ///
    /// * `netlist` - Reference to the netlist for getting net names
    /// * `stackup_layers` - Reference to the stackup layers for looking up layer bounds
    pub fn build_analytic_routes(
        &self,
        netlist: &crate::netlist::NetlistArena,
        stackup_layers: &[crate::space::StackupLayer],
    ) -> Vec<AnalyticTrace> {
        eprintln!(
            "[ROUTING DB BUILD] Starting build_analytic_routes: {} parent routes, {} child route groups",
            self.parent_interconnects.len(),
            self.child_instance_routes.len()
        );
        
        let mut routes = Vec::new();

        // Parent interconnects are already AnalyticTrace
        eprintln!("[ROUTING DB BUILD] Extending with {} parent interconnects", self.parent_interconnects.len());
        routes.extend(self.parent_interconnects.iter().cloned());

        // Convert child routes to AnalyticTrace format
        eprintln!("[ROUTING DB BUILD] Processing {} child route groups", self.child_instance_routes.len());
        for ((instance_name, net_id), segments) in &self.child_instance_routes {
            if segments.is_empty() {
                continue;
            }

            let width_nm = segments[0].width_nm;
            let material = segments[0].material_id;

            let line_segments: Vec<crate::space::LineSegment> = segments
                .iter()
                .map(|seg| crate::space::LineSegment::new(
                    seg.start,
                    seg.end,
                ))
                .collect();

            let net_name = netlist
                .get_net(*net_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("{}_child_route", instance_name).into());

            // **v0.2.1 FIX: Calculate layer_z_range for child routes**
            // Child routes often contain vias + horizontal segments.
            // Find the most common horizontal Z level (the routing layer)
            eprintln!(
                "[ROUTING DB] Processing child route: instance='{}', net={:?}, {} segments",
                instance_name, net_id, line_segments.len()
            );
            
            let layer_z_range = if !line_segments.is_empty() {
                // Collect all horizontal segments (where start.z == end.z)
                let horizontal_z_levels: Vec<i64> = line_segments
                    .iter()
                    .filter(|s| s.start.z == s.end.z)
                    .map(|s| s.start.z)
                    .collect();
                
                eprintln!(
                    "[ROUTING DB]   Found {} horizontal segments at Z levels: {:?}",
                    horizontal_z_levels.len(),
                    horizontal_z_levels
                );
                
                // If we have horizontal segments, find the most common Z level
                if let Some(&first_horiz_z) = horizontal_z_levels.first() {
                    let centerline_z = first_horiz_z;
                    
                    eprintln!(
                        "[ROUTING DB] Child route for net={:?}, instance='{}': looking up layer at Z={}nm (stackup has {} layers)",
                        net_id, instance_name, centerline_z, stackup_layers.len()
                    );
                    
                    for (idx, layer) in stackup_layers.iter().enumerate() {
                        eprintln!(
                            "[ROUTING DB]   Layer {}: z_bottom={}, z_top={}, name='{}'",
                            idx, layer.z_bottom, layer.z_top, layer.name
                        );
                    }
                    
                    // Look up the layer from stackup (single source of truth).
                    // Use half-open intervals [z_bottom, z_top) for all layers except the
                    // topmost, to match HardwareSpace::find_layer_at_z semantics and avoid
                    // ambiguity at shared layer boundaries (e.g. Z=1250 is metal1.z_bottom,
                    // not d1.z_top).
                    let layer_count = stackup_layers.len();
                    let result = stackup_layers
                        .iter()
                        .enumerate()
                        .find(|(idx, layer)| {
                            let is_last = *idx == layer_count - 1;
                            let matches = if is_last {
                                centerline_z >= layer.z_bottom && centerline_z <= layer.z_top
                            } else {
                                centerline_z >= layer.z_bottom && centerline_z < layer.z_top
                            };
                            eprintln!(
                                "[ROUTING DB]   Checking layer '{}': z_bottom={}, z_top={}, centerline={}, matches={}",
                                layer.name, layer.z_bottom, layer.z_top, centerline_z, matches
                            );
                            matches
                        })
                        .map(|(_, layer)| {
                            eprintln!(
                                "[ROUTING DB]   ✓ Found layer '{}' at Z={}→{}nm for centerline Z={}nm",
                                layer.name, layer.z_bottom, layer.z_top, centerline_z
                            );
                            (layer.z_bottom, layer.z_top)
                        });
                    
                    if result.is_none() {
                        eprintln!(
                            "[ROUTING DB]   ✗ No layer found for centerline Z={}nm!",
                            centerline_z
                        );
                    }
                    
                    result
                } else {
                    eprintln!("[ROUTING DB]   No horizontal segments found - route is pure vias");
                    None
                }
            } else {
                None
            };

            routes.push(AnalyticTrace::with_layer_z_range(
                *net_id,
                crate::space::CrossSection::new(width_nm, 400),
                line_segments,
                material,
                net_name,
                crate::space::CurrentRating::new(0.0, 0.0),
                layer_z_range,
            ));
        }

        eprintln!(
            "[ROUTING DB BUILD] Returning {} total routes ({} parent + {} child converted)",
            routes.len(),
            self.parent_interconnects.len(),
            routes.len() - self.parent_interconnects.len()
        );
        
        routes
    }

    /// Validate routing database consistency.
    ///
    /// Returns errors if child routes exist without parent interconnects for nets
    /// that appear in multiple instances.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Group child routes by net_id
        let mut nets_with_multiple_instances: FxHashMap<NetId, Vec<CompactString>> =
            FxHashMap::default();

        for ((instance, net_id), _) in &self.child_instance_routes {
            nets_with_multiple_instances
                .entry(*net_id)
                .or_insert_with(Vec::new)
                .push(instance.clone());
        }

        // Check that nets appearing in multiple instances have parent routes
        for (net_id, instances) in nets_with_multiple_instances {
            if instances.len() > 1 {
                let has_parent_route = self.parent_interconnects
                    .iter()
                    .any(|trace| trace.net_id == net_id);

                if !has_parent_route {
                    errors.push(format!(
                        "Net {:?} appears in {} instances ({}) but has no parent-level interconnect",
                        net_id,
                        instances.len(),
                        instances.join(", ")
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for HierarchicalRoutingDatabase {
    fn default() -> Self {
        Self::new()
    }
}

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
        writeln!(f, "  Child instance segments: {}", self.total_child_segments)?;
        writeln!(f, "  Parent interconnect segments: {}", self.total_parent_segments)?;
        writeln!(f, "  Unique child instances: {}", self.unique_child_instances)?;
        writeln!(f, "  Unique nets in children: {}", self.unique_nets_in_children)?;
        writeln!(f, "  Parent traces: {}", self.total_parent_traces)?;
        Ok(())
    }
}

/// Connectivity errors detected during hierarchical validation
#[derive(Debug, Clone)]
pub enum ConnectivityError {
    /// A net appears in multiple child instances but has no parent-level routing
    IsolatedChildInstances {
        /// Network ID
        net_id: NetId,
        
        /// List of (instance_name, original_net_name) pairs
        instances: Vec<(CompactString, Option<CompactString>)>,
    },
}

impl fmt::Display for ConnectivityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectivityError::IsolatedChildInstances { net_id, instances } => {
                writeln!(f, "❌ HIERARCHICAL CONNECTIVITY ERROR")?;
                writeln!(f)?;
                writeln!(f, "Net {:?} exists in {} child instances but has NO parent-level routing:", 
                    net_id, instances.len())?;
                writeln!(f)?;
                
                for (instance, original_net) in instances {
                    if let Some(orig) = original_net {
                        writeln!(f, "  • Instance '{}' (original net: '{}')", instance, orig)?;
                    } else {
                        writeln!(f, "  • Instance '{}'", instance)?;
                    }
                }
                
                writeln!(f)?;
                writeln!(f, "These instances have internal routing but are NOT connected to each other.")?;
                writeln!(f)?;
                writeln!(f, "Suggested fix:")?;
                writeln!(f, "  Add a parent-level route statement in your space to connect these instances.")?;
                writeln!(f, "  Example:")?;
                writeln!(f, "    route {}.PowerRail to {}.PowerRail:",
                    instances[0].0, instances.get(1).map(|(i, _)| i.as_str()).unwrap_or("OtherInst"))?;
                writeln!(f, "        net: YourNetName")?;
                writeln!(f, "        width: 200nm")?;
                writeln!(f, "        layer: metal1")?;
                
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3D;
    use crate::material::MaterialId;
    use crate::space::CrossSection;
    
    #[test]
    fn test_empty_database() {
        let db = HierarchicalRoutingDatabase::new();
        let stats = db.get_statistics();
        
        assert_eq!(stats.total_child_segments, 0);
        assert_eq!(stats.total_parent_segments, 0);
        assert_eq!(stats.unique_child_instances, 0);
    }
    
    #[test]
    fn test_register_child_routes() {
        let mut db = HierarchicalRoutingDatabase::new();
        
        let seg = TraceSegment::new(
            Point3D::new(0, 0, 0),
            Point3D::new(100, 100, 0),
            200,
            MaterialId(1),
        );
        
        db.register_child_routes(
            "PMOS_Inst".into(),
            NetId::new(1),
            "VDD".into(),
            vec![seg],
        );
        
        let stats = db.get_statistics();
        assert_eq!(stats.total_child_segments, 1);
        assert_eq!(stats.unique_child_instances, 1);
    }
    
    #[test]
    fn test_hierarchical_validation_pass() {
        let mut db = HierarchicalRoutingDatabase::new();
        
        // Single instance - should pass
        let seg = TraceSegment::new(
            Point3D::new(0, 0, 0),
            Point3D::new(100, 100, 0),
            200,
            MaterialId(1),
        );
        
        db.register_child_routes(
            "PMOS_Inst".into(),
            NetId::new(1),
            "VDD".into(),
            vec![seg],
        );
        
        let result = db.validate_hierarchical_connectivity();
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_hierarchical_validation_fail() {
        let mut db = HierarchicalRoutingDatabase::new();
        
        let seg1 = TraceSegment::new(
            Point3D::new(0, 0, 0),
            Point3D::new(100, 100, 0),
            200,
            MaterialId(1),
        );
        
        let seg2 = TraceSegment::new(
            Point3D::new(200, 200, 0),
            Point3D::new(300, 300, 0),
            200,
            MaterialId(1),
        );
        
        // Same net in two instances - should fail without parent route
        db.register_child_routes(
            "PMOS_Inst".into(),
            NetId::new(1),
            "VDD".into(),
            vec![seg1],
        );
        
        db.register_child_routes(
            "NMOS_Inst".into(),
            NetId::new(1),
            "VDD".into(),
            vec![seg2],
        );
        
        let result = db.validate_hierarchical_connectivity();
        assert!(result.is_err());
        
        if let Err(errors) = result {
            assert_eq!(errors.len(), 1);
        }
    }
}
