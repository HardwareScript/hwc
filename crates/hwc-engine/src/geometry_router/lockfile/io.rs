use std::fs;
use std::io;
use std::path::Path;

use super::archived_types::CompactLockfileBinary;

pub struct LockfileData {
    pub(super) _mmap: memmap2::Mmap,
    pub(super) ptr: *const <CompactLockfileBinary as rkyv::Archive>::Archived,
}

unsafe impl Send for LockfileData {}
unsafe impl Sync for LockfileData {}

impl LockfileData {
    #[inline]
    pub fn data(&self) -> &<CompactLockfileBinary as rkyv::Archive>::Archived {
        unsafe { &*self.ptr }
    }
}

pub fn write_lockfile(lockfile: &CompactLockfileBinary, path: &Path) -> io::Result<()> {
    let bytes: rkyv::AlignedVec = rkyv::to_bytes::<_, 1_048_576>(lockfile)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("rkyv serialize: {e}")))?;

    fs::write(path, bytes.as_slice())
}

pub fn load_lockfile(path: &Path) -> io::Result<LockfileData> {
    let file = fs::File::open(path)?;
    let mmap =
        unsafe { memmap2::Mmap::map(&file).map_err(|e| io::Error::other(format!("mmap: {e}")))? };

    let archived = rkyv::validation::validators::check_archived_root::<CompactLockfileBinary>(
        &mmap,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("rkyv validation: {e}")))?;

    let ptr: *const _ = archived;
    Ok(LockfileData { _mmap: mmap, ptr })
}

#[inline]
pub fn is_valid(loaded: &LockfileData, current_fingerprint: &[u8; 32]) -> bool {
    loaded.data().placement_hash == *current_fingerprint
}

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
