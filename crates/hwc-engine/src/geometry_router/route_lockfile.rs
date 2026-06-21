//! AVS (Alphanumeric Vector Stream) Lock System
//!
//! **Architecture Reference:** Docs/v0.1.7/ROUTING-LOCK-SYSTEM-SPEC.md
//!
//! High-density, deterministic routing storage format for the Hardware Script compiler.
//! Resolved A* paths are saved into `project.routes.lock` using three compression pillars:
//! 1. Topology-Sharing (Shared Arcs) — deduplicate parallel buses
//! 2. Base-36 Command-Value RLC — eliminate coordinates and spaces
//! 3. Columnar Flat Allocation — single-allocation heap buffer

use crate::geometry::Point3D;
use crate::netlist::NetId;
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Core Data Model
// ---------------------------------------------------------------------------

/// Compact lockfile schema (v0.1.7).
///
/// Replaces the legacy `RouteLockfile` with an AVS-encoded format:
/// - `arcs`: Base-36 RLC directional templates shared across parallel nets
/// - `instances`: Flat i32 array, 5 elements per route (net_id, arc_idx, start_x, start_y, start_z)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactLockfile {
    /// Lockfile format version — must be exactly `"0.1.7"`.
    pub version: CompactString,
    /// Board / space name.
    pub board: CompactString,
    /// Deterministic hash of all component placements, orientations, and physical parameters.
    pub placement_hash: CompactString,
    /// Pre-calculated Base-36 RLC path templates. Each string encodes a directional
    /// sequence shared by one or more nets.
    pub arcs: Vec<CompactString>,
    /// Flat list of routing instances. Each instance is 5 consecutive i32 values:
    /// `[net_id, arc_idx, start_x_nm, start_y_nm, start_z_nm]`.
    pub instances: Vec<i32>,
}

/// The required lockfile version for v0.1.7.
pub const LOCKFILE_VERSION: &str = "0.1.7";

// ---------------------------------------------------------------------------
// Arc Encoder — AnalyticTrace / waypoints → Base-36 RLC arc strings
// ---------------------------------------------------------------------------

/// Encode a sequence of waypoints into a Base-36 RLC arc string.
///
/// Walks waypoints in pairs, computes directional deltas (dx, dy, dz),
/// coalesces collinear consecutive segments, and emits direction chars
/// (`R`/`L`/`U`/`D`) followed by a Base-36 magnitude.
///
/// # Example
/// ```text
/// waypoints: [[0,0,0], [2000000,0,0], [2000000,1500000,0]]
/// arc:       "RkUf"
/// ```
pub fn encode_arc(waypoints: &[Point3D]) -> CompactString {
    if waypoints.len() < 2 {
        return CompactString::default();
    }

    let mut arc = String::new();

    // Walk pairs and coalesce collinear segments
    let mut i = 0;
    while i < waypoints.len() - 1 {
        let start = waypoints[i];
        let seg_end = waypoints[i + 1];
        let dx = seg_end.x - start.x;
        let dy = seg_end.y - start.y;
        let dz = seg_end.z - start.z;

        // Determine primary axis of movement
        let (dir, mag) = if dx != 0 {
            let dir = if dx > 0 { 'R' } else { 'L' };
            (dir, dx.abs())
        } else if dy != 0 {
            let dir = if dy > 0 { 'U' } else { 'D' };
            (dir, dy.abs())
        } else if dz != 0 {
            let dir = if dz > 0 { 'U' } else { 'D' };
            (dir, dz.abs())
        } else {
            i += 1;
            continue;
        };

        // Coalesce collinear consecutive segments
        let mut coalesced_mag = mag;
        let mut j = i + 1;
        while j < waypoints.len() - 1 {
            let next_start = waypoints[j];
            let next_end = waypoints[j + 1];
            let ndx = next_end.x - next_start.x;
            let ndy = next_end.y - next_start.y;
            let ndz = next_end.z - next_start.z;

            let next_dir = if ndx != 0 {
                if ndx > 0 { 'R' } else { 'L' }
            } else if ndy != 0 {
                if ndy > 0 { 'U' } else { 'D' }
            } else if ndz != 0 {
                if ndz > 0 { 'U' } else { 'D' }
            } else {
                break;
            };

            if next_dir != dir {
                break;
            }

            coalesced_mag += if ndx != 0 { ndx.abs() } else if ndy != 0 { ndy.abs() } else { ndz.abs() };
            j += 1;
        }

        arc.push(dir);
        encode_base36(coalesced_mag, &mut arc);

        i = j;
    }

    arc.into()
}

/// Encode a decimal value as Base-36 into the output string.
fn encode_base36(mut value: i64, out: &mut String) {
    if value == 0 {
        out.push('0');
        return;
    }

    let mut buf = [0u8; 12];
    let mut pos = buf.len();

    while value > 0 {
        pos -= 1;
        let digit = (value % 36) as u8;
        buf[pos] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + (digit - 10)
        };
        value /= 36;
    }

    out.push_str(std::str::from_utf8(&buf[pos..]).unwrap());
}

/// Encode analytic traces into the compact instances array.
///
/// Each instance occupies 5 i32 slots: `[net_id, arc_idx, start_x, start_y, start_z]`.
/// Shared arc templates are deduplicated into the `arcs` vector.
pub fn encode_instances(
    traces: &[crate::space::AnalyticTrace],
) -> (Vec<CompactString>, Vec<i32>) {
    let mut arcs: Vec<CompactString> = Vec::new();
    let mut arc_index: FxHashMap<CompactString, usize> = FxHashMap::default();
    let mut instances: Vec<i32> = Vec::new();

    for trace in traces {
        // Extract ordered waypoints from segments
        let mut waypoints: Vec<Point3D> = Vec::new();
        for seg in &trace.segments {
            if waypoints.is_empty() {
                waypoints.push(seg.start);
            }
            waypoints.push(seg.end);
        }

        if waypoints.len() < 2 {
            continue;
        }

        let arc_str = encode_arc(&waypoints);

        // Get or insert arc template
        let idx = *arc_index.entry(arc_str.clone()).or_insert_with(|| {
            let new_idx = arcs.len();
            arcs.push(arc_str.clone());
            new_idx
        });

        // Encode instance: [net_id, arc_idx, start_x, start_y, start_z]
        let start = waypoints[0];
        instances.push(trace.net_id.raw() as i32);
        instances.push(idx as i32);
        instances.push(start.x as i32);
        instances.push(start.y as i32);
        instances.push(start.z as i32);
    }

    (arcs, instances)
}

// ---------------------------------------------------------------------------
// Arc Decoder — Base-36 RLC arc strings → Point3D waypoints
// ---------------------------------------------------------------------------

/// Decode a Base-36 RLC arc string into absolute 3D waypoints.
///
/// Characters `R`/`L`/`U`/`D` are direction commands; alphanumeric characters
/// (`0`-`9`, `a`-`z`) accumulate into a Base-36 magnitude.
pub fn decode_arc(arc: &str, start: Point3D) -> Vec<Point3D> {
    let mut points = vec![start];
    let mut pos = start;
    let mut magnitude: i64 = 0;
    let mut has_magnitude = false;
    let mut prev_dir = 'R';

    for ch in arc.chars() {
        match ch {
            'R' | 'L' | 'U' | 'D' => {
                if has_magnitude {
                    pos = apply_direction(pos, prev_dir, magnitude);
                    points.push(pos);
                    magnitude = 0;
                    has_magnitude = false;
                }
                prev_dir = ch;
            }
            '0'..='9' => {
                magnitude = magnitude * 36 + (ch as i64 - '0' as i64);
                has_magnitude = true;
            }
            'a'..='z' => {
                magnitude = magnitude * 36 + (ch as i64 - 'a' as i64 + 10);
                has_magnitude = true;
            }
            _ => {}
        }
    }
    if has_magnitude {
        pos = apply_direction(pos, prev_dir, magnitude);
        points.push(pos);
    }
    points
}

/// Apply a directional delta to a point.
fn apply_direction(p: Point3D, dir: char, mag: i64) -> Point3D {
    match dir {
        'R' => Point3D::new(p.x + mag, p.y, p.z),
        'L' => Point3D::new(p.x - mag, p.y, p.z),
        'U' => Point3D::new(p.x, p.y + mag, p.z),
        'D' => Point3D::new(p.x, p.y - mag, p.z),
        _ => p,
    }
}

/// Decode all instances from a `CompactLockfile` into per-net waypoint vectors.
///
/// Iterates the flat `instances` array in chunks of 5, resolves each arc
/// reference, and returns a map of `NetId` → waypoints.
pub fn decode_instances(
    compact: &CompactLockfile,
) -> FxHashMap<NetId, Vec<Point3D>> {
    let mut result: FxHashMap<NetId, Vec<Point3D>> = FxHashMap::default();

    for chunk in compact.instances.chunks(5) {
        if chunk.len() < 5 {
            continue;
        }
        let net_id = NetId::new(chunk[0] as u32);
        let arc_idx = chunk[1] as usize;
        let start = Point3D::new(chunk[2] as i64, chunk[3] as i64, chunk[4] as i64);

        if arc_idx >= compact.arcs.len() {
            continue;
        }

        let waypoints = decode_arc(compact.arcs[arc_idx].as_str(), start);
        result.insert(net_id, waypoints);
    }

    result
}

// ---------------------------------------------------------------------------
// Placement Hash — Deterministic hash of component placements
// ---------------------------------------------------------------------------

/// Compute a deterministic hash of all component placements, orientations,
/// and physical parameters in a `HardwareSpace`.
///
/// Uses `DefaultHasher` for deterministic output. Hashes component names,
/// bounding boxes (min/max XYZ), grid dimensions, and netlist structure.
/// Does NOT hash `analytic_routes` (those are derived outputs, not inputs).
pub fn compute_placement_hash(space: &crate::space::HardwareSpace) -> CompactString {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash component bounding boxes (sorted by name for determinism)
    let mut components: Vec<_> = space
        .component_bboxes
        .iter()
        .collect();
    components.sort_by_key(|(name, _)| name.as_str());

    for (name, bbox) in &components {
        name.hash(&mut hasher);
        bbox.min.x.hash(&mut hasher);
        bbox.min.y.hash(&mut hasher);
        bbox.min.z.hash(&mut hasher);
        bbox.max.x.hash(&mut hasher);
        bbox.max.y.hash(&mut hasher);
        bbox.max.z.hash(&mut hasher);
    }

    // Hash grid dimensions and voxel size
    space.grid.x_cols.hash(&mut hasher);
    space.grid.y_rows.hash(&mut hasher);
    space.grid.z_layers.hash(&mut hasher);
    space.voxel_size.x_nm.hash(&mut hasher);
    space.voxel_size.y_nm.hash(&mut hasher);
    space.voxel_size.z_nm.hash(&mut hasher);

    // Hash netlist structure (detects net/route additions and removals)
    // Use net count — this reflects the source declarations, not derived analytic routes.
    let net_count = space.netlist.num_nets();
    net_count.hash(&mut hasher);

    format!("{:016x}", hasher.finish()).into()
}

// ---------------------------------------------------------------------------
// Legacy Lockfile Rejection
// ---------------------------------------------------------------------------

/// Error returned when a legacy or corrupt lockfile is detected.
#[derive(Debug)]
pub enum LockfileError {
    /// Lockfile version is not `"0.1.7"`.
    ObsoleteVersion(String),
    /// JSON deserialization failed.
    ParseError(String),
    /// IO error reading the file.
    IoError(std::io::Error),
}

impl std::fmt::Display for LockfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockfileError::ObsoleteVersion(ver) => write!(
                f,
                "[LOCK] Obsolete lockfile detected (version {}). Delete the .routes.lock file and rebuild.",
                ver
            ),
            LockfileError::ParseError(e) => write!(
                f,
                "[LOCK] Failed to parse lockfile: {}. Delete the .routes.lock file and rebuild.",
                e
            ),
            LockfileError::IoError(e) => write!(f, "[LOCK] IO error: {}", e),
        }
    }
}

impl std::error::Error for LockfileError {}

// ---------------------------------------------------------------------------
// CompactLockfile — Load / Save / Validate
// ---------------------------------------------------------------------------

impl CompactLockfile {
    /// Load a lockfile from disk with strict version checking.
    ///
    /// Returns `Ok(CompactLockfile)` only if the file exists, parses correctly,
    /// and has version `"0.1.7"`. Returns a descriptive error otherwise.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, LockfileError> {
        let contents = fs::read_to_string(path).map_err(LockfileError::IoError)?;
        let lock: CompactLockfile =
            serde_json::from_str(&contents).map_err(|e| LockfileError::ParseError(e.to_string()))?;

        if lock.version.as_str() != LOCKFILE_VERSION {
            return Err(LockfileError::ObsoleteVersion(lock.version.to_string()));
        }

        Ok(lock)
    }

    /// Save the lockfile to disk.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
    }

    /// Validate that the stored placement hash matches the current layout.
    pub fn validate_placement(&self, current_hash: &str) -> bool {
        self.placement_hash.as_str() == current_hash
    }

    /// Convert this compact lockfile into per-net waypoint vectors.
    pub fn decode(&self) -> FxHashMap<NetId, Vec<Point3D>> {
        decode_instances(self)
    }

    /// Convert decoded waypoints into `AnalyticTrace` objects.
    pub fn to_analytic_traces(
        &self,
        material_id: crate::material::MaterialId,
        netlist: &crate::netlist::NetlistArena,
    ) -> Vec<crate::space::AnalyticTrace> {
        let per_net = decode_instances(self);
        let mut traces = Vec::new();

        for (net_id, waypoints) in &per_net {
            if waypoints.len() < 2 {
                continue;
            }

            let segments: Vec<crate::space::LineSegment> = waypoints
                .windows(2)
                .map(|w| {
                    crate::space::LineSegment::new(w[0], w[1])
                })
                .collect();

            // Resolve net name from netlist
            let net_name = netlist
                .get_net(*net_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("net_{}", net_id.raw()).into());

            traces.push(crate::space::AnalyticTrace::new(
                *net_id,
                100_000, // default trace width
                35_000,  // default thickness
                segments,
                material_id,
                net_name,
            ));
        }

        traces
    }
}

// ---------------------------------------------------------------------------
// LockfileManager — Selective rerouting manager
// ---------------------------------------------------------------------------

/// Route lockfile manager for selective rerouting.
pub struct LockfileManager {
    /// Current lockfile (if loaded).
    lockfile: Option<CompactLockfile>,
    /// Routes that need rerouting.
    invalidated_routes: FxHashMap<NetId, String>,
}

impl LockfileManager {
    /// Create a new lockfile manager.
    pub fn new(lockfile: Option<CompactLockfile>) -> Self {
        Self {
            lockfile,
            invalidated_routes: FxHashMap::default(),
        }
    }

    /// Check if a route is locked and valid.
    pub fn get_locked_route(
        &self,
        net_id: NetId,
        _start: Point3D,
        _end: Point3D,
    ) -> Option<Vec<Point3D>> {
        let lockfile = self.lockfile.as_ref()?;
        let per_net = decode_instances(lockfile);
        let waypoints = per_net.get(&net_id)?;

        // Check if route was invalidated
        if self.invalidated_routes.contains_key(&net_id) {
            return None;
        }

        Some(waypoints.clone())
    }

    /// Invalidate a route (mark for rerouting).
    pub fn invalidate_route(&mut self, net_id: NetId, reason: String) {
        self.invalidated_routes.insert(net_id, reason);
    }

    /// Get all invalidated routes.
    pub fn get_invalidated_routes(&self) -> &FxHashMap<NetId, String> {
        &self.invalidated_routes
    }

    /// Check if any routes were invalidated.
    pub fn has_invalidated_routes(&self) -> bool {
        !self.invalidated_routes.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Legacy re-exports for backward compatibility
// ---------------------------------------------------------------------------

/// Legacy type alias — use `CompactLockfile` for new code.
pub type RouteLockfile = CompactLockfile;
