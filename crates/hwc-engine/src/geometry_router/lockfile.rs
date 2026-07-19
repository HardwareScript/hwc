//! Semantic Lockfile System — rkyv binary format (Roadmap 6.1)
//!
//! Zero-copy, memory-mapped lockfile for deterministic route caching.
//! Uses rkyv 0.7 for serialization and memmap2 for memory-mapped I/O.
//! All coordinates are i64 nanometers. No f64 in core path.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::Path;

// ---------------------------------------------------------------------------
// Archived structs — rkyv 0.7 with check_bytes validation
// ---------------------------------------------------------------------------

/// A single arc segment stored in the lockfile.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug)]
#[archive(check_bytes)]
pub struct ArchivedArcSegment {
    pub net_id: u32,
    pub layer: u16,
    pub width_nm: i64,
    pub x1: i64,
    pub y1: i64,
    pub z1: i64, // v0.1.9: Added for vector engine - preserves vertical connectivity
    pub x2: i64,
    pub y2: i64,
    pub z2: i64, // v0.1.9: Added for vector engine - preserves vertical connectivity
    pub thickness_nm: i64,
    pub material_name: String,
    pub current_ma: i64,
}

/// A component instance stored in the lockfile.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug)]
#[archive(check_bytes)]
pub struct ArchivedComponentInstance {
    pub id: u32,
    pub x_nm: i64,
    pub y_nm: i64,
    pub rotation_deg: i64,
    pub mirror: bool,
}

/// Top-level binary lockfile. Memory-mappable and zero-copy accessible.
///
/// Uses `String` instead of `CompactString` because `CompactString` does not
/// implement rkyv's `Archive` trait. The archived form stores strings as
/// `rkyv::string::ArchivedString` with inline small-string optimization.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug)]
#[archive(check_bytes)]
pub struct CompactLockfileBinary {
    pub version: u32,
    pub board_name: String,
    pub placement_hash: [u8; 32],
    pub arcs: Vec<ArchivedArcSegment>,
    pub instances: Vec<ArchivedComponentInstance>,
}

// ---------------------------------------------------------------------------
// Semantic fingerprint
// ---------------------------------------------------------------------------

/// Compute a SHA-256 fingerprint from component bounds, routing rules hash,
/// stackup hash, and router version. The result is deterministic for identical
/// inputs — a mismatch invalidates the lockfile.
#[inline]
pub fn compute_fingerprint(
    component_bounds: &[(i64, i64, i64, i64)],
    rules_hash: &[u8; 32],
    stackup_hash: &[u8; 32],
    router_version: u32,
) -> [u8; 32] {
    let mut hasher = Sha256::new();

    for &(min_x, min_y, max_x, max_y) in component_bounds {
        hasher.update(min_x.to_le_bytes());
        hasher.update(min_y.to_le_bytes());
        hasher.update(max_x.to_le_bytes());
        hasher.update(max_y.to_le_bytes());
    }

    hasher.update(rules_hash);
    hasher.update(stackup_hash);
    hasher.update(router_version.to_le_bytes());

    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

// ---------------------------------------------------------------------------
// Semantic Layer Resolution
// ---------------------------------------------------------------------------

/// Resolve a physical Z-coordinate to a semantic layer index.
///
/// Scans the entity graph's substrate layers to find which layer owns the
/// given Z-position. Returns a stable u8 layer index for lockfile storage.
fn resolve_z_to_layer_index(
    z_nm: i64,
    entity_graph: &crate::geometry_router::entity_graph::EntityGraph,
) -> u16 {
    use rustc_hash::FxHashMap;
    let mut z_to_layer: FxHashMap<i64, u16> = FxHashMap::default();
    let mut next_idx: u16 = 0;

    for layer in entity_graph.get_substrate_layers() {
        let z_min = layer.bbox.min.z;
        let z_max = layer.bbox.max.z;
        let z_mid = (z_min + z_max) / 2;

        if let std::collections::hash_map::Entry::Vacant(e) = z_to_layer.entry(z_mid) {
            e.insert(next_idx);
            next_idx = next_idx.wrapping_add(1);
        }
    }

    // Find the closest Z-layer
    let mut best_layer: u16 = 0;
    let mut best_dist: i64 = i64::MAX;
    for (layer_z, layer_idx) in &z_to_layer {
        let dist = (z_nm - layer_z).abs();
        if dist < best_dist {
            best_dist = dist;
            best_layer = *layer_idx;
        }
    }
    best_layer
}

/// Build a layer-index → Z-position mapping from substrate layers.
///
/// Returns a `Vec<(u16, i64)>` suitable for passing to `lockfile_to_traces`.
pub fn build_layer_z_map(
    entity_graph: &crate::geometry_router::entity_graph::EntityGraph,
) -> Vec<(u16, i64)> {
    use rustc_hash::FxHashMap;
    let mut z_to_layer: FxHashMap<i64, u16> = FxHashMap::default();
    let mut next_idx: u16 = 0;

    for layer in entity_graph.get_substrate_layers() {
        let z_min = layer.bbox.min.z;
        let z_max = layer.bbox.max.z;
        let z_mid = (z_min + z_max) / 2;

        if let std::collections::hash_map::Entry::Vacant(e) = z_to_layer.entry(z_mid) {
            e.insert(next_idx);
            next_idx = next_idx.wrapping_add(1);
        }
    }

    z_to_layer.into_iter().map(|(z, idx)| (idx, z)).collect()
}

// ---------------------------------------------------------------------------
// Write lockfile
// ---------------------------------------------------------------------------

/// Serialize a lockfile to disk using rkyv with 16-byte alignment.
///
/// Uses `rkyv::to_bytes` with a 1 MiB scratch buffer (const generic `N`).
/// The 16-byte alignment ensures the mmap can be used directly.
pub fn write_lockfile(lockfile: &CompactLockfileBinary, path: &Path) -> io::Result<()> {
    // 1 MiB scratch buffer — large enough for typical lockfiles
    let bytes: rkyv::AlignedVec = rkyv::to_bytes::<_, 1_048_576>(lockfile)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("rkyv serialize: {e}")))?;

    fs::write(path, bytes.as_slice())
}

// ---------------------------------------------------------------------------
// Zero-copy memory-mapped load
// ---------------------------------------------------------------------------

/// Holds the mmap and provides access to the archived lockfile data.
/// The lockfile data is accessed directly from the mmap — zero parsing overhead.
pub struct LockfileData {
    _mmap: memmap2::Mmap,
    ptr: *const <CompactLockfileBinary as rkyv::Archive>::Archived,
}

// Safety: LockfileData is Send/Sync because the archived data is immutable
// once written and the mmap is read-only mapped.
unsafe impl Send for LockfileData {}
unsafe impl Sync for LockfileData {}

impl LockfileData {
    /// Access the archived lockfile data.
    #[inline]
    pub fn data(&self) -> &<CompactLockfileBinary as rkyv::Archive>::Archived {
        // SAFETY: ptr is validated by check_archived_root during load
        unsafe { &*self.ptr }
    }
}

/// Load a lockfile via memory mapping with zero-copy access.
/// The returned `LockfileData` borrows the mmap; access is O(1).
///
/// Uses `check_archived_root` for validated zero-copy access.
pub fn load_lockfile(path: &Path) -> io::Result<LockfileData> {
    let file = fs::File::open(path)?;
    let mmap =
        unsafe { memmap2::Mmap::map(&file).map_err(|e| io::Error::other(format!("mmap: {e}")))? };

    // Validate the archived data with check_bytes
    let archived = rkyv::validation::validators::check_archived_root::<CompactLockfileBinary>(
        &mmap,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("rkyv validation: {e}")))?;

    let ptr: *const _ = archived;
    Ok(LockfileData { _mmap: mmap, ptr })
}

// ---------------------------------------------------------------------------
// Lockfile validation
// ---------------------------------------------------------------------------

/// Check whether a loaded lockfile is still valid for the given fingerprint.
/// On mismatch the caller should discard the lock and re-run the pathfinder.
#[inline]
pub fn is_valid(loaded: &LockfileData, current_fingerprint: &[u8; 32]) -> bool {
    loaded.data().placement_hash == *current_fingerprint
}

// ---------------------------------------------------------------------------
// CLI inspect — decode binary to human-readable JSON
// ---------------------------------------------------------------------------

/// Decode a binary lockfile to a human-readable JSON string.
/// No secondary JSON file is generated during builds.
pub fn inspect_lockfile(path: &Path) -> io::Result<String> {
    let loaded = load_lockfile(path)?;
    let data = loaded.data();

    let arcs: Vec<serde_json::Value> = data
        .arcs
        .iter()
        .map(|a| {
            serde_json::json!({
                "net_id": a.net_id,
                "layer": a.layer,
                "width_nm": a.width_nm,
                "x1": a.x1,
                "y1": a.y1,
                "z1": a.z1,
                "x2": a.x2,
                "y2": a.y2,
                "z2": a.z2,
                "thickness_nm": a.thickness_nm,
                "material_name": &*a.material_name,
                "current_ma": a.current_ma,
            })
        })
        .collect();

    let instances: Vec<serde_json::Value> = data
        .instances
        .iter()
        .map(|inst| {
            serde_json::json!({
                "id": inst.id,
                "x_nm": inst.x_nm,
                "y_nm": inst.y_nm,
                "rotation_deg": inst.rotation_deg,
                "mirror": inst.mirror,
            })
        })
        .collect();

    let board_name: &str = &data.board_name;

    let obj = serde_json::json!({
        "version": data.version,
        "board_name": board_name,
        "placement_hash": data.placement_hash.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "arcs": arcs,
        "instances": instances,
    });

    serde_json::to_string_pretty(&obj)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("json: {e}")))
}

// ---------------------------------------------------------------------------
// Path Topology Reconstruction
// ---------------------------------------------------------------------------

/// Reconstruct the original path connectivity from an unordered set of segments.
///
/// When segments are saved to the lockfile and loaded back via HashMap iteration,
/// they lose their original order. This function rebuilds a connected path by
/// finding segments that share endpoints.
///
/// Algorithm:
/// 1. Build an adjacency graph of segments (which segments connect to which)
/// 2. Find chain endpoints (segments with only one connection)
/// 3. Walk the chain from start to end, building the ordered path
/// 4. Handle branching/multi-path nets gracefully (keep all segments)
fn reconstruct_path_topology(
    mut segments: Vec<crate::space::LineSegment>,
) -> Vec<crate::space::LineSegment> {
    if segments.len() <= 1 {
        return segments; // Single segment or empty - no reordering needed
    }

    // Build adjacency map: for each segment, find which other segments share an endpoint
    use rustc_hash::FxHashMap;
    let mut connections: FxHashMap<usize, Vec<usize>> = FxHashMap::default();

    for (i, seg_i) in segments.iter().enumerate() {
        for (j, seg_j) in segments.iter().enumerate() {
            if i == j {
                continue;
            }
            // Check if segments share an endpoint
            if seg_i.end == seg_j.start
                || seg_i.end == seg_j.end
                || seg_i.start == seg_j.start
                || seg_i.start == seg_j.end
            {
                connections.entry(i).or_default().push(j);
            }
        }
    }

    // Find a starting segment (prefer one with only one connection = chain endpoint)
    let start_idx = connections
        .iter()
        .find(|(_, neighbors)| neighbors.len() == 1)
        .map(|(idx, _)| *idx)
        .unwrap_or(0); // Fallback to first segment if no clear endpoint

    // Walk the chain from start, building ordered path
    let mut ordered = Vec::new();
    let mut visited = vec![false; segments.len()];
    let mut current = start_idx;

    while !visited[current] {
        visited[current] = true;
        ordered.push(current);

        // Find next unvisited neighbor
        if let Some(neighbors) = connections.get(&current) {
            if let Some(&next) = neighbors.iter().find(|&&n| !visited[n]) {
                current = next;
            } else {
                break; // No more unvisited neighbors
            }
        } else {
            break; // No neighbors
        }
    }

    // Collect any unvisited segments (handles branching nets)
    for (i, &vis) in visited.iter().enumerate() {
        if !vis {
            ordered.push(i);
        }
    }

    // Rebuild segment vector in the reconstructed order
    let original_segments = segments.clone();
    segments.clear();
    for &idx in &ordered {
        segments.push(original_segments[idx].clone());
    }

    segments
}

// ---------------------------------------------------------------------------
// Bridge: HardwareSpace ↔ CompactLockfileBinary
// ---------------------------------------------------------------------------

/// Compute a `[u8; 32]` fingerprint directly from a `HardwareSpace`.
///
/// This bridges the old `compute_placement_hash` (hex string) and the new
/// binary lockfile's `[u8; 32]` placement_hash field.
pub fn compute_fingerprint_from_space(space: &crate::space::HardwareSpace) -> [u8; 32] {
    let mut component_bounds: Vec<(i64, i64, i64, i64)> = space
        .component_bboxes
        .values()
        .map(|bbox| (bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y))
        .collect();
    component_bounds.sort();

    let rules_hash = {
        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}", space.fabrication_constraints).as_bytes());
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    };
    let stackup_hash = {
        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}", space.substrate_material_id).as_bytes());
        hasher.update(format!("{:?}", space.dimensions).as_bytes());
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    };
    let router_version = {
        let mut hasher = Sha256::new();
        hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
        let result = hasher.finalize();
        u32::from_le_bytes([result[0], result[1], result[2], result[3]])
    };

    compute_fingerprint(
        &component_bounds,
        &rules_hash,
        &stackup_hash,
        router_version,
    )
}

/// Build a `CompactLockfileBinary` from the analytic routes in a `HardwareSpace`.
///
/// Each `LineSegment` in each `AnalyticTrace` becomes one `ArchivedArcSegment`.
pub fn traces_to_lockfile(
    space: &crate::space::HardwareSpace,
    fingerprint: [u8; 32],
) -> Result<CompactLockfileBinary, String> {
    let mut arcs = Vec::new();

    // v0.1.9: DEDUPLICATION MAP
    // The routing engine creates bidirectional segments (A->B and B->A) for connectivity.
    // We only need to store one direction to preserve the route geometry.
    use rustc_hash::FxHashSet;
    let mut seen_segments: FxHashSet<(u32, i64, i64, i64, i64, i64, i64)> = FxHashSet::default();

    for trace in &space.analytic_routes {
        for seg in &trace.segments {
            // v0.1.9: CRITICAL FIX - Skip zero-length point markers
            let dx = (seg.end.x - seg.start.x).abs();
            let dy = (seg.end.y - seg.start.y).abs();
            let dz = (seg.end.z - seg.start.z).abs();

            if dx == 0 && dy == 0 && dz == 0 {
                // True zero-length point marker - skip it
                continue;
            }

            // v0.1.9: DEDUPLICATION - Create canonical key (always min->max order)
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

            // Skip if we've already seen this segment (or its reverse)
            if !seen_segments.insert(key) {
                continue;
            }

            // v0.1.9: VECTOR ENGINE FIX
            // Store the ACTUAL 3D vector coordinates (x1,y1,z1) -> (x2,y2,z2)
            // The layer field is now only used for semantic grouping/debugging
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
                current_ma: (trace.current.actual_ma * 1000.0) as i64,
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

/// Convert a loaded binary lockfile into `AnalyticTrace` objects.
///
/// Groups arc segments by `net_id` and builds one `AnalyticTrace` per net.
/// Uses semantic layer resolution: the stored `layer` index is mapped to a
/// physical Z-coordinate via `layer_z_positions`, making the lockfile
/// resilient to stackup thickness changes.
pub fn lockfile_to_traces(
    data: &LockfileData,
    netlist: &crate::netlist::NetlistArena,
    layer_z_positions: &[(u16, i64)],
    material_registry: &crate::material::MaterialRegistry,
) -> Result<Vec<crate::space::AnalyticTrace>, String> {
    use rustc_hash::FxHashMap;

    let d = data.data();
    let mut per_net: FxHashMap<u32, Vec<crate::space::LineSegment>> = FxHashMap::default();
    let mut net_widths: FxHashMap<u32, i64> = FxHashMap::default();
    let mut net_material_names: FxHashMap<u32, String> = FxHashMap::default();
    let mut net_currents: FxHashMap<u32, i64> = FxHashMap::default();

    let _layer_to_z: FxHashMap<u16, i64> = layer_z_positions.iter().copied().collect();

    for arc in d.arcs.iter() {
        // v0.1.9: VECTOR ENGINE FIX
        // Use the stored z1/z2 coordinates directly instead of resolving from layer.
        // This preserves vertical connectivity for vias and maintains the true 3D vector nature.
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

    // v0.1.9: Sort net IDs to ensure deterministic trace order
    let mut net_ids: Vec<u32> = per_net.keys().copied().collect();
    net_ids.sort_unstable();

    for net_id_raw in net_ids {
        let mut segments = per_net.remove(&net_id_raw).expect("net_id exists");
        if segments.is_empty() {
            continue;
        }

        // v0.1.9: CRITICAL FIX - Reconstruct path connectivity
        // The lockfile stores segments in arbitrary order (HashMap iteration).
        // We must rebuild the original path topology by connecting segments end-to-end.
        // This ensures the physical continuity checker sees a connected chain.
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
        // v0.1.8: Fail-fast if current is missing from lockfile. No hardcoded defaults.
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

        // Get net's actual operating current from netlist
        let net_actual_current_ma = netlist
            .get_net(net_id)
            .and_then(|n| n.current_ma)
            .unwrap_or(0.0);

        traces.push(crate::space::AnalyticTrace::new(
            net_id,
            crate::space::CrossSection::new(width_nm, thickness_nm),
            segments,
            material_id,
            net_name,
            crate::space::CurrentRating::new(net_actual_current_ma, current_ma),
        ));
    }

    Ok(traces)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_deterministic() {
        let bounds = vec![(0i64, 0, 1000, 2000)];
        let rules = [0xABu8; 32];
        let stackup = [0xCDu8; 32];
        let ver = 1u32;

        let a = compute_fingerprint(&bounds, &rules, &stackup, ver);
        let b = compute_fingerprint(&bounds, &rules, &stackup, ver);
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_differs_on_input_change() {
        let bounds_a = vec![(0i64, 0, 1000, 2000)];
        let bounds_b = vec![(0i64, 0, 2000, 2000)];
        let rules = [0xABu8; 32];
        let stackup = [0xCDu8; 32];
        let ver = 1u32;

        let h1 = compute_fingerprint(&bounds_a, &rules, &stackup, ver);
        let h2 = compute_fingerprint(&bounds_b, &rules, &stackup, ver);
        assert_ne!(h1, h2);
    }

    #[test]
    fn fingerprint_differs_on_version_change() {
        let bounds = vec![(0i64, 0, 1000, 2000)];
        let rules = [0xABu8; 32];
        let stackup = [0xCDu8; 32];

        let h1 = compute_fingerprint(&bounds, &rules, &stackup, 1);
        let h2 = compute_fingerprint(&bounds, &rules, &stackup, 2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn write_and_load_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.routes.lock");

        let lockfile = CompactLockfileBinary {
            version: 1,
            board_name: "test_board".into(),
            placement_hash: [0x42u8; 32],
            arcs: vec![
                ArchivedArcSegment {
                    net_id: 1,
                    layer: 0,
                    width_nm: 100_000,
                    x1: 0,
                    y1: 0,
                    z1: 0,
                    x2: 5_000_000,
                    y2: 0,
                    z2: 0,
                    thickness_nm: 35_000,
                    material_name: "Copper".to_string(),
                    current_ma: 20_000,
                },
                ArchivedArcSegment {
                    net_id: 2,
                    layer: 1,
                    width_nm: 150_000,
                    x1: 1_000_000,
                    y1: 2_000_000,
                    z1: 100_000,
                    x2: 1_000_000,
                    y2: 8_000_000,
                    z2: 100_000,
                    thickness_nm: 35_000,
                    material_name: "Copper".to_string(),
                    current_ma: 20_000,
                },
            ],
            instances: vec![
                ArchivedComponentInstance {
                    id: 100,
                    x_nm: 500_000,
                    y_nm: 600_000,
                    rotation_deg: 90,
                    mirror: false,
                },
                ArchivedComponentInstance {
                    id: 200,
                    x_nm: 1_200_000,
                    y_nm: 3_400_000,
                    rotation_deg: 0,
                    mirror: true,
                },
            ],
        };

        write_lockfile(&lockfile, &path).expect("write");

        let loaded = load_lockfile(&path).expect("load");
        let data = loaded.data();

        assert_eq!(data.version, 1);
        assert_eq!(data.board_name.as_str(), "test_board");
        assert_eq!(data.placement_hash, [0x42u8; 32]);
        assert_eq!(data.arcs.len(), 2);
        assert_eq!(data.instances.len(), 2);

        let a0 = &data.arcs[0];
        assert_eq!(a0.net_id, 1);
        assert_eq!(a0.layer, 0);
        assert_eq!(a0.width_nm, 100_000);
        assert_eq!(a0.x1, 0);
        assert_eq!(a0.y1, 0);
        assert_eq!(a0.x2, 5_000_000);
        assert_eq!(a0.y2, 0);
        assert_eq!(a0.thickness_nm, 35_000);
        assert_eq!(a0.material_name, "Copper");
        assert_eq!(a0.current_ma, 20_000);

        let a1 = &data.arcs[1];
        assert_eq!(a1.net_id, 2);
        assert_eq!(a1.layer, 1);
        assert_eq!(a1.width_nm, 150_000);
        assert_eq!(a1.x1, 1_000_000);
        assert_eq!(a1.y1, 2_000_000);
        assert_eq!(a1.x2, 1_000_000);
        assert_eq!(a1.y2, 8_000_000);
        assert_eq!(a1.thickness_nm, 35_000);
        assert_eq!(a1.material_name, "Copper");
        assert_eq!(a1.current_ma, 20_000);

        let i0 = &data.instances[0];
        assert_eq!(i0.id, 100);
        assert_eq!(i0.x_nm, 500_000);
        assert_eq!(i0.y_nm, 600_000);
        assert_eq!(i0.rotation_deg, 90);
        assert!(!i0.mirror);

        let i1 = &data.instances[1];
        assert_eq!(i1.id, 200);
        assert_eq!(i1.x_nm, 1_200_000);
        assert_eq!(i1.y_nm, 3_400_000);
        assert_eq!(i1.rotation_deg, 0);
        assert!(i1.mirror);
    }

    #[test]
    fn invalidation_detects_hash_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.routes.lock");

        let lockfile = CompactLockfileBinary {
            version: 1,
            board_name: "test".into(),
            placement_hash: [0xAAu8; 32],
            arcs: vec![],
            instances: vec![],
        };

        write_lockfile(&lockfile, &path).expect("write");
        let loaded = load_lockfile(&path).expect("load");

        // Same hash → valid
        assert!(is_valid(&loaded, &[0xAAu8; 32]));

        // Different hash → invalid
        assert!(!is_valid(&loaded, &[0xBBu8; 32]));
    }

    #[test]
    fn inspect_lockfile_outputs_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("inspect.routes.lock");

        let lockfile = CompactLockfileBinary {
            version: 1,
            board_name: "inspect_board".into(),
            placement_hash: [0x01u8; 32],
            arcs: vec![ArchivedArcSegment {
                net_id: 5,
                layer: 2,
                width_nm: 200_000,
                x1: 100,
                y1: 200,
                z1: 50_000,
                x2: 300,
                y2: 400,
                z2: 50_000,
                thickness_nm: 35_000,
                material_name: "Copper".to_string(),
                current_ma: 20_000,
            }],
            instances: vec![ArchivedComponentInstance {
                id: 10,
                x_nm: 500,
                y_nm: 600,
                rotation_deg: 180,
                mirror: true,
            }],
        };

        write_lockfile(&lockfile, &path).expect("write");
        let json_str = inspect_lockfile(&path).expect("inspect");

        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["board_name"], "inspect_board");
        assert_eq!(parsed["arcs"][0]["net_id"], 5);
        assert_eq!(parsed["instances"][0]["mirror"], true);
    }
}
