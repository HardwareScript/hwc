use std::ops::Range;

/// A point in the source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

/// A map of line start offsets in a source string.
/// This allows O(log N) lookup of line/column from byte offset.
#[derive(Debug)]
pub struct SourceMap {
    line_starts: Vec<usize>,
    total_len: usize,
}

impl SourceMap {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        let mut last_was_cr = false;

        for (i, c) in source.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
                last_was_cr = false;
            } else if c == '\r' {
                last_was_cr = true;
            } else if last_was_cr {
                // If we had a CR and the next char isn't LF, it's an old Mac style newline
                line_starts.push(i);
                last_was_cr = false;
            }
        }

        Self {
            line_starts,
            total_len: source.len(),
        }
    }

    /// Get the line and column for a byte offset.
    pub fn get_location(&self, offset: usize) -> SourceLocation {
        let offset = offset.min(self.total_len);
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx - 1,
        };

        SourceLocation {
            line: line_idx + 1,
            column: offset - self.line_starts[line_idx] + 1,
            offset,
        }
    }

    /// Get the byte range for a given line (1-indexed).
    pub fn get_line_range(&self, line: usize) -> Option<Range<usize>> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }

        let start = self.line_starts[line - 1];
        let end = if line < self.line_starts.len() {
            self.line_starts[line]
        } else {
            self.total_len
        };

        Some(start..end)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_map_lf() {
        let source = "line 1\nline 2\nline 3";
        let map = SourceMap::new(source);
        
        let loc1 = map.get_location(0);
        assert_eq!(loc1.line, 1);
        assert_eq!(loc1.column, 1);

        let loc2 = map.get_location(7);
        assert_eq!(loc2.line, 2);
        assert_eq!(loc2.column, 1);
    }

    #[test]
    fn test_source_map_crlf() {
        let source = "line 1\r\nline 2\r\nline 3";
        let map = SourceMap::new(source);
        
        let loc1 = map.get_location(0);
        assert_eq!(loc1.line, 1);
        assert_eq!(loc1.column, 1);

        let loc2 = map.get_location(8);
        assert_eq!(loc2.line, 2);
        assert_eq!(loc2.column, 1);
    }
}
