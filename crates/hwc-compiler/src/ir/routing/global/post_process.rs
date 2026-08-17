use super::builder::RoutingData;
use super::config::AutoRouter;
use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::geometry::Point3D;
use hwc_engine::geometry_router::RouteResult;
use hwc_engine::netlist::NetId;

/// Inputs for registering a pre-computed multi-segment analytic route.
///
/// Zero-cost grouping: destructured at function entry so the body operates on
/// plain locals, identical to passing the values individually.
pub(crate) struct AnalyticRouteSegments<'a> {
    pub net_id: NetId,
    pub net_name: &'a str,
    pub segments: Vec<hwc_engine::LineSegment>,
    pub routing_layer_name: &'a str,
    pub thickness_nm: i64,
    pub declared_width_nm: Option<i64>,
    pub current_limit_ma: f64,
}

impl<'a> AutoRouter<'a> {
    pub(crate) fn post_process_routes(
        &mut self,
        mut result: RouteResult,
        data: &RoutingData,
    ) -> Result<(), IrError> {
        if !self.config.route_net_policies.is_empty() {
            result = self.inject_meanders(result, data)?;
        }

        let trace_thickness_nm = self.resolve_trace_thickness(&result)?;
        let _trace_width = self.require_trace_width()?;


        for (net_id_raw, segments) in &result.paths {
            let actual_net_id = if !self.config.auto_routes.is_empty() {
                NetId::new(net_id_raw.raw() % 10000)
            } else {
                *net_id_raw
            };

            let net_name = data
                .net_id_to_name
                .get(net_id_raw)
                .cloned()
                .unwrap_or_else(|| CompactString::from(format!("net_{}", actual_net_id.raw())));

            // **v0.2.0 FIX: Process all path segments for this net and merge into single route**
            // The router may return multiple disconnected path segments. We process each
            // segment independently but only call register_analytic_route once with a merged
            // path to avoid duplicate parent route registration.

            // **BUG FIX v0.2.1: Process route segments independently, then combine**
            // Previously, all route segments were concatenated into a single waypoint array,
            // which caused manhattan_path_to_segments to incorrectly delete valid routes due to
            // false collinearity detection between unrelated segments.
            //
            // The fix: Process each route statement separately to preserve route boundaries,
            // then combine all processed segments into a single AnalyticTrace registration.
            // This prevents both the concatenation bug AND duplicate parent route errors.

            let mut all_segments: Vec<hwc_engine::LineSegment> = Vec::new();
            let mut first_thickness = trace_thickness_nm;
            let mut route_count = 0;

            for path in segments {
                if path.len() < 2 {
                    continue;
                }

//                 eprintln!(
//                     "[POST_PROCESS DEBUG] Net {:?} topological path (len={}):",
//                     net_id_raw,
//                     path.len()
//                 );

                // Process un-mitered path: refine Z & vertical transitions
                let (refined_path, actual_thickness) =
                    self.refine_path_z(path.clone(), trace_thickness_nm)?;

                let mut final_path = refined_path;

                // STRUCTURAL FIX: Only add vertical transitions if the path doesn't already have them
                let has_z_transitions = final_path.windows(2).any(|w| w[0].z != w[1].z);
                if !has_z_transitions {
                    eprintln!("[POST_PROCESS] Path is planar - adding vertical transitions");
                    self.add_vertical_transitions(&mut final_path, &net_name, data);
                } else {
                    eprintln!("[POST_PROCESS] Path already has Z transitions - skipping add_vertical_transitions");
                }

                // Store the first thickness value
                if route_count == 0 {
                    first_thickness = actual_thickness;
                }

                // Convert path to un-mitered Manhattan segments independently
                if final_path.len() >= 2 {
                    let min_seg_len_nm =
                        crate::ir::routing::helpers::require_min_segment_length_nm(self.profile)?;

                    let has_z_transitions = final_path.windows(2).any(|w| w[0].z != w[1].z);
                    let has_diagonal_segments = final_path.windows(2).any(|w| {
                        let dx = (w[1].x - w[0].x).abs();
                        let dy = (w[1].y - w[0].y).abs();
                        let dz = (w[1].z - w[0].z).abs();
                        let moving_axes = (dx > 0) as u8 + (dy > 0) as u8 + (dz > 0) as u8;
                        moving_axes > 1
                    });

                    let route_segments = if has_z_transitions || has_diagonal_segments {
                        let mut segs = Vec::new();
                        for i in 0..final_path.len() - 1 {
                            segs.push(hwc_engine::LineSegment::new(
                                final_path[i],
                                final_path[i + 1],
                            ));
                        }
                        segs
                    } else {
                        crate::ir::routing::helpers::manhattan_path_to_segments(
                            &final_path,
                            min_seg_len_nm,
                        )
                    };

                    all_segments.extend(route_segments);
                    route_count += 1;
                }
            }

            // Register all segments as a single parent route in routing_database
            if !all_segments.is_empty() {
                let declared_width = data
                    .net_declared_widths
                    .get::<str>(net_name.as_ref())
                    .copied();
                let current_ma = self.resolve_net_current(&net_name, data)?;

                // Determine the routing layer for this net
                let routing_layer_name = data
                    .net_layer_names_by_id
                    .get(&actual_net_id)
                    .ok_or_else(|| {
                        IrError::RoutingError(format!(
                            "Could not determine routing layer for net '{}' - no layer name recorded",
                            net_name
                        ))
                    })?;

                // Create a single AnalyticTrace with all segments
                self.register_analytic_route_from_segments(AnalyticRouteSegments {
                    net_id: actual_net_id,
                    net_name: &net_name,
                    segments: all_segments,
                    routing_layer_name,
                    thickness_nm: first_thickness,
                    declared_width_nm: declared_width,
                    current_limit_ma: current_ma,
                })?;
            }
        }

        self.space.entity_graph.commit_route();

        // Step 2: Run Hierarchical Legalizer (QP/Nudge) with full obstacle spatial index
        self.run_legalization(data)?;

        // Step 3: Run 45Â° Miter Pass AFTER Legalization completes
        self.apply_post_legalization_mitering()?;

        // Step 4: Configure spatial & rebuild analytic routes for export
        self.configure_entity_graph_spatial()?;
        self.rebuild_analytic_routes()?;

        Ok(())
    }

    /// Apply 45° miter chamfering to parent interconnects AFTER legalization.
    ///
    /// Pipeline Rule: Topological Route → Legalization → Compaction → 45° Miter Pass → Export
    fn apply_post_legalization_mitering(&mut self) -> Result<(), IrError> {
        let trace_width = self.require_trace_width()?;
        let miter_engine = hwc_engine::MiterEngine::new(trace_width);

        // Step 1: Extract parent trace segment chains to avoid simultaneous mutable & immutable borrow of self.space.
        // Trace segments are grouped into contiguous chains so disconnected routes or branches on the same net
        // are not incorrectly connected into a single false path.
        let parent_traces: Vec<_> = self
            .space
            .routing_database
            .get_parent_interconnects()
            .iter()
            .map(|trace| {
                let mut chains: Vec<Vec<Point3D>> = Vec::new();
                let mut current_chain: Vec<Point3D> = Vec::new();

                for seg in &trace.segments {
                    if current_chain.is_empty() {
                        current_chain.push(seg.start);
                        current_chain.push(seg.end);
                    } else if current_chain.last() == Some(&seg.start) {
                        current_chain.push(seg.end);
                    } else {
                        chains.push(current_chain);
                        current_chain = vec![seg.start, seg.end];
                    }
                }
                if !current_chain.is_empty() {
                    chains.push(current_chain);
                }

                (trace.net_id, chains)
            })
            .collect();

        let mut mitered_segments_by_net = rustc_hash::FxHashMap::default();

        // Step 2: Apply context-aware mitering per contiguous chain
        for (net_id, chains) in parent_traces {
            let mut new_segments = Vec::new();

            for chain in chains {
                if chain.len() < 3 {
                    for window in chain.windows(2) {
                        new_segments.push(hwc_engine::space::LineSegment::new(window[0], window[1]));
                    }
                    continue;
                }

                let mitered_path = miter_engine.apply_miter_pass_with_context(
                    &chain,
                    &*self.space as &dyn hwc_engine::geometry_router::miter_pass::MiterContext,
                    Some(net_id),
                );

                for window in mitered_path.windows(2) {
                    new_segments.push(hwc_engine::space::LineSegment::new(window[0], window[1]));
                }
            }

            if !new_segments.is_empty() {
                mitered_segments_by_net.insert(net_id, new_segments);
            }
        }

        // Step 3: Update routing database with mitered segments
        for trace in self.space.routing_database.get_parent_interconnects_mut() {
            if let Some(new_segs) = mitered_segments_by_net.remove(&trace.net_id) {
                trace.segments = new_segs;
            }
        }

        Ok(())
    }


    fn inject_meanders(
        &mut self,
        result: RouteResult,
        data: &RoutingData,
    ) -> Result<RouteResult, IrError> {
        let trace_width = self.require_trace_width()?;
        let min_clearance = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_spacing_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Meander injection requires spacing constraints.".into(),
                hint: "Add 'trace:' block.".into(),
            })?;

        let injector = crate::ir::meander_injection::MeanderInjector::new(
            &self.config.route_net_policies,
            &data.obstacle_bboxes,
            trace_width,
            min_clearance,
        );
        let result = injector.inject(result);

        Ok(result)
    }

    fn resolve_trace_thickness(&self, result: &RouteResult) -> Result<i64, IrError> {
        let default_thickness = self.space.manufacturing_grid_nm;
        let sample_z = result
            .paths
            .values()
            .next()
            .and_then(|s| s.first())
            .and_then(|p| p.first())
            .map(|p| p.z)
            .unwrap_or(0);
        self.stackup_manager
            .get_layer_index_at_z(sample_z)
            .map(|idx| self.stackup_manager.get_thickness_for_layer_index(idx))
            .unwrap_or(Ok(default_thickness))
    }

    fn require_trace_width(&self) -> Result<i64, IrError> {
        self.space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Missing trace width constraint.".into(),
                hint: "Add 'trace:' block.".into(),
            })
    }

    fn refine_path_z(
        &self,
        mut path: Vec<Point3D>,
        default_thickness: i64,
    ) -> Result<(Vec<Point3D>, i64), IrError> {
        let has_z_transitions = path.windows(2).any(|w| w[0].z != w[1].z);

        if has_z_transitions {
            let first_z = path.first().map(|p| p.z).unwrap_or(0);
            let first_layer = self.stackup_manager.get_layer_index_at_z(first_z);
            let actual_thickness = if let Some(layer_idx) = first_layer {
                self.stackup_manager
                    .get_thickness_for_layer_index(layer_idx)?
            } else {
                default_thickness
            };
            return Ok((path, actual_thickness));
        }

        let first_z = path.first().map(|p| p.z).unwrap_or(0);
        let last_z = path.last().map(|p| p.z).unwrap_or(0);
        let first_layer = self.stackup_manager.get_layer_index_at_z(first_z);
        let last_layer = self.stackup_manager.get_layer_index_at_z(last_z);

        let mut actual_thickness = default_thickness;
        let target_z = match (first_layer, last_layer) {
            (Some(a), Some(b)) if a == b => {
                actual_thickness = self.stackup_manager.get_thickness_for_layer_index(a)?;
                Some((first_z + last_z) / 2)
            }
            (Some(a), _) => {
                actual_thickness = self.stackup_manager.get_thickness_for_layer_index(a)?;
                Some(first_z)
            }
            _ => None,
        };

        if let Some(z) = target_z {
            for point in path.iter_mut() {
                point.z = z;
            }
        } else {
            for point in path.iter_mut() {
                if let Some(idx) = self.stackup_manager.get_layer_index_at_z(point.z) {
                    point.z = self.stackup_manager.get_z_start_nm_for_layer_index(idx)?;
                }
            }
        }
        Ok((path, actual_thickness))
    }

    fn add_vertical_transitions(
        &self,
        path: &mut Vec<Point3D>,
        net_name: &CompactString,
        data: &RoutingData,
    ) {
        if let Some(&target_z) = data.net_layer_targets.get::<str>(net_name.as_ref()) {
            let original_pin_z = path.first().map(|p| p.z).unwrap_or(0);
            let pin_layer = self.stackup_manager.get_layer_index_at_z(original_pin_z);
            let target_layer = self.stackup_manager.get_layer_index_at_z(target_z);

            if match (pin_layer, target_layer) {
                (Some(p), Some(t)) => p != t,
                _ => true,
            } && original_pin_z != target_z
            {
                let start = *path.first().unwrap();
                path.insert(0, Point3D::new(start.x, start.y, original_pin_z));
                path.insert(1, Point3D::new(start.x, start.y, target_z));

                let end = *path.last().unwrap();
                path.push(Point3D::new(end.x, end.y, target_z));
                path.push(Point3D::new(end.x, end.y, original_pin_z));
            }
        }
    }

    fn resolve_net_current(
        &self,
        net_name: &CompactString,
        data: &RoutingData,
    ) -> Result<f64, IrError> {
        data.net_currents_ma
            .get::<str>(net_name.as_ref())
            .copied()
            .ok_or_else(|| {
                if self.profile.as_ref().is_some_and(|p| p.is_asic()) {
                    IrError::MissingAsicConstraint {
                        message: format!("Net '{}' missing current declaration.", net_name),
                        hint: "Add current limit.".into(),
                    }
                } else {
                    IrError::MissingAsicConstraint {
                        message: "Internal error".into(),
                        hint: "".into(),
                    }
                }
            })
            .or_else(|e| {
                if self.profile.as_ref().is_some_and(|p| p.is_asic()) {
                    Err(e)
                } else {
                    Ok(0.0)
                }
            })
    }

    fn run_legalization(&mut self, data: &RoutingData) -> Result<(), IrError> {
        // **v0.2.3: HIERARCHICAL LEGALIZATION OVERHAUL**
        //
        // Post-routing legalization with proper obstacle population and via/port sliding.
        //
        // Pipeline:
        // 1. Fetch parent routes (mutable) and child routes (immutable) from routing_database.
        // 2. Inject all static substrate pours, keepout zones, vias, and contacts as frozen obstacles.
        // 3. Build comprehensive spatial index containing parent routes + ALL frozen obstacles.
        // 4. Run hierarchical legalizer that nudges parent routes around obstacles.
        // 5. Propagate displacement deltas (dx, dy) to connecting vias and docking escape stubs.
        // 6. Update parent routes in routing_database.
        
        let min_clearance = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_spacing_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Legalization requires spacing constraints.".into(),
                hint: "Add 'trace:' block with min_spacing_nm.".into(),
            })?;

//         eprintln!("[LEGALIZATION] Running hierarchical post-routing legalization");

        // Step 1: Extract parent and child segments from routing database
        let (parent_segments, parent_net_ids) = self
            .space
            .routing_database
            .get_parent_segments_for_legalization(&self.space.routing_layer_db);

        let (mut child_segments, mut child_net_ids) = self
            .space
            .routing_database
            .get_child_segments_for_legalization();

        // Add substrate layers (pours, pads, bulk taps, diffusions) as frozen obstacles
        for layer in self.space.entity_graph.get_substrate_layers().iter() {
            let (seg, net_id) = bbox_to_frozen_segment(&layer.bbox, layer.net, layer.material);
            child_segments.push(seg);
            child_net_ids.push(net_id);
        }

        // Add contacts as frozen obstacles
        for contact in &self.space.contacts {
            if let Some(bbox) = contact.bbox {
                let net_id = contact
                    .net
                    .as_ref()
                    .and_then(|name| self.space.netlist.get_net_by_name(name.as_str()))
                    .unwrap_or(hwc_engine::netlist::NetId::UNCONNECTED);
                let mat_id = self
                    .space
                    .material_registry
                    .get_id(&contact.material_name)
                    .unwrap_or(0);
                let (seg, net) = bbox_to_frozen_segment(&bbox, net_id, mat_id);
                child_segments.push(seg);
                child_net_ids.push(net);
            }
        }

        // Add vias as frozen obstacles
        for via in &self.space.vias {
            let r = via.footprint_radius_nm(via.enclosure_nm, 0);
            let bbox = hwc_engine::geometry::BoundingBox::new(
                hwc_engine::geometry::Point3D::new(
                    via.position.0 - r,
                    via.position.1 - r,
                    via.from_z_nm.min(via.to_z_nm),
                ),
                hwc_engine::geometry::Point3D::new(
                    via.position.0 + r,
                    via.position.1 + r,
                    via.from_z_nm.max(via.to_z_nm),
                ),
            );
            let (seg, net) = bbox_to_frozen_segment(&bbox, via.net_id, via.material_id);
            child_segments.push(seg);
            child_net_ids.push(net);
        }

        // Add keepout zones as frozen obstacles
        for bbox in &data.obstacle_bboxes {
            let (seg, net) = bbox_to_frozen_segment(bbox, hwc_engine::netlist::NetId::UNCONNECTED, 0);
            child_segments.push(seg);
            child_net_ids.push(net);
        }

        
        if parent_segments.is_empty() {
//             eprintln!("[LEGALIZATION] No parent routes to legalize - skipping");
            return Ok(());
        }

        // Step 2: Build spatial index with layer awareness
        let profile_layers = self.stackup_manager.ordered_layers();
        let mut z_ranges = Vec::with_capacity(profile_layers.len());
        for i in 0..profile_layers.len() {
            let z_min = self
                .stackup_manager
                .get_layer_start_z(&profile_layers[i])
                .unwrap_or(0);
            let z_max = if i + 1 < profile_layers.len() {
                self.stackup_manager
                    .get_layer_start_z(&profile_layers[i + 1])
                    .unwrap_or(self.space.dimensions.depth_nm)
            } else {
                self.space.dimensions.depth_nm
            };
            z_ranges.push((z_min, z_max));
        }

        // Combine all segments for spatial indexing
        let mut all_segments = parent_segments.clone();
        all_segments.extend(child_segments.clone());
        let mut all_net_ids = parent_net_ids.clone();
        all_net_ids.extend(child_net_ids.clone());

        // Build spatial index
        let mut spatial_index = hwc_engine::geometry_router::DynamicSpatialIndex::new();
        if !z_ranges.is_empty() {
            spatial_index.set_layer_z_ranges(&z_ranges);
        }

        for (idx, (seg, net_id)) in all_segments.iter().zip(all_net_ids.iter()).enumerate() {
            let layer_z = seg.start.z; // Use Z coordinate as layer identifier
            
            // Look up actual thickness from stackup
            let thickness = self
                .stackup_manager
                .get_layer_index_at_z(layer_z)
                .and_then(|layer_idx| {
                    self.stackup_manager
                        .get_thickness_for_layer_index(layer_idx)
                        .ok()
                })
                .unwrap_or(self.space.manufacturing_grid_nm);
            
            spatial_index.insert(hwc_engine::geometry_router::IndexedSegment::new(
                hwc_physics::SpatialEntitySource::RouteSegment {
                    net_idx: net_id.raw() as usize,
                    seg_idx: idx,
                },
                idx,
                *net_id,
                seg,
                layer_z,
                thickness,
            ));
        }

//         eprintln!(
//             "[LEGALIZATION] Built spatial index with {} segments",
//             spatial_index.len()
//         );

        // Step 3: Run hierarchical legalization
        // **Flaw 1 Fix: spatial_index is MOVED (owned) so legalize_hierarchical can rebuild it
        //   at the end of each iteration with updated parent positions.**
        let legalizer = hwc_engine::geometry_router::Legalizer::new(min_clearance);
        let max_iterations = 50;

        let (legalized_parent, legalized_net_ids) = legalizer.legalize_hierarchical(
            &parent_segments,
            &parent_net_ids,
            &child_segments,
            &child_net_ids,
            spatial_index,        // â† moved (not borrowed) â€” Flaw 1 Fix
            max_iterations,
        );

//         eprintln!(
//             "[LEGALIZATION] Legalization complete: {} parent segments processed",
//             legalized_parent.len()
//         );

        // Step 4: Dynamic Via and Port/Docking Sliding
        let mut shifted_vias = Vec::new();
        let mut shifted_contacts = Vec::new();

        for (i, (orig_seg, leg_seg)) in parent_segments.iter().zip(legalized_parent.iter()).enumerate() {
            let net_id = parent_net_ids[i];

            let dx_start = leg_seg.start.x - orig_seg.start.x;
            let dy_start = leg_seg.start.y - orig_seg.start.y;
            let dx_end = leg_seg.end.x - orig_seg.end.x;
            let dy_end = leg_seg.end.y - orig_seg.end.y;

            if dx_start == 0 && dy_start == 0 && dx_end == 0 && dy_end == 0 {
                continue;
            }

            let trace_r = orig_seg.width_nm / 2;

            for via in &mut self.space.vias {
                if via.net_id != net_id {
                    continue;
                }
                let via_r = via.footprint_radius_nm(via.enclosure_nm, 0);
                let capture_radius = trace_r + via_r;
                let cap_sq = capture_radius * capture_radius;

                let dist_start_sq = (via.position.0 - orig_seg.start.x).pow(2)
                    + (via.position.1 - orig_seg.start.y).pow(2);
                let dist_end_sq = (via.position.0 - orig_seg.end.x).pow(2)
                    + (via.position.1 - orig_seg.end.y).pow(2);

                if dist_start_sq <= cap_sq && (dx_start != 0 || dy_start != 0) {
                    let old_vx = via.position.0;
                    let old_vy = via.position.1;
                    via.position.0 += dx_start;
                    via.position.1 += dy_start;
                    shifted_vias.push((via.net_id, old_vx, old_vy, dx_start, dy_start));
                    eprintln!(
                        "[DYNAMIC VIA SLIDING] Shifted via for net {:?} at start by ({},{}) nm",
                        net_id, dx_start, dy_start
                    );
                } else if dist_end_sq <= cap_sq && (dx_end != 0 || dy_end != 0) {
                    let old_vx = via.position.0;
                    let old_vy = via.position.1;
                    via.position.0 += dx_end;
                    via.position.1 += dy_end;
                    shifted_vias.push((via.net_id, old_vx, old_vy, dx_end, dy_end));
                    eprintln!(
                        "[DYNAMIC VIA SLIDING] Shifted via for net {:?} at end by ({},{}) nm",
                        net_id, dx_end, dy_end
                    );
                }
            }

            for contact in &mut self.space.contacts {
                let contact_net_id = contact
                    .net
                    .as_ref()
                    .and_then(|name| self.space.netlist.get_net_by_name(name.as_str()));

                if contact_net_id != Some(net_id) {
                    continue;
                }

                if let Some(ref mut bbox) = contact.bbox {
                    let center_x = (bbox.min.x + bbox.max.x) / 2;
                    let center_y = (bbox.min.y + bbox.max.y) / 2;
                    let contact_r = ((bbox.max.x - bbox.min.x).abs() / 2)
                        .max((bbox.max.y - bbox.min.y).abs() / 2);
                    let capture_radius = trace_r + contact_r;
                    let cap_sq = capture_radius * capture_radius;

                    let dist_start_sq = (center_x - orig_seg.start.x).pow(2)
                        + (center_y - orig_seg.start.y).pow(2);
                    let dist_end_sq =
                        (center_x - orig_seg.end.x).pow(2) + (center_y - orig_seg.end.y).pow(2);

                    if dist_start_sq <= cap_sq && (dx_start != 0 || dy_start != 0) {
                        let old_cx = center_x;
                        let old_cy = center_y;
                        bbox.min.x += dx_start;
                        bbox.max.x += dx_start;
                        bbox.min.y += dy_start;
                        bbox.max.y += dy_start;
                        shifted_contacts.push((net_id, old_cx, old_cy, dx_start, dy_start));
                        eprintln!(
                            "[DYNAMIC CONTACT SLIDING] Shifted contact for net {:?} at start by ({},{}) nm",
                            net_id, dx_start, dy_start
                        );
                    } else if dist_end_sq <= cap_sq && (dx_end != 0 || dy_end != 0) {
                        let old_cx = center_x;
                        let old_cy = center_y;
                        bbox.min.x += dx_end;
                        bbox.max.x += dx_end;
                        bbox.min.y += dy_end;
                        bbox.max.y += dy_end;
                        shifted_contacts.push((net_id, old_cx, old_cy, dx_end, dy_end));
                        eprintln!(
                            "[DYNAMIC CONTACT SLIDING] Shifted contact for net {:?} at end by ({},{}) nm",
                            net_id, dx_end, dy_end
                        );
                    }
                }
            }
        }

        // **Step 4b: Synchronise shifted contact bboxes â†’ EntityGraph**
        if !shifted_contacts.is_empty() {
            use hwc_engine::geometry_router::substrate_types::SubstrateLayerType;
            let grid = self.space.manufacturing_grid_nm;

            for layer in self.space.entity_graph.get_substrate_layers_mut().iter_mut() {
                if layer.layer_type != SubstrateLayerType::Contact {
                    continue;
                }
                let layer_cx = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let layer_cy = (layer.bbox.min.y + layer.bbox.max.y) / 2;

                for &(net, old_cx, old_cy, dx, dy) in &shifted_contacts {
                    if layer.net != net {
                        continue;
                    }
                    if (old_cx - layer_cx).abs() <= grid && (old_cy - layer_cy).abs() <= grid {
                        layer.bbox.min.x += dx;
                        layer.bbox.max.x += dx;
                        layer.bbox.min.y += dy;
                        layer.bbox.max.y += dy;
                        eprintln!(
                            "[ENTITY_GRAPH SYNC] Contact for net {:?}: bbox shifted by ({},{})",
                            net, dx, dy
                        );
                        break;
                    }
                }
            }
        }

        // **Step 4c: Synchronise shifted via positions â†’ EntityGraph**
        if !shifted_vias.is_empty() {
            use hwc_engine::geometry_router::substrate_types::SubstrateLayerType;
            let grid = self.space.manufacturing_grid_nm;

            for layer in self.space.entity_graph.get_substrate_layers_mut().iter_mut() {
                if layer.layer_type != SubstrateLayerType::Contact {
                    continue;
                }
                let layer_cx = (layer.bbox.min.x + layer.bbox.max.x) / 2;
                let layer_cy = (layer.bbox.min.y + layer.bbox.max.y) / 2;

                for &(net_id, old_vx, old_vy, dx, dy) in &shifted_vias {
                    if layer.net != net_id {
                        continue;
                    }
                    if (old_vx - layer_cx).abs() <= grid && (old_vy - layer_cy).abs() <= grid {
                        layer.bbox.min.x += dx;
                        layer.bbox.max.x += dx;
                        layer.bbox.min.y += dy;
                        layer.bbox.max.y += dy;
                        eprintln!(
                            "[ENTITY_GRAPH SYNC] Via for net {:?}: bbox shifted by ({},{})",
                            net_id, dx, dy
                        );
                        break;
                    }
                }
            }
        }

        // Step 5: Update parent routes in routing database
        self.space
            .routing_database
            .update_parent_segments_after_legalization(
                legalized_parent,
                legalized_net_ids,
                &self.space.routing_layer_db,
            );

//         eprintln!("[LEGALIZATION] Parent routes updated in routing database");

        Ok(())
    }


    fn configure_entity_graph_spatial(&mut self) -> Result<(), IrError> {
        if let Some(_profile) = self.profile {
            let profile_layers = self.stackup_manager.ordered_layers();
            if !profile_layers.is_empty() {
                let mut z_ranges = Vec::with_capacity(profile_layers.len());
                for i in 0..profile_layers.len() {
                    let z_min = self
                        .stackup_manager
                        .get_layer_start_z(&profile_layers[i])
                        .unwrap_or(0);
                    let z_max = if i + 1 < profile_layers.len() {
                        self.stackup_manager
                            .get_layer_start_z(&profile_layers[i + 1])
                            .unwrap_or(self.space.dimensions.depth_nm)
                    } else {
                        self.space.dimensions.depth_nm
                    };
                    z_ranges.push((z_min, z_max));
                }
                self.space
                    .entity_graph
                    .set_spatial_layer_z_ranges(&z_ranges);
            }
        }
        Ok(())
    }

    fn rebuild_analytic_routes(&mut self) -> Result<(), IrError> {
        // v0.2.0: Build analytic_routes from the routing database (single source of truth)
        self.space.sync_analytic_routes_from_database();

        // Validate routing database consistency
        self.space.routing_database.validate().map_err(|errors| {
            IrError::RoutingError(format!(
                "Routing database validation failed:\n{}",
                errors.join("\n")
            ))
        })?;

        Ok(())
    }

    /// Register a route from pre-computed segments (v0.2.1 bug fix for multi-segment routes)
    /// This bypasses path concatenation to avoid false collinearity detection in manhattan_path_to_segments
    fn register_analytic_route_from_segments(
        &mut self,
        params: AnalyticRouteSegments,
    ) -> Result<(), IrError> {
        let AnalyticRouteSegments {
            net_id,
            net_name,
            segments,
            routing_layer_name,
            thickness_nm,
            declared_width_nm,
            current_limit_ma,
        } = params;

        use hwc_engine::AnalyticTrace;

        if segments.is_empty() {
            return Ok(());
        }

        let min_width_nm = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Analytic route requires trace width constraint but none is loaded.".into(),
                hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
            })?;

        let trace_width_nm = declared_width_nm.unwrap_or(min_width_nm);

        // Material determination using routing layer
        let material_id = self
            .profile
            .and_then(|p| p.stackup.as_ref())
            .and_then(|stackup| {
                stackup
                    .layers
                    .iter()
                    .find(|l| l.name.name == routing_layer_name)
                    .map(|l| l.material.clone())
            })
            .ok_or_else(|| IrError::UndeclaredMaterial {
                material: format!(
                    "No material defined for routing layer '{}'",
                    routing_layer_name
                )
                .into(),
            })
            .and_then(|mat_name| {
                self.space
                    .material_registry
                    .get_id(&mat_name)
                    .ok_or_else(|| IrError::UndeclaredMaterial {
                        material: mat_name.clone(),
                    })
                    .inspect(|&id| {
                        eprintln!(
                            "[REGISTRY MATERIAL DEBUG] Net '{}': routing_layer='{}', material='{}', material_id={}",
                            net_name, routing_layer_name, mat_name, id
                        );
                    })
            })?;

        let net_budget_current_ma = self
            .space
            .netlist
            .get_net(net_id)
            .and_then(|n| n.current_ma)
            .unwrap_or(0.0);

        // Compute layer_z_range for horizontal traces
        let layer_z_range = segments
            .iter()
            .find(|s| s.start.z == s.end.z)
            .and_then(|s| self.space.find_layer_at_z(s.start.z))
            .map(|layer| (layer.z_bottom, layer.z_top));

        let trace = AnalyticTrace::with_layer_z_range(hwc_engine::space::AnalyticTraceParams {
            net_id,
            cross_section: hwc_engine::space::CrossSection::new(trace_width_nm, thickness_nm),
            segments,
            material: material_id,
            net_name: net_name.into(),
            current: hwc_engine::space::CurrentRating::new(net_budget_current_ma, current_limit_ma),
            layer_z_range,
            layer_name: routing_layer_name.into(), // v0.2.2: Explicit layer lineage
        });

        let from_entity = format!("auto_route_{}_start", net_name);
        let to_entity = format!("auto_route_{}_end", net_name);

        self.space
            .routing_database
            .register_autorouter_route(trace, from_entity.into(), to_entity.into())
            .map_err(IrError::RoutingError)?;

        Ok(())
    }
}

/// Convert a 3D bounding box obstacle into an exact frozen `TraceSegment` for legalization spatial indexing.
fn bbox_to_frozen_segment(
    bbox: &hwc_engine::geometry::BoundingBox,
    net_id: hwc_engine::netlist::NetId,
    material_id: u8,
) -> (hwc_engine::geometry::TraceSegment, hwc_engine::netlist::NetId) {
    use hwc_engine::geometry::{Point3D, TraceSegment};

    let dx = (bbox.max.x - bbox.min.x).abs();
    let dy = (bbox.max.y - bbox.min.y).abs();
    let cx = (bbox.min.x + bbox.max.x) / 2;
    let cy = (bbox.min.y + bbox.max.y) / 2;
    let cz = (bbox.min.z + bbox.max.z) / 2;

    let seg = if dx >= dy {
        let half_w = dy / 2;
        let start = Point3D::new(bbox.min.x + half_w, cy, cz);
        let end = Point3D::new(bbox.max.x - half_w, cy, cz);
        TraceSegment::new_frozen(start, end, dy.max(1), material_id)
    } else {
        let half_w = dx / 2;
        let start = Point3D::new(cx, bbox.min.y + half_w, cz);
        let end = Point3D::new(cx, bbox.max.y - half_w, cz);
        TraceSegment::new_frozen(start, end, dx.max(1), material_id)
    };

    (seg, net_id)
}

