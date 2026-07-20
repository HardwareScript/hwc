/// A single arc segment stored in the lockfile.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug)]
#[archive(check_bytes)]
pub struct ArchivedArcSegment {
    pub net_id: u32,
    pub layer: u16,
    pub width_nm: i64,
    pub x1: i64,
    pub y1: i64,
    pub z1: i64,
    pub x2: i64,
    pub y2: i64,
    pub z2: i64,
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
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug)]
#[archive(check_bytes)]
pub struct CompactLockfileBinary {
    pub version: u32,
    pub board_name: String,
    pub placement_hash: [u8; 32],
    pub arcs: Vec<ArchivedArcSegment>,
    pub instances: Vec<ArchivedComponentInstance>,
}
