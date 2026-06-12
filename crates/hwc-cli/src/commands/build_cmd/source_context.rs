/// Source code context extraction for error reporting
/// Task 5.4: Code snippet extraction with line numbers
///
/// This module provides utilities for extracting source code snippets
/// to display in error messages. Currently used for future integration
/// when source location tracking is available throughout the pipeline.

use std::fs;
use std::path::Path;

/// Extract a code snippet from a source file with line numbers
/// 
/// # Arguments
/// * `file_path` - Path to the source file
/// * `line_number` - Target line number (1-indexed)
/// * `context_lines` - Number of lines to show before and after the target line
/// 
/// # Returns
/// A formatted string with line numbers and code, or None if file cannot be read
#[allow(dead_code)]
pub fn extract_snippet(
    file_path: &Path,
    line_number: usize,
    context_lines: usize,
) -> Option<String> {
    let content = fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    
    if line_number == 0 || line_number > lines.len() {
        return None;
    }
    
    let start_line = line_number.saturating_sub(context_lines).max(1);
    let end_line = (line_number + context_lines).min(lines.len());
    
    let mut snippet = String::new();
    snippet.push_str(&format!("\n📄 {}:{}\n", file_path.display(), line_number));
    snippet.push_str("─────────────────────────────────────────────────────────\n");
    
    for (idx, line) in lines.iter().enumerate().skip(start_line - 1).take(end_line - start_line + 1) {
        let line_num = idx + 1;
        let marker = if line_num == line_number { ">" } else { " " };
        snippet.push_str(&format!("{} {:4} │ {}\n", marker, line_num, line));
    }
    
    snippet.push_str("─────────────────────────────────────────────────────────\n");
    
    Some(snippet)
}

/// Extract multiple code snippets from a source file
/// 
/// # Arguments
/// * `file_path` - Path to the source file
/// * `line_numbers` - List of target line numbers (1-indexed)
/// * `context_lines` - Number of lines to show before and after each target line
/// 
/// # Returns
/// A formatted string with all snippets, or None if file cannot be read
#[allow(dead_code)]
pub fn extract_multiple_snippets(
    file_path: &Path,
    line_numbers: &[usize],
    context_lines: usize,
) -> Option<String> {
    let content = fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    
    let mut snippet = String::new();
    snippet.push_str(&format!("\n📄 {}\n", file_path.display()));
    snippet.push_str("─────────────────────────────────────────────────────────\n");
    
    for &line_number in line_numbers {
        if line_number == 0 || line_number > lines.len() {
            continue;
        }
        
        let start_line = line_number.saturating_sub(context_lines).max(1);
        let end_line = (line_number + context_lines).min(lines.len());
        
        for (idx, line) in lines.iter().enumerate().skip(start_line - 1).take(end_line - start_line + 1) {
            let line_num = idx + 1;
            let marker = if line_num == line_number { ">" } else { " " };
            snippet.push_str(&format!("{} {:4} │ {}\n", marker, line_num, line));
        }
        
        snippet.push_str("─────────────────────────────────────────────────────────\n");
    }
    
    Some(snippet)
}

/// Convert byte offset to line and column numbers
/// 
/// # Arguments
/// * `content` - The source file content
/// * `byte_offset` - Byte offset in the file (0-indexed)
/// 
/// # Returns
/// (line_number, column_number) both 1-indexed, or None if offset is invalid
#[allow(dead_code)]
pub fn byte_offset_to_line_col(content: &str, byte_offset: usize) -> Option<(usize, usize)> {
    if byte_offset > content.len() {
        return None;
    }
    
    let mut line = 1;
    let mut col = 1;
    
    for (idx, ch) in content.chars().enumerate() {
        if idx >= byte_offset {
            break;
        }
        
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    
    Some((line, col))
}

/// Format a code snippet with syntax highlighting markers
/// 
/// # Arguments
/// * `code` - The code snippet to format
/// * `highlight_ranges` - List of (start_col, end_col) ranges to highlight (1-indexed)
/// 
/// # Returns
/// Formatted code with highlight markers (^^^) under highlighted ranges
#[allow(dead_code)]
pub fn format_with_highlights(code: &str, highlight_ranges: &[(usize, usize)]) -> String {
    let mut result = String::new();
    result.push_str(code);
    result.push('\n');
    
    if !highlight_ranges.is_empty() {
        // Create highlight line with ^^^ markers
        let max_col = highlight_ranges.iter().map(|(_, end)| *end).max().unwrap_or(0);
        let mut highlight_line = vec![' '; max_col];
        
        for &(start, end) in highlight_ranges {
            for i in (start.saturating_sub(1))..end.min(max_col) {
                highlight_line[i] = '^';
            }
        }
        
        result.push_str("     │ ");
        result.push_str(&highlight_line.iter().collect::<String>());
        result.push('\n');
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_byte_offset_to_line_col() {
        let content = "line 1\nline 2\nline 3";
        
        // Start of file
        assert_eq!(byte_offset_to_line_col(content, 0), Some((1, 1)));
        
        // End of first line
        assert_eq!(byte_offset_to_line_col(content, 6), Some((1, 7)));
        
        // Start of second line (after newline)
        assert_eq!(byte_offset_to_line_col(content, 7), Some((2, 1)));
        
        // Middle of second line
        assert_eq!(byte_offset_to_line_col(content, 10), Some((2, 4)));
    }
    
    #[test]
    fn test_format_with_highlights() {
        let code = "add component at [x: 10mm, y: 20mm, z: 4]";
        let highlights = vec![(40, 41)]; // Highlight the "4"
        
        let formatted = format_with_highlights(code, &highlights);
        assert!(formatted.contains("^^^") || formatted.contains("^"));
    }
}
