use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::core::CodeItem;
use crate::errors::{AppError, Result};

const TUI_SOURCE_LIMIT: usize = 2000;

const BENCHMARK_FILE: &str = "benchmark-report.json";
const BENCHMARK_BASELINE_FILE: &str = "benchmark-baseline.json";
const BENCHMARK_HISTORY_FILE: &str = "benchmark-history.json";
const BENCHMARK_HISTORY_LIMIT: usize = 20;
const SLOW_ROW_EXPLAIN_MS: u128 = 500;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchComparison {
    pub root: PathBuf,
    pub baseline_rows: usize,
    pub current_rows: usize,
    pub threshold_ms: u128,
    pub threshold_percent: u128,
    pub regressions: Vec<BenchRegression>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchRegression {
    pub name: String,
    pub baseline_ms: u128,
    pub current_ms: u128,
    pub delta_ms: i128,
    pub delta_percent: i128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BenchHistoryEntry {
    recorded_at_unix: u64,
    rows: Vec<BenchRow>,
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
    report
        .rows
        .extend(tui_source_rows(root, query, file_options, default_ignore, ignore_file)?);
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

pub fn run_tui_sources(
    root: &Path,
    query: &str,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<BenchReport> {
    let mut report = BenchReport::new(Some(root.to_path_buf()));
    report
        .rows
        .extend(tui_source_rows(root, query, options, default_ignore, ignore_file)?);
    write_report(root, &report)?;
    Ok(report)
}

pub fn save_baseline(root: &Path) -> Result<PathBuf> {
    let report = read_report(root)?;
    let path = benchmark_baseline_path(root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(&report).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(&path, contents)?;
    Ok(path)
}

pub fn compare_to_baseline(root: &Path, threshold_ms: u128, threshold_percent: u128) -> Result<BenchComparison> {
    let baseline = read_report_from(&benchmark_baseline_path(root)?)?;
    let current = read_report(root)?;
    let baseline_by_name = baseline
        .rows
        .iter()
        .map(|row| (row.name.clone(), row.elapsed_ms))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut regressions = Vec::new();
    for row in &current.rows {
        let Some(baseline_ms) = baseline_by_name.get(&row.name).copied() else {
            continue;
        };
        let delta_ms = row.elapsed_ms as i128 - baseline_ms as i128;
        let delta_percent = if baseline_ms == 0 {
            if row.elapsed_ms == 0 {
                0
            } else {
                100
            }
        } else {
            ((delta_ms * 100) / baseline_ms as i128).max(0)
        };
        if delta_ms > threshold_ms as i128 || delta_percent > threshold_percent as i128 {
            regressions.push(BenchRegression {
                name: row.name.clone(),
                baseline_ms,
                current_ms: row.elapsed_ms,
                delta_ms,
                delta_percent,
            });
        }
    }
    Ok(BenchComparison {
        root: root.to_path_buf(),
        baseline_rows: baseline.rows.len(),
        current_rows: current.rows.len(),
        threshold_ms,
        threshold_percent,
        regressions,
    })
}

pub fn format_comparison(comparison: &BenchComparison, format: &str) -> Result<String> {
    match format {
        "text" => {
            let mut output = String::new();
            output.push_str(&format!("Root: {}\n", comparison.root.display()));
            output.push_str(&format!(
                "baseline_rows: {} current_rows: {}\n",
                comparison.baseline_rows, comparison.current_rows
            ));
            if comparison.regressions.is_empty() {
                output.push_str("regressions: none\n");
            } else {
                output.push_str("regressions:\n");
                for regression in &comparison.regressions {
                    output.push_str(&format!(
                        "  {} {}ms -> {}ms delta={}ms percent={}%\n",
                        regression.name,
                        regression.baseline_ms,
                        regression.current_ms,
                        regression.delta_ms,
                        regression.delta_percent
                    ));
                }
            }
            Ok(output)
        }
        "json" => serde_json::to_string_pretty(comparison)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|err| AppError::General(err.to_string())),
        other => Err(AppError::General(format!(
            "Unsupported benchmark compare format: {other}"
        ))),
    }
}

fn tui_source_rows(
    root: &Path,
    query: &str,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<Vec<BenchRow>> {
    let mut rows = Vec::new();
    let root_arg = root.to_string_lossy().to_string();

    let start = Instant::now();
    let files = crate::files::find_files(Some(&root_arg), options, default_ignore, ignore_file)?;
    rows.push(BenchRow {
        name: "tui_files_source".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: files.len(),
    });

    let start = Instant::now();
    let symbols =
        match crate::index::query_code_items(root, crate::index::IndexListKind::Symbols, query, TUI_SOURCE_LIMIT)? {
            Some(items) => items,
            None => {
                let symbols = crate::symbols::find_symbols(Some(&root_arg), options, default_ignore, ignore_file)?;
                filter_bench_items(symbols, query)
            }
        };
    rows.push(BenchRow {
        name: "tui_symbols_source".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: symbols.len(),
    });

    let start = Instant::now();
    let trace_entries = crate::trace::list_for_workspace(root)?;
    rows.push(BenchRow {
        name: "tui_trace_source".to_string(),
        elapsed_ms: start.elapsed().as_millis(),
        count: trace_entries.len(),
    });

    Ok(rows)
}

fn filter_bench_items(items: Vec<CodeItem>, query: &str) -> Vec<CodeItem> {
    if query.trim().is_empty() {
        return items;
    }

    items
        .into_iter()
        .filter(|item| item.display_text().contains(query) || item.detail.contains(query))
        .take(TUI_SOURCE_LIMIT)
        .collect()
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
            for explanation in slow_row_explanations(report) {
                output.push_str(&format!("explain: {explanation}\n"));
            }
            if let Some(root) = &report.root {
                for trend in benchmark_trend_lines(root, report)? {
                    output.push_str(&format!("trend: {trend}\n"));
                }
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
    let sessions_elapsed_ms = start.elapsed().as_millis();
    let start = Instant::now();
    let graph = crate::trace::export_graph(None)?;
    Ok(vec![
        BenchRow {
            name: "trace_list".to_string(),
            elapsed_ms,
            count: entries.len(),
        },
        BenchRow {
            name: "trace_sessions".to_string(),
            elapsed_ms: sessions_elapsed_ms,
            count: sessions.len(),
        },
        BenchRow {
            name: "trace_graph".to_string(),
            elapsed_ms: start.elapsed().as_millis(),
            count: graph.lines().count(),
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
    let path = benchmark_report_path(root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(report).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    append_benchmark_history(root, report)?;
    Ok(())
}

fn read_report(root: &Path) -> Result<BenchReport> {
    read_report_from(&benchmark_report_path(root)?)
}

fn read_report_from(path: &Path) -> Result<BenchReport> {
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(|err| AppError::General(err.to_string()))
}

fn benchmark_report_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(BENCHMARK_FILE))
}

fn benchmark_baseline_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(BENCHMARK_BASELINE_FILE))
}

fn benchmark_history_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(BENCHMARK_HISTORY_FILE))
}

fn append_benchmark_history(root: &Path, report: &BenchReport) -> Result<()> {
    let path = benchmark_history_path(root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut history = read_benchmark_history(root)?;
    history.push(BenchHistoryEntry {
        recorded_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0),
        rows: report.rows.clone(),
    });
    if history.len() > BENCHMARK_HISTORY_LIMIT {
        let overflow = history.len() - BENCHMARK_HISTORY_LIMIT;
        history.drain(0..overflow);
    }
    let contents = serde_json::to_string_pretty(&history).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn read_benchmark_history(root: &Path) -> Result<Vec<BenchHistoryEntry>> {
    let path = benchmark_history_path(root)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(|err| AppError::General(format!("Corrupt benchmark history: {err}")))
}

fn benchmark_trend_lines(root: &Path, report: &BenchReport) -> Result<Vec<String>> {
    let history = read_benchmark_history(root)?;
    if history.len() < 2 {
        return Ok(Vec::new());
    }

    let mut lines = Vec::new();
    for row in &report.rows {
        let previous = history.iter().rev().skip(1).find_map(|entry| {
            entry
                .rows
                .iter()
                .find(|candidate| candidate.name == row.name)
                .map(|candidate| candidate.elapsed_ms)
        });
        let Some(previous_ms) = previous else {
            continue;
        };
        let delta = row.elapsed_ms as i128 - previous_ms as i128;
        if delta == 0 {
            continue;
        }
        let direction = if delta > 0 { "slower" } else { "faster" };
        lines.push(format!(
            "{} {}ms -> {}ms ({} {}ms)",
            row.name,
            previous_ms,
            row.elapsed_ms,
            direction,
            delta.unsigned_abs()
        ));
    }
    Ok(lines)
}

fn slow_row_explanations(report: &BenchReport) -> Vec<String> {
    report
        .rows
        .iter()
        .filter(|row| row.elapsed_ms >= SLOW_ROW_EXPLAIN_MS)
        .map(|row| {
            let hint = if row.name.contains("search") {
                "check ignore patterns and narrow the search root".to_string()
            } else if row.name.contains("index_query") {
                slow_index_query_hint(report)
            } else if row.name.contains("index_build") {
                "review index_roots and generated directories".to_string()
            } else if row.name.contains("tui_symbols") {
                "prefer the index source or rebuild a fresh symbol cache".to_string()
            } else if row.name.contains("trace") {
                "archive old trace sessions or filter by session".to_string()
            } else {
                "compare with benchmark history and inspect workspace health".to_string()
            };
            format!("{} took {}ms; {}", row.name, row.elapsed_ms, hint)
        })
        .collect()
}

fn slow_index_query_hint(report: &BenchReport) -> String {
    let Some(root) = &report.root else {
        return "build index shards for large symbol tables".to_string();
    };
    let Ok(status) = crate::index::shard_status(root) else {
        return "build index shards for large symbol tables".to_string();
    };

    if status.exists && !status.stale {
        return "use index shard-query with a top-level path hint for large symbol tables".to_string();
    }
    if status.exists && status.stale {
        return "refresh stale index shards with `fcs index shards --write`".to_string();
    }
    "build index shards for large symbol tables".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_bench_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fcs_bench_{name}_{}", std::process::id()))
    }

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

    #[test]
    fn benchmark_report_explains_slow_rows() {
        let mut report = BenchReport::new(None);
        report.rows.push(BenchRow {
            name: "index_query_symbols".to_string(),
            elapsed_ms: 700,
            count: 4,
        });

        let output = format_report(&report, "text").unwrap();

        assert!(output.contains("explain: index_query_symbols took 700ms"));
        assert!(output.contains("build index shards"));
    }

    #[test]
    fn benchmark_report_uses_shard_query_hint_when_shards_are_fresh() {
        let temp_dir = temp_bench_dir("fresh_shard_hint");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        let cache_dir = crate::workspace::cache_dir_for_root(&temp_dir).unwrap();
        let _ = fs::remove_dir_all(&cache_dir);
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");
        crate::index::build(&temp_dir, &[], &[], &ignore_file).unwrap();
        crate::index::build_shards(&temp_dir, 1).unwrap();

        let mut report = BenchReport::new(Some(temp_dir.clone()));
        report.rows.push(BenchRow {
            name: "index_query_symbols".to_string(),
            elapsed_ms: 700,
            count: 1,
        });

        let output = format_report(&report, "text").unwrap();

        assert!(output.contains("use index shard-query"));
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&cache_dir);
    }

    #[test]
    fn benchmark_history_reports_trend() {
        let temp_dir = temp_bench_dir("history");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        if let Some(cache_dir) = benchmark_history_path(&temp_dir)
            .unwrap()
            .parent()
            .map(Path::to_path_buf)
        {
            let _ = fs::remove_dir_all(cache_dir);
        }

        let mut first = BenchReport::new(Some(temp_dir.clone()));
        first.rows.push(BenchRow {
            name: "search".to_string(),
            elapsed_ms: 10,
            count: 1,
        });
        write_report(&temp_dir, &first).unwrap();

        let mut second = BenchReport::new(Some(temp_dir.clone()));
        second.rows.push(BenchRow {
            name: "search".to_string(),
            elapsed_ms: 15,
            count: 1,
        });
        write_report(&temp_dir, &second).unwrap();

        let output = format_report(&second, "text").unwrap();

        assert!(output.contains("trend: search 10ms -> 15ms"));

        let _ = fs::remove_dir_all(&temp_dir);
        if let Some(cache_dir) = benchmark_history_path(&temp_dir)
            .unwrap()
            .parent()
            .map(Path::to_path_buf)
        {
            let _ = fs::remove_dir_all(cache_dir);
        }
    }
}
