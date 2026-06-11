use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, Result};

const BENCHMARK_FILE: &str = "benchmark-report.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchReport {
    pub root: Option<PathBuf>,
    pub rows: Vec<BenchRow>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchRow {
    pub name: String,
    pub elapsed_ms: u128,
    pub count: usize,
}

impl BenchReport {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            rows: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn push_warning_rows(&mut self, warn_ms: Option<u128>) {
        let Some(warn_ms) = warn_ms else {
            return;
        };
        self.warnings.extend(
            self.rows
                .iter()
                .filter(|row| row.elapsed_ms > warn_ms)
                .map(|row| format!("{} took {}ms, above {}ms", row.name, row.elapsed_ms, warn_ms)),
        );
    }
}

pub fn run_all(
    root: &Path,
    query: &str,
    limit: usize,
    file_options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<BenchReport> {
    let mut report = BenchReport::new(Some(root.to_path_buf()));
    report.rows.extend(index_rows(
        root,
        false,
        limit,
        query,
        file_options,
        default_ignore,
        ignore_file,
    )?);
    report
        .rows
        .extend(search_rows(root, query, file_options, default_ignore, ignore_file)?);
    report.rows.extend(trace_rows()?);
    report.rows.extend(plugin_rows(root)?);
    write_report(root, &report)?;
    Ok(report)
}

pub fn run_search(
    root: &Path,
    pattern: &str,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<BenchReport> {
    let mut report = BenchReport::new(Some(root.to_path_buf()));
    report
        .rows
        .extend(search_rows(root, pattern, options, default_ignore, ignore_file)?);
    write_report(root, &report)?;
    Ok(report)
}

pub fn run_index(
    root: &Path,
    include_build: bool,
    limit: usize,
    query: &str,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<BenchReport> {
    let mut report = BenchReport::new(Some(root.to_path_buf()));
    report.rows.extend(index_rows(
        root,
        include_build,
        limit,
        query,
        options,
        default_ignore,
        ignore_file,
    )?);
    write_report(root, &report)?;
    Ok(report)
}

pub fn run_trace() -> Result<BenchReport> {
    let mut report = BenchReport::new(None);
    report.rows.extend(trace_rows()?);
    Ok(report)
}

pub fn run_preview_read(path: &Path) -> Result<BenchReport> {
    let mut report = BenchReport::new(None);
    let start = Instant::now();
    let contents = fs::read_to_string(path)?;
    report.rows.push(BenchRow {
        name: "preview_read".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: contents.lines().count(),
    });
    Ok(report)
}

pub fn format_report(report: &BenchReport, format: &str) -> Result<String> {
    match format {
        "text" => {
            let mut output = String::new();
            if let Some(root) = &report.root {
                output.push_str(&format!("Root: {}\n", root.display()));
            }
            for row in &report.rows {
                output.push_str(&format!("{}\t{}\t{}\n", row.name, row.elapsed_ms, row.count));
            }
            for warning in &report.warnings {
                output.push_str(&format!("warning: {warning}\n"));
            }
            Ok(output)
        }
        "json" => serde_json::to_string_pretty(report)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|err| AppError::General(err.to_string())),
        other => Err(AppError::General(format!("Unsupported benchmark format: {other}"))),
    }
}

fn index_rows(
    root: &Path,
    include_build: bool,
    limit: usize,
    query: &str,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<Vec<BenchRow>> {
    let mut rows = Vec::new();

    let start = Instant::now();
    let status = crate::index::status(root)?;
    rows.push(BenchRow {
        name: "index_status".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: status.file_count + status.symbol_count,
    });

    if include_build {
        let start = Instant::now();
        let report = crate::index::build(root, options, default_ignore, ignore_file)?;
        rows.push(BenchRow {
            name: "index_build".to_string(),
            elapsed_ms: start.elapsed().as_millis(),
            count: report.file_count + report.symbol_count,
        });
    }

    let start = Instant::now();
    let index = crate::index::load(root)?;
    rows.push(BenchRow {
        name: "index_load".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: index
            .as_ref()
            .map_or(0, |index| index.files.len() + index.symbols.len()),
    });

    let start = Instant::now();
    let files = crate::index::list(root, crate::index::IndexListKind::Files, limit)?;
    rows.push(BenchRow {
        name: "index_list_files".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: files.len(),
    });

    let start = Instant::now();
    let symbols = crate::index::query(root, crate::index::IndexListKind::Symbols, query, limit)?;
    rows.push(BenchRow {
        name: "index_query_symbols".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: symbols.len(),
    });

    Ok(rows)
}

fn search_rows(
    root: &Path,
    pattern: &str,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<Vec<BenchRow>> {
    let root_arg = root.to_string_lossy().to_string();
    let start = Instant::now();
    let results = crate::search::search(pattern, Some(&root_arg), options, default_ignore, ignore_file)?;
    Ok(vec![BenchRow {
        name: "search".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: results.flat().len(),
    }])
}

fn trace_rows() -> Result<Vec<BenchRow>> {
    let start = Instant::now();
    let entries = crate::trace::list()?;
    let elapsed_ms = start.elapsed().as_millis();
    let start = Instant::now();
    let sessions = crate::trace::list_sessions(true)?;
    Ok(vec![
        BenchRow {
            name: "trace_list".to_string(),
            elapsed_ms,
            count: entries.len(),
        },
        BenchRow {
            name: "trace_sessions".to_string(),
            elapsed_ms: start.elapsed().as_millis(),
            count: sessions.len(),
        },
    ])
}

fn plugin_rows(root: &Path) -> Result<Vec<BenchRow>> {
    let start = Instant::now();
    let diagnostics = crate::plugins::doctor(Some(root))?;
    Ok(vec![BenchRow {
        name: "plugin_doctor".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: diagnostics.len(),
    }])
}

fn write_report(root: &Path, report: &BenchReport) -> Result<()> {
    let path = crate::workspace::cache_dir_for_root(root)?.join(BENCHMARK_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(report).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_report_formats_text() {
        let mut report = BenchReport::new(Some(PathBuf::from("/tmp/fcs")));
        report.rows.push(BenchRow {
            name: "index_status".to_string(),
            elapsed_ms: 1,
            count: 2,
        });

        let output = format_report(&report, "text").unwrap();

        assert!(output.contains("Root: /tmp/fcs"));
        assert!(output.contains("index_status\t1\t2"));
    }
}
