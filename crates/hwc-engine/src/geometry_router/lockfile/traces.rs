use super::archived_types::{ArchivedArcSegment, CompactLockfileBinary};
use super::io::LockfileData;
use super::layer_resolution::resolve_z_to_layer_index;
use super::path_reconstruction::reconstruct_path_topology;

pub fn traces_to_lockfile(
    space: &crate::space::HardwareSpace,
    fingerprint: [u8; 32],
) -> Result<CompactLockfileBinary, String> {
    let mut arcs = Vec::new();

    use rustc_hash::FxHashSet;
    let mut seen_segments: FxHashSet<(u32, i64, i64, i64, i64, i64, i64)> = FxHashSet::default();

    for trace in &space.analytic_routes {
        for seg in &trace.segments {
            let dx = (seg.end.x - seg.start.x).abs();
            let dy = (seg.end.y - seg.start.y).abs();
            let dz = (seg.end.z - seg.start.z).abs();

            if dx == 0 && dy == 0 && dz == 0 {
                continue;
            }

            let key =
                if (seg.start.x, seg.start.y, seg.start.z) <= (seg.end.x, seg.end.y, seg.end.z) {
                    (
                        trace.net_id.raw(),
                        seg.start.x,
                        seg.start.y,
                        seg.start.z,
                        seg.end.x,
                        seg.end.y,
                        seg.end.z,
                    )
                } else {
                    (
                        trace.net_id.raw(),
                        seg.end.x,
                        seg.end.y,
                        seg.end.z,
                        seg.start.x,
                        seg.start.y,
                        seg.start.z,
                    )
                };

            if !seen_segments.insert(key) {
                continue;
            }

            let z_center = seg.start.z.min(seg.end.z)
                + ((seg.start.z.max(seg.end.z) - seg.start.z.min(seg.end.z)) / 2);
            let layer_idx = resolve_z_to_layer_index(z_center, &space.entity_graph);

            let material_name = space
                .material_registry
                .get_name(trace.material)
                .ok_or_else(|| {
                    format!(
                        "[LOCK] FATAL: material_id {} not found in registry for net '{}'",
                        trace.material, trace.net_name
                    )
                })?
                .to_string();

            arcs.push(ArchivedArcSegment {
                net_id: trace.net_id.raw(),
                layer: layer_idx,
                width_nm: trace.cross_section.width_nm,
                x1: seg.start.x,
                y1: seg.start.y,
                z1: seg.start.z,
                x2: seg.end.x,
                y2: seg.end.y,
                z2: seg.end.z,
                thickness_nm: trace.cross_section.thickness_nm,
                material_name,
                current_ma: (trace.current.budget_ma * 1000.0) as i64,
            });
        }
    }

    let board_name = space.name.to_string();

    Ok(CompactLockfileBinary {
        version: 1,
        board_name,
        placement_hash: fingerprint,
        arcs,
        instances: Vec::new(),
    })
}

pub fn lockfile_to_traces(
    data: &LockfileData,
    netlist: &crate::netlist::NetlistArena,
    stackup_layers: &[crate::space::StackupLayer],
    material_registry: &crate::material::MaterialRegistry,
) -> Result<Vec<crate::space::AnalyticTrace>, String> {
    use rustc_hash::FxHashMap;

    let d = data.data();
    let mut per_net: FxHashMap<u32, Vec<crate::space::LineSegment>> = FxHashMap::default();
    let mut net_widths: FxHashMap<u32, i64> = FxHashMap::default();
    let mut net_material_names: FxHashMap<u32, String> = FxHashMap::default();
    let mut net_currents: FxHashMap<u32, i64> = FxHashMap::default();

    for arc in d.arcs.iter() {
        per_net
            .entry(arc.net_id)
            .or_default()
            .push(crate::space::LineSegment::new(
                crate::geometry::Point3D::new(arc.x1, arc.y1, arc.z1),
                crate::geometry::Point3D::new(arc.x2, arc.y2, arc.z2),
            ));
        net_widths.entry(arc.net_id).or_insert(arc.width_nm);
        net_material_names
            .entry(arc.net_id)
            .or_insert_with(|| arc.material_name.to_string());
        net_currents.entry(arc.net_id).or_insert(arc.current_ma);
    }

    let mut traces = Vec::new();

    let mut net_ids: Vec<u32> = per_net.keys().copied().collect();
    net_ids.sort_unstable();

    for net_id_raw in net_ids {
        let mut segments = per_net.remove(&net_id_raw).expect("net_id exists");
        if segments.is_empty() {
            continue;
        }

        segments = reconstruct_path_topology(segments);

        let net_id = crate::netlist::NetId::new(net_id_raw);
        let width_nm = net_widths
            .get(&net_id_raw)
            .copied()
            .ok_or_else(|| format!("[LOCK] FATAL: missing width for net {}", net_id_raw))?;
        let net_name = netlist
            .get_net(net_id)
            .map(|n| n.name.clone())
            .ok_or_else(|| format!("[LOCK] FATAL: net {} not found in netlist", net_id_raw))?;

        let material_name = net_material_names
            .get(&net_id_raw)
            .ok_or_else(|| format!("[LOCK] FATAL: missing material for net {}", net_id_raw))?;
        let material_id = material_registry.get_id(material_name).ok_or_else(|| {
            format!(
                "[LOCK] FATAL: material '{}' not found in registry",
                material_name
            )
        })?;
        let current_ma_raw = net_currents.get(&net_id_raw).ok_or_else(|| {
            format!(
                "[LOCK] FATAL: net {} has no current value in lockfile. \
                 Ensure all nets have current_limit declared.",
                net_id_raw
            )
        })?;
        let current_ma = *current_ma_raw as f64 / 1000.0;

        let thickness_nm = d
            .arcs
            .iter()
            .find(|a| a.net_id == net_id_raw)
            .map(|a| a.thickness_nm)
            .ok_or_else(|| format!("[LOCK] FATAL: no arcs found for net {}", net_id_raw))?;

        let net_budget_current_ma = netlist
            .get_net(net_id)
            .and_then(|n| n.current_ma)
            .unwrap_or(0.0);

        // Compute layer_z_range directly from the canonical stackup table.
        // This mirrors the logic in every other routing path (manual, auto, global).
        //
        // **v0.2.2 LAYER LINEAGE**: Extract layer name for explicit lineage tracking
        let layer_z_range_with_name =
            {
                let is_horizontal = segments.iter().all(|s| s.start.z == s.end.z)
                    && segments.windows(2).all(|w| w[0].start.z == w[1].start.z);

                if is_horizontal {
                    let centerline_z = segments.first().map(|s| s.start.z).ok_or_else(|| {
                        format!("[LOCK] FATAL: net {} has no segments", net_id_raw)
                    })?;
                    let count = stackup_layers.len();
                    let layer = stackup_layers
                        .iter()
                        .enumerate()
                        .find(|&(idx, l)| {
                            let is_top = idx == count - 1;
                            if is_top {
                                centerline_z >= l.z_bottom && centerline_z <= l.z_top
                            } else {
                                centerline_z >= l.z_bottom && centerline_z < l.z_top
                            }
                        })
                        .map(|(_, l)| l)
                        .ok_or_else(|| {
                            format!(
                                "[LOCK] FATAL: net {} has a horizontal segment at Z={}nm \
                             that does not match any layer in the stackup. \
                             Delete the lockfile and rebuild.",
                                net_id_raw, centerline_z
                            )
                        })?;
                    Ok::<_, String>(Some(((layer.z_bottom, layer.z_top), layer.name.clone())))
                } else {
                    // Via or multi-layer trace: Z span encoded in segment start/end.
                    // For vias, we must still determine a layer for lineage tracking.
                    let first_z = segments.first().map(|s| s.start.z).ok_or_else(|| {
                        format!("[LOCK] FATAL: net {} has no segments", net_id_raw)
                    })?;

                    let nearest_layer = stackup_layers
                        .iter()
                        .find(|l| first_z >= l.z_bottom && first_z <= l.z_top)
                        .ok_or_else(|| {
                            format!(
                                "[LOCK] FATAL: net {} vertical segment at Z={}nm \
                             does not match any stackup layer. Delete the lockfile and rebuild.",
                                net_id_raw, first_z
                            )
                        })?;

                    // Return the same type structure: ((z_bottom, z_top), name)
                    // For vias, we use the nearest layer's Z bounds
                    Ok::<_, String>(Some((
                        (nearest_layer.z_bottom, nearest_layer.z_top),
                        nearest_layer.name.clone(),
                    )))
                }
            }?;

        let (layer_z_range, route_layer_name) = match layer_z_range_with_name {
            Some((z_range, name)) => (Some(z_range), name),
            None => {
                return Err(format!(
                    "[LOCK] FATAL: Could not determine layer for net {}",
                    net_id_raw
                ))
            }
        };

        traces.push(crate::space::AnalyticTrace::with_layer_z_range(
            crate::space::AnalyticTraceParams {
                net_id,
                cross_section: crate::space::CrossSection::new(width_nm, thickness_nm),
                segments,
                material: material_id,
                net_name,
                current: crate::space::CurrentRating::new(net_budget_current_ma, current_ma),
                layer_z_range,
                layer_name: route_layer_name, // v0.2.2: Explicit layer lineage
            },
        ));
    }

    Ok(traces)
}
