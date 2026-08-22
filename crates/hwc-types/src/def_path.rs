//! Canonical Hierarchical Definition Path (DefPath)
//!
//! Modeled after rustc's DefPath / DefId architecture.
//! Preserves parent -> child -> grandchild lexical and spatial scope boundaries,
//! preventing symbol and instance collisions in complex multi-file, multi-layer ASIC/PCB designs.

use compact_str::CompactString;
use smallvec::SmallVec;
use std::fmt;

/// Canonical Hierarchical Definition Path
///
/// Example: `CMOS_Inverter_Space::PMOS_Inst::M1::Gate_Poly`
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct DefPath {
    pub segments: SmallVec<[CompactString; 4]>,
}

impl DefPath {
    /// Create a new DefPath from a root scope name
    #[inline]
    pub fn root(root_name: impl Into<CompactString>) -> Self {
        let mut segments = SmallVec::new();
        segments.push(root_name.into());
        Self { segments }
    }

    /// Create an empty DefPath
    #[inline]
    pub fn empty() -> Self {
        Self {
            segments: SmallVec::new(),
        }
    }

    /// Create a DefPath from a list of segments
    pub fn from_segments<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<CompactString>,
    {
        Self {
            segments: iter.into_iter().map(Into::into).collect(),
        }
    }

    /// Parse a `::` or `.` delimited string into a DefPath
    pub fn parse(path_str: &str) -> Self {
        let delimiter = if path_str.contains("::") { "::" } else { "." };
        let segments: SmallVec<[CompactString; 4]> = path_str
            .split(delimiter)
            .filter(|s| !s.is_empty())
            .map(CompactString::from)
            .collect();
        Self { segments }
    }

    /// Push a child segment to the path, returning a new DefPath
    pub fn push(&self, child_name: impl Into<CompactString>) -> Self {
        let mut new_segments = self.segments.clone();
        new_segments.push(child_name.into());
        Self {
            segments: new_segments,
        }
    }

    /// Push a child segment in-place
    #[inline]
    pub fn push_mut(&mut self, child_name: impl Into<CompactString>) {
        self.segments.push(child_name.into());
    }

    /// Return parent DefPath if available
    pub fn parent(&self) -> Option<Self> {
        if self.segments.len() <= 1 {
            None
        } else {
            let mut parent_segments = self.segments.clone();
            parent_segments.pop();
            Some(Self {
                segments: parent_segments,
            })
        }
    }

    /// Return leaf segment name (the last identifier in the path)
    #[inline]
    pub fn leaf(&self) -> Option<&str> {
        self.segments.last().map(|s| s.as_str())
    }

    /// Return root segment name (the first identifier in the path)
    #[inline]
    pub fn root_segment(&self) -> Option<&str> {
        self.segments.first().map(|s| s.as_str())
    }

    /// Check if the path is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Number of path segments
    #[inline]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Get canonical string representation with `::`
    pub fn to_string(&self) -> String {
        self.segments.join("::")
    }

    /// Get dot-separated string representation for netlist / spice export compatibility
    pub fn to_dotted_string(&self) -> String {
        self.segments.join(".")
    }
}

impl Default for DefPath {
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for DefPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DefPath({})", self.to_string())
    }
}

impl fmt::Display for DefPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl From<&str> for DefPath {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for DefPath {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<CompactString> for DefPath {
    fn from(s: CompactString) -> Self {
        Self::parse(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_def_path_creation() {
        let root = DefPath::root("CMOS_Inverter_Space");
        assert_eq!(root.to_string(), "CMOS_Inverter_Space");
        assert_eq!(root.leaf(), Some("CMOS_Inverter_Space"));
        assert_eq!(root.parent(), None);

        let pmos = root.push("PMOS_Inst");
        assert_eq!(pmos.to_string(), "CMOS_Inverter_Space::PMOS_Inst");
        assert_eq!(pmos.leaf(), Some("PMOS_Inst"));

        let m1 = pmos.push("M1");
        assert_eq!(m1.to_string(), "CMOS_Inverter_Space::PMOS_Inst::M1");
        assert_eq!(m1.leaf(), Some("M1"));
        assert_eq!(m1.parent(), Some(pmos));
    }

    #[test]
    fn test_def_path_parse() {
        let path1 = DefPath::parse("A::B::C");
        assert_eq!(path1.len(), 3);
        assert_eq!(path1.to_string(), "A::B::C");

        let path2 = DefPath::parse("A.B.C");
        assert_eq!(path2.len(), 3);
        assert_eq!(path2.to_string(), "A::B::C");
    }
}
