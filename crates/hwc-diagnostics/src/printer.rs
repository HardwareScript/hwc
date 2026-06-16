use crate::location::SourceMap;
use miette::{Diagnostic, Severity};
use owo_colors::OwoColorize;
use std::fmt::Write;

pub struct DiagnosticPrinter<'a> {
    source: &'a str,
    file_name: &'a str,
    source_map: SourceMap,
}

impl<'a> DiagnosticPrinter<'a> {
    pub fn new(source: &'a str, file_name: &'a str) -> Self {
        Self {
            source,
            file_name,
            source_map: SourceMap::new(source),
        }
    }

    pub fn format_diagnostic(&self, diagnostic: &dyn Diagnostic) -> String {
        let mut out = String::new();

        let severity = diagnostic.severity().unwrap_or(Severity::Error);
        let code = diagnostic
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "E000".to_string());
        let message = diagnostic.to_string();

        // 1. Header: error[E001]: message
        self.render_header(&mut out, severity, &code, &message);

        // 2. Labels and Snippets
        if let Some(labels) = diagnostic.labels() {
            let mut first = true;
            for label in labels {
                let offset = label.offset();
                let loc = self.source_map.get_location(offset);

                if first {
                    // Clickable location header
                    writeln!(
                        out,
                        "  {} {}:{}:{}",
                        "-->".blue().bold(),
                        self.file_name.white().bold(),
                        loc.line,
                        loc.column
                    )
                    .unwrap();
                    writeln!(out, "   {}", "│".blue()).unwrap();
                    first = false;
                }

                // Render the code snippet with context
                self.render_snippet(&mut out, &loc, label.len(), label.label(), severity);
            }
        } else {
            writeln!(out, "   {}", "│".blue()).unwrap();
        }

        // 3. Help / Notes (Cyan for visibility)
        if let Some(help) = diagnostic.help() {
            writeln!(out, "   {}", "│".blue()).unwrap();
            writeln!(out, "   {} {}: {}", "=".cyan(), "help".cyan().bold(), help).unwrap();
        }

        // 4. Related errors (recursive)
        if let Some(related) = diagnostic.related() {
            for rel in related {
                writeln!(out, "\n{}", self.format_diagnostic(rel)).unwrap();
            }
        }

        out
    }

    fn render_header(&self, out: &mut String, severity: Severity, code: &str, message: &str) {
        let (sev_text, code_text) = match severity {
            Severity::Error => (
                "error".red().bold().to_string(),
                format!("[{}]", code).red().bold().to_string(),
            ),
            Severity::Warning => (
                "warning".yellow().bold().to_string(),
                format!("[{}]", code).yellow().bold().to_string(),
            ),
            Severity::Advice => {
                if code.starts_with('W') {
                    (
                        "waiver".yellow().bold().to_string(),
                        format!("[{}]", code).yellow().bold().to_string(),
                    )
                } else {
                    (
                        "note".cyan().bold().to_string(),
                        format!("[{}]", code).cyan().bold().to_string(),
                    )
                }
            }
        };

        writeln!(out, "{}{}: {}", sev_text, code_text, message.bold()).unwrap();
    }

    fn render_snippet(
        &self,
        out: &mut String,
        loc: &crate::location::SourceLocation,
        len: usize,
        label: Option<&str>,
        severity: Severity,
    ) {
        // Color for this severity
        let sev_color = match severity {
            Severity::Error => owo_colors::Style::new().red().bold(),
            Severity::Warning => owo_colors::Style::new().yellow().bold(),
            Severity::Advice => owo_colors::Style::new().cyan().bold(),
        };

        // Show 1 line of context above if possible
        if loc.line > 1 {
            if let Some(prev_range) = self.source_map.get_line_range(loc.line - 1) {
                let prev_text = self.source[prev_range].trim_end_matches(['\r', '\n']);
                writeln!(
                    out,
                    "{:>2} {} {}",
                    (loc.line - 1).dimmed(),
                    "│".blue(),
                    prev_text.dimmed()
                )
                .unwrap();
            }
        }

        // Current line
        let line_range = self.source_map.get_line_range(loc.line).unwrap();
        let line_text = &self.source[line_range.clone()];
        let clean_line = line_text.trim_end_matches(['\r', '\n']);

        writeln!(out, "{:>2} {} {}", loc.line.bold(), "│".blue(), clean_line).unwrap();

        // Underline and label (Unicode Spider Style)
        write!(out, "   {} ", "│".blue()).unwrap();

        // Padding to column
        for _ in 1..loc.column {
            write!(out, " ").unwrap();
        }

        // The Underline itself (Capped at line length)
        let max_len = clean_line.len().saturating_sub(loc.column - 1);
        let actual_len = len.min(max_len).max(1);

        if actual_len <= 1 {
            write!(out, "{}", "▲".style(sev_color)).unwrap();
        } else {
            // multi-character underline
            write!(out, "{}", "─".style(sev_color)).unwrap();
            for _ in 1..actual_len.saturating_sub(1) {
                write!(out, "{}", "─".style(sev_color)).unwrap();
            }
            write!(out, "{}", "┬".style(sev_color)).unwrap();
        };

        if let Some(msg) = label {
            if msg.len() > 20 || actual_len > 1 {
                writeln!(out).unwrap();
                write!(out, "   {} ", "│".blue()).unwrap();
                for _ in 1..loc.column {
                    write!(out, " ").unwrap();
                }
                for _ in 0..actual_len.saturating_sub(1) {
                    write!(out, " ").unwrap();
                }
                write!(out, "{}", format!("╰── {}", msg).style(sev_color)).unwrap();
            } else {
                write!(out, " {}", msg.style(sev_color)).unwrap();
            }
        }
        writeln!(out).unwrap();

        // Show 1 line of context below if possible
        let total_lines = self.source_map.line_count();
        if loc.line < total_lines {
            if let Some(next_range) = self.source_map.get_line_range(loc.line + 1) {
                let next_text = self.source[next_range].trim_end_matches(['\r', '\n']);
                writeln!(
                    out,
                    "{:>2} {} {}",
                    (loc.line + 1).dimmed(),
                    "│".blue(),
                    next_text.dimmed()
                )
                .unwrap();
            }
        }
    }
}
