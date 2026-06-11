use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::{CodeItem, Location};
use crate::errors::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphOptions {
    pub limit: usize,
    pub depth: usize,
    pub fanout: usize,
    pub exclude: Vec<String>,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            limit: 500,
            depth: 1,
            fanout: 0,
            exclude: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    Text,
    Json,
    Mermaid,
    Dot,
}

impl GraphFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "text" | "graph" | "markdown" | "md" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "mermaid" | "mmd" => Ok(Self::Mermaid),
            "dot" | "graphviz" => Ok(Self::Dot),
            other => Err(AppError::General(format!("Unsupported graph format: {other}"))),
        }
    }
}

pub fn lsp_edges(origin: &Location, relation: &str, items: &[CodeItem]) -> Vec<GraphEdge> {
    let from = location_label(origin);
    items
        .iter()
        .map(|item| GraphEdge {
            from: from.clone(),
            to: location_label(&item.location),
            kind: relation.to_string(),
            detail: item.display_text().to_string(),
        })
        .collect()
}

pub fn index_fallback_edges(
    root: &Path,
    origin: &Location,
    relation: &str,
    reason: &str,
    options: &GraphOptions,
) -> Result<Vec<GraphEdge>> {
    let Some(index) = crate::index::load(root)? else {
        return Ok(Vec::new());
    };
    let origin_path = relative_path(&normalize_root(root), origin.path());
    let origin_line = origin.line.unwrap_or(1);
    let mut symbols = index
        .symbols
        .into_iter()
        .filter(|symbol| symbol.path == origin_path)
        .collect::<Vec<_>>();
    symbols.sort_by_key(|symbol| {
        let distance = symbol.line.abs_diff(origin_line);
        (distance, symbol.line, symbol.name.clone())
    });

    let from = location_label(origin);
    let edges = symbols
        .into_iter()
        .take(options.limit.max(1))
        .map(|symbol| GraphEdge {
            from: from.clone(),
            to: format!("{}:{} {}", symbol.path, symbol.line, symbol.name),
            kind: format!("fallback:index:{relation}"),
            detail: format!("{reason}; {} {}", symbol.kind, symbol.detail),
        })
        .collect::<Vec<GraphEdge>>();
    Ok(apply_options(&dedupe_edges(edges), options))
}

pub fn apply_options(edges: &[GraphEdge], options: &GraphOptions) -> Vec<GraphEdge> {
    if options.depth == 0 || options.limit == 0 {
        return Vec::new();
    }

    let mut filtered = apply_fanout(
        edges
            .iter()
            .filter(|edge| !is_edge_excluded(edge, &options.exclude))
            .cloned(),
        options.fanout,
    );
    filtered.truncate(options.limit);
    filtered
}

pub fn import_edges(root: &Path, files: &[CodeItem], options: &GraphOptions) -> Result<Vec<GraphEdge>> {
    dependency_edges(root, files, options, &[SourceEdgeKind::Import, SourceEdgeKind::Module])
}

pub fn module_edges(root: &Path, files: &[CodeItem], options: &GraphOptions) -> Result<Vec<GraphEdge>> {
    dependency_edges(root, files, options, &[SourceEdgeKind::Module])
}

pub fn call_edges(root: &Path, files: &[CodeItem], options: &GraphOptions) -> Result<Vec<GraphEdge>> {
    if options.depth == 0 || options.limit == 0 {
        return Ok(Vec::new());
    }

    let root = normalize_root(root);
    let mut edges = Vec::new();
    for file in files.iter().take(options.limit) {
        let path = resolve_path(&root, &file.location.path);
        if is_label_excluded(&relative_path(&root, &path), &options.exclude) {
            continue;
        }

        edges.extend(collect_call_edges(&root, &path)?);
    }

    Ok(apply_options(&dedupe_edges(edges), options))
}

pub fn format_edges(edges: &[GraphEdge], format: GraphFormat) -> Result<String> {
    match format {
        GraphFormat::Text => Ok(format_edges_text(edges)),
        GraphFormat::Json => serde_json::to_string_pretty(edges)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|err| AppError::General(err.to_string())),
        GraphFormat::Mermaid => Ok(format_edges_mermaid(edges)),
        GraphFormat::Dot => Ok(format_edges_dot(edges)),
    }
}

fn apply_fanout(edges: impl Iterator<Item = GraphEdge>, fanout: usize) -> Vec<GraphEdge> {
    let mut counts = HashMap::<String, usize>::new();
    let mut filtered = Vec::new();

    for edge in edges {
        let count = counts.entry(edge.from.clone()).or_default();
        if fanout > 0 && *count >= fanout {
            continue;
        }

        *count += 1;
        filtered.push(edge);
    }

    filtered
}

fn dedupe_edges(mut edges: Vec<GraphEdge>) -> Vec<GraphEdge> {
    edges.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| left.detail.cmp(&right.detail))
    });

    let mut seen = HashSet::<(String, String, String, String)>::new();
    edges
        .into_iter()
        .filter(|edge| {
            seen.insert((
                edge.from.clone(),
                edge.to.clone(),
                edge.kind.clone(),
                edge.detail.clone(),
            ))
        })
        .collect()
}

fn format_edges_text(edges: &[GraphEdge]) -> String {
    let mut output = String::from("# fcs Graph\n\n");
    if edges.is_empty() {
        output.push_str("- <empty>\n");
        return output;
    }

    for edge in edges {
        output.push_str(&format!(
            "- {} -[{}]-> {} | {}\n",
            edge.from, edge.kind, edge.to, edge.detail
        ));
    }
    output
}

fn format_edges_mermaid(edges: &[GraphEdge]) -> String {
    let mut output = String::from("flowchart LR\n");
    if edges.is_empty() {
        output.push_str("  %% <empty>\n");
        return output;
    }

    for edge in edges {
        output.push_str(&format!(
            "  \"{}\" -->|\"{}\"| \"{}\"\n",
            escape_mermaid(&edge.from),
            escape_mermaid(&edge.kind),
            escape_mermaid(&edge.to)
        ));
    }
    output
}

fn format_edges_dot(edges: &[GraphEdge]) -> String {
    let mut output = String::from("digraph fcs_graph {\n  rankdir=LR;\n");
    if edges.is_empty() {
        output.push_str("  // <empty>\n");
    }

    for edge in edges {
        output.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            escape_dot(&edge.from),
            escape_dot(&edge.to),
            escape_dot(&edge.kind)
        ));
    }
    output.push_str("}\n");
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceEdgeKind {
    Import,
    Module,
}

impl SourceEdgeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Module => "module",
        }
    }
}

struct ParsedSourceEdge {
    edge: GraphEdge,
    local_path: Option<PathBuf>,
}

struct ParsedDependencyTarget {
    kind: SourceEdgeKind,
    target: String,
}

fn dependency_edges(
    root: &Path,
    files: &[CodeItem],
    options: &GraphOptions,
    kinds: &[SourceEdgeKind],
) -> Result<Vec<GraphEdge>> {
    if options.depth == 0 || options.limit == 0 {
        return Ok(Vec::new());
    }

    let root = normalize_root(root);
    let max_depth = options.depth.max(1);
    let seed_limit = files.len().min(options.limit.max(1));
    let scan_cap = seed_limit.saturating_mul(max_depth).max(seed_limit);
    let mut queue = VecDeque::<(PathBuf, usize)>::new();
    let mut scanned = HashSet::<String>::new();
    let mut edges = Vec::new();

    for file in files.iter().take(seed_limit) {
        queue.push_back((resolve_path(&root, &file.location.path), 1));
    }

    while let Some((path, depth)) = queue.pop_front() {
        if scanned.len() >= scan_cap {
            break;
        }

        let scan_key = relative_path(&root, &path);
        if is_label_excluded(&scan_key, &options.exclude) || !scanned.insert(scan_key) {
            continue;
        }

        for parsed in collect_dependency_edges(&root, &path, kinds)? {
            if is_edge_excluded(&parsed.edge, &options.exclude) {
                continue;
            }

            if depth < max_depth {
                if let Some(next_path) = parsed.local_path {
                    queue.push_back((next_path, depth + 1));
                }
            }

            edges.push(parsed.edge);
        }
    }

    Ok(apply_options(&dedupe_edges(edges), options))
}

fn collect_dependency_edges(root: &Path, path: &Path, kinds: &[SourceEdgeKind]) -> Result<Vec<ParsedSourceEdge>> {
    let Ok(file) = File::open(path) else {
        return Ok(Vec::new());
    };

    let from = relative_path(root, path);
    let mut edges = Vec::new();
    for line in BufReader::new(file).lines().map_while(std::result::Result::ok) {
        if let Some(parsed) = parse_dependency_target(&line) {
            if !kinds.contains(&parsed.kind) {
                continue;
            }

            let (to, local_path) = resolve_import_target(root, path, &parsed.target);
            edges.push(ParsedSourceEdge {
                edge: GraphEdge {
                    from: from.clone(),
                    to,
                    kind: parsed.kind.as_str().to_string(),
                    detail: line.trim().to_string(),
                },
                local_path,
            });
        }
    }
    Ok(edges)
}

fn collect_call_edges(root: &Path, path: &Path) -> Result<Vec<GraphEdge>> {
    let Ok(file) = File::open(path) else {
        return Ok(Vec::new());
    };

    let from_path = relative_path(root, path);
    let mut edges = Vec::new();
    for (index, line) in BufReader::new(file)
        .lines()
        .map_while(std::result::Result::ok)
        .enumerate()
    {
        for target in parse_call_targets(&line) {
            edges.push(GraphEdge {
                from: format!("{}:{}", from_path, index + 1),
                to: target,
                kind: "call".to_string(),
                detail: line.trim().to_string(),
            });
        }
    }

    Ok(edges)
}

fn parse_dependency_target(line: &str) -> Option<ParsedDependencyTarget> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("#include") {
        return rest
            .trim()
            .trim_matches(['<', '>', '"'])
            .split_whitespace()
            .next()
            .map(|target| ParsedDependencyTarget {
                kind: SourceEdgeKind::Import,
                target: target.to_string(),
            });
    }

    let rust_item = strip_rust_visibility(trimmed);
    if let Some(rest) = rust_item.strip_prefix("use ") {
        return Some(ParsedDependencyTarget {
            kind: SourceEdgeKind::Import,
            target: rest.trim_end_matches(';').trim().to_string(),
        });
    }
    if let Some(rest) = rust_item.strip_prefix("mod ") {
        let rest = rest.trim();
        if rest.contains('{') {
            return None;
        }
        return rest
            .trim_end_matches(';')
            .split_whitespace()
            .next()
            .map(|target| ParsedDependencyTarget {
                kind: SourceEdgeKind::Module,
                target: target.to_string(),
            });
    }

    if let Some(rest) = trimmed.strip_prefix("import ") {
        let target = parse_import_statement_target(rest)?;
        return Some(ParsedDependencyTarget {
            kind: SourceEdgeKind::Import,
            target,
        });
    }

    if let Some(rest) = trimmed.strip_prefix("from ") {
        return rest.split_whitespace().next().map(|target| ParsedDependencyTarget {
            kind: SourceEdgeKind::Import,
            target: target.trim_matches(['"', '\'']).trim_end_matches(';').to_string(),
        });
    }

    None
}

fn strip_rust_visibility(value: &str) -> &str {
    if let Some(rest) = value.strip_prefix("pub ") {
        return rest.trim_start();
    }

    if let Some(rest) = value.strip_prefix("pub(") {
        if let Some(index) = rest.find(')') {
            return rest[index + 1..].trim_start();
        }
    }

    value
}

fn parse_import_statement_target(rest: &str) -> Option<String> {
    if let Some(target) = quoted_segment_after(rest, " from ") {
        return Some(target);
    }

    quoted_segment(rest).or_else(|| {
        rest.split_whitespace()
            .next()
            .map(|value| value.trim_matches(['"', '\'']).trim_end_matches(';').to_string())
    })
}

fn quoted_segment_after(value: &str, marker: &str) -> Option<String> {
    let (_, rest) = value.split_once(marker)?;
    quoted_segment(rest)
}

fn quoted_segment(value: &str) -> Option<String> {
    let start = value.find(['"', '\''])?;
    let quote = value[start..].chars().next()?;
    let rest = &value[start + quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn parse_call_targets(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || parse_dependency_target(trimmed).is_some()
        || is_likely_definition_line(trimmed)
    {
        return Vec::new();
    }

    let mut calls = Vec::new();
    let mut offset = 0;
    while let Some(relative_index) = trimmed[offset..].find('(') {
        let index = offset + relative_index;
        if let Some(target) = call_target_before_paren(&trimmed[..index]) {
            if !is_call_keyword(&target) && !calls.contains(&target) {
                calls.push(target);
            }
        }
        offset = index + 1;
    }

    calls
}

fn call_target_before_paren(prefix: &str) -> Option<String> {
    let prefix = prefix.trim_end();
    let start = prefix
        .char_indices()
        .rev()
        .find(|(_, ch)| !is_call_target_char(*ch))
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    let target = prefix[start..].trim_matches([':', '.']);
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

fn is_call_target_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '.'
}

fn is_likely_definition_line(line: &str) -> bool {
    line.starts_with("fn ")
        || line.starts_with("pub fn ")
        || line.starts_with("async fn ")
        || line.starts_with("pub async fn ")
        || line.starts_with("def ")
        || line.starts_with("class ")
        || line.starts_with("struct ")
        || line.starts_with("enum ")
}

fn is_call_keyword(value: &str) -> bool {
    matches!(
        value,
        "if" | "for" | "while" | "switch" | "match" | "return" | "sizeof" | "catch"
    )
}

fn resolve_import_target(root: &Path, from: &Path, target: &str) -> (String, Option<PathBuf>) {
    for candidate in import_candidates(root, from, target) {
        if candidate.is_file() {
            return (relative_path(root, &candidate), Some(candidate));
        }
    }

    (target.to_string(), None)
}

fn import_candidates(root: &Path, from: &Path, target: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let from_dir = from.parent().unwrap_or(root);
    let target_path = Path::new(target);

    if target_path.is_absolute() {
        candidates.push(target_path.to_path_buf());
    } else {
        candidates.push(from_dir.join(target_path));
        candidates.push(root.join(target_path));
    }

    for module in module_path_prefixes(target) {
        candidates.push(from_dir.join(format!("{module}.rs")));
        candidates.push(from_dir.join(&module).join("mod.rs"));
        candidates.push(root.join("src").join(format!("{module}.rs")));
        candidates.push(root.join("src").join(&module).join("mod.rs"));
    }

    candidates
}

fn module_path_prefixes(target: &str) -> Vec<String> {
    let mut parts = target
        .split("::")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter(|part| *part != "crate" && *part != "self" && *part != "super")
        .take_while(|part| !part.contains('{') && !part.contains('*') && *part != "as")
        .map(|part| part.trim_matches(';').to_string())
        .collect::<Vec<String>>();

    let mut prefixes = Vec::new();
    while !parts.is_empty() {
        prefixes.push(parts.join("/"));
        parts.pop();
    }
    prefixes
}

fn is_edge_excluded(edge: &GraphEdge, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| matches_edge_pattern(edge, pattern))
}

fn matches_edge_pattern(edge: &GraphEdge, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }

    if let Some((field, value)) = pattern.split_once(':') {
        if value.is_empty() {
            return false;
        }

        return match field {
            "from" => edge.from.contains(value),
            "to" => edge.to.contains(value),
            "kind" => edge.kind.contains(value),
            "detail" => edge.detail.contains(value),
            _ => {
                edge.from.contains(pattern)
                    || edge.to.contains(pattern)
                    || edge.kind.contains(pattern)
                    || edge.detail.contains(pattern)
            }
        };
    }

    edge.from.contains(pattern)
        || edge.to.contains(pattern)
        || edge.kind.contains(pattern)
        || edge.detail.contains(pattern)
}

fn is_label_excluded(label: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| !pattern.is_empty() && label.contains(pattern))
}

fn location_label(location: &Location) -> String {
    let line = location.line.unwrap_or(1);
    let column = location.column.map(|value| format!(":{value}")).unwrap_or_default();
    format!("{}:{line}{column}", location.path.display())
}

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() || path.exists() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn normalize_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn escape_mermaid(value: &str) -> String {
    value.replace('"', "'").replace('\n', " ")
}

fn escape_dot(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_graph_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fcs_graph_{name}_{}", std::process::id()))
    }

    #[test]
    fn parses_common_import_forms() {
        let include = parse_dependency_target("#include <stdio.h>").unwrap();
        let rust_use = parse_dependency_target("pub use crate::config;").unwrap();
        let rust_mod = parse_dependency_target("pub(crate) mod cli;").unwrap();
        let python_import = parse_dependency_target("import os;").unwrap();
        let python_from = parse_dependency_target("from pathlib import Path").unwrap();
        let js_import = parse_dependency_target("import { run } from './runner.js';").unwrap();

        assert_eq!(include.kind, SourceEdgeKind::Import);
        assert_eq!(include.target, "stdio.h");
        assert_eq!(rust_use.target, "crate::config");
        assert_eq!(rust_mod.kind, SourceEdgeKind::Module);
        assert_eq!(rust_mod.target, "cli");
        assert_eq!(parse_dependency_target("mod tests {").map(|target| target.target), None);
        assert_eq!(python_import.target, "os");
        assert_eq!(python_from.target, "pathlib");
        assert_eq!(js_import.target, "./runner.js");
    }

    #[test]
    fn formats_edges_as_text_json_mermaid_and_dot() {
        let edges = vec![GraphEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            kind: "reference".to_string(),
            detail: "a -> b".to_string(),
        }];

        assert!(format_edges(&edges, GraphFormat::Text)
            .unwrap()
            .contains("a -[reference]-> b"));
        assert!(format_edges(&edges, GraphFormat::Json)
            .unwrap()
            .contains("\"kind\": \"reference\""));
        assert!(format_edges(&edges, GraphFormat::Mermaid)
            .unwrap()
            .contains("\"a\" -->|\"reference\"| \"b\""));
        assert!(format_edges(&edges, GraphFormat::Dot)
            .unwrap()
            .contains("\"a\" -> \"b\" [label=\"reference\"]"));
    }

    #[test]
    fn graph_options_filter_and_limit_fanout() {
        let edges = vec![
            GraphEdge {
                from: "a".to_string(),
                to: "b".to_string(),
                kind: "import".to_string(),
                detail: "mod b;".to_string(),
            },
            GraphEdge {
                from: "a".to_string(),
                to: "skip".to_string(),
                kind: "import".to_string(),
                detail: "mod skip;".to_string(),
            },
            GraphEdge {
                from: "a".to_string(),
                to: "c".to_string(),
                kind: "import".to_string(),
                detail: "mod c;".to_string(),
            },
        ];
        let options = GraphOptions {
            limit: 10,
            depth: 1,
            fanout: 1,
            exclude: vec!["skip".to_string()],
        };

        let filtered = apply_options(&edges, &options);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].to, "b");
    }

    #[test]
    fn graph_options_apply_limit_and_field_specific_excludes() {
        let edges = vec![
            GraphEdge {
                from: "src/a.rs".to_string(),
                to: "run".to_string(),
                kind: "call".to_string(),
                detail: "run();".to_string(),
            },
            GraphEdge {
                from: "src/a.rs".to_string(),
                to: "src/b.rs".to_string(),
                kind: "module".to_string(),
                detail: "mod b;".to_string(),
            },
            GraphEdge {
                from: "src/c.rs".to_string(),
                to: "src/d.rs".to_string(),
                kind: "import".to_string(),
                detail: "use crate::d;".to_string(),
            },
        ];
        let options = GraphOptions {
            limit: 1,
            depth: 1,
            fanout: 0,
            exclude: vec!["kind:call".to_string()],
        };

        let filtered = apply_options(&edges, &options);

        assert_eq!(filtered.len(), 1);
        assert_ne!(filtered[0].kind, "call");
    }

    #[test]
    fn index_fallback_edges_use_nearby_index_symbols() {
        let temp_dir = temp_graph_dir("index_fallback");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();
        std::fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        std::fs::write(
            temp_dir.join("src").join("main.rs"),
            "pub fn helper() {}\npub fn main() { helper(); }\n",
        )
        .unwrap();
        let ignore_file = temp_dir.join("missing.ignore");
        crate::index::build(&temp_dir, &[], &[], &ignore_file).unwrap();

        let origin = Location::new(temp_dir.join("src").join("main.rs"), Some(2), Some(1));
        let edges = index_fallback_edges(
            &temp_dir,
            &origin,
            "references",
            "lsp returned no edges",
            &GraphOptions {
                limit: 5,
                depth: 1,
                fanout: 0,
                exclude: Vec::new(),
            },
        )
        .unwrap();

        assert!(edges.iter().any(|edge| {
            edge.kind == "fallback:index:references"
                && edge.to.contains("main")
                && edge.detail.contains("lsp returned no edges")
        }));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn import_graph_expands_local_modules_by_depth() {
        let temp_dir = temp_graph_dir("depth");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();
        std::fs::write(temp_dir.join("src").join("a.rs"), "mod b;\nmod skip;\n").unwrap();
        std::fs::write(temp_dir.join("src").join("b.rs"), "mod c;\n").unwrap();
        std::fs::write(temp_dir.join("src").join("c.rs"), "\n").unwrap();
        std::fs::write(temp_dir.join("src").join("skip.rs"), "\n").unwrap();

        let files = vec![
            CodeItem::file_with_display(temp_dir.join("src").join("a.rs"), "src/a.rs"),
            CodeItem::file_with_display(temp_dir.join("src").join("b.rs"), "src/b.rs"),
            CodeItem::file_with_display(temp_dir.join("src").join("c.rs"), "src/c.rs"),
        ];
        let options = GraphOptions {
            limit: 10,
            depth: 2,
            fanout: 1,
            exclude: vec!["skip".to_string()],
        };

        let edges = import_edges(&temp_dir, &files, &options).unwrap();

        assert!(edges
            .iter()
            .any(|edge| edge.from == "src/a.rs" && edge.to == "src/b.rs" && edge.kind == "module"));
        assert!(edges
            .iter()
            .any(|edge| edge.from == "src/b.rs" && edge.to == "src/c.rs" && edge.kind == "module"));
        assert!(!edges.iter().any(|edge| edge.to.contains("skip")));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn module_edges_return_only_modules_and_call_edges_skip_definitions() {
        let temp_dir = temp_graph_dir("module_call");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();
        std::fs::write(
            temp_dir.join("src").join("main.rs"),
            "mod worker;\nuse crate::config;\nfn main() {\n    worker::run(config());\n    if ready() {}\n}\n",
        )
        .unwrap();
        std::fs::write(temp_dir.join("src").join("worker.rs"), "pub fn run(_: Config) {}\n").unwrap();
        let files = vec![CodeItem::file_with_display(
            temp_dir.join("src").join("main.rs"),
            "src/main.rs",
        )];
        let options = GraphOptions {
            limit: 10,
            depth: 1,
            fanout: 0,
            exclude: Vec::new(),
        };

        let modules = module_edges(&temp_dir, &files, &options).unwrap();
        let calls = call_edges(&temp_dir, &files, &options).unwrap();

        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].kind, "module");
        assert_eq!(modules[0].to, "src/worker.rs");
        assert!(calls.iter().any(|edge| edge.kind == "call" && edge.to == "worker::run"));
        assert!(calls.iter().any(|edge| edge.kind == "call" && edge.to == "config"));
        assert!(calls.iter().any(|edge| edge.kind == "call" && edge.to == "ready"));
        assert!(!calls.iter().any(|edge| edge.to == "main"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
