use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::LspConfig;
use crate::errors::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySource {
    All,
    Index,
    Trace,
    Semantic,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryExpression {
    pub terms: Vec<String>,
    pub fields: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryMatch {
    pub source: String,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub label: String,
    pub kind: String,
    pub detail: String,
    pub score: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryExplanation {
    pub expression: String,
    pub source: String,
    pub selected_sources: Vec<String>,
    pub terms: Vec<String>,
    pub fields: BTreeMap<String, Vec<String>>,
    pub supported_fields: Vec<String>,
}

impl QuerySource {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "all" => Ok(Self::All),
            "index" | "idx" => Ok(Self::Index),
            "trace" | "traces" => Ok(Self::Trace),
            "semantic" | "lsp" => Ok(Self::Semantic),
            "auto" | "smart" => Ok(Self::Auto),
            other => Err(AppError::General(format!(
                "Unsupported query source: {other}. Use all, index, trace, semantic, or auto"
            ))),
        }
    }

    fn includes_index(self) -> bool {
        matches!(self, Self::All | Self::Index | Self::Auto)
    }

    fn includes_trace(self) -> bool {
        matches!(self, Self::All | Self::Trace | Self::Auto)
    }

    fn includes_lsp(self) -> bool {
        matches!(self, Self::Semantic | Self::Auto)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Index => "index",
            Self::Trace => "trace",
            Self::Semantic => "semantic",
            Self::Auto => "auto",
        }
    }

    pub fn selected_sources(self) -> Vec<String> {
        let mut sources = Vec::new();
        if self.includes_index() {
            sources.push("index".to_string());
        }
        if self.includes_trace() {
            sources.push("trace".to_string());
        }
        if self.includes_lsp() {
            sources.push("semantic".to_string());
        }
        sources
    }
}

pub fn parse_expression(expression: &str) -> QueryExpression {
    let mut parsed = QueryExpression {
        terms: Vec::new(),
        fields: BTreeMap::new(),
    };

    for token in tokenize(expression) {
        if let Some((field, value)) = token.split_once(':') {
            let field = field.trim().to_ascii_lowercase();
            let value = value.trim();
            if is_supported_field(&field) && !value.is_empty() {
                parsed.fields.entry(field).or_default().push(value.to_string());
                continue;
            }
        }
        if !token.trim().is_empty() {
            parsed.terms.push(token);
        }
    }

    parsed
}

pub fn run(root: &Path, expression: &str, source: QuerySource, limit: usize) -> Result<Vec<QueryMatch>> {
    run_with_config(root, expression, source, limit, None)
}

pub fn explain(expression: &str, source: QuerySource) -> QueryExplanation {
    let parsed = parse_expression(expression);
    QueryExplanation {
        expression: expression.to_string(),
        source: source.as_str().to_string(),
        selected_sources: source.selected_sources(),
        terms: parsed.terms,
        fields: parsed.fields,
        supported_fields: supported_fields().iter().map(|field| (*field).to_string()).collect(),
    }
}

pub fn format_explanation(explanation: &QueryExplanation, format: &str) -> Result<String> {
    match format {
        "text" => {
            let mut output = String::new();
            output.push_str(&format!("expression: {}\n", explanation.expression));
            output.push_str(&format!("source: {}\n", explanation.source));
            output.push_str(&format!(
                "selected_sources: {}\n",
                explanation.selected_sources.join(", ")
            ));
            output.push_str(&format!("terms: {}\n", display_values(&explanation.terms)));
            output.push_str("fields:\n");
            if explanation.fields.is_empty() {
                output.push_str("  none\n");
            } else {
                for (field, values) in &explanation.fields {
                    output.push_str(&format!("  {field}: {}\n", display_values(values)));
                }
            }
            output.push_str(&format!(
                "supported_fields: {}\n",
                explanation.supported_fields.join(", ")
            ));
            Ok(output)
        }
        "json" => serde_json::to_string_pretty(explanation)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|err| AppError::General(err.to_string())),
        other => Err(AppError::General(format!("Unsupported query explain format: {other}"))),
    }
}

pub fn run_with_config(
    root: &Path,
    expression: &str,
    source: QuerySource,
    limit: usize,
    lsp_config: Option<&LspConfig>,
) -> Result<Vec<QueryMatch>> {
    let parsed = parse_expression(expression);
    let mut matches = Vec::new();

    if source.includes_index() {
        matches.extend(query_index(root, &parsed)?);
    }
    if source.includes_trace() {
        matches.extend(query_trace(root, &parsed)?);
    }
    if source.includes_lsp() {
        match lsp_config {
            Some(config) => match query_lsp(root, &parsed, expression, config) {
                Ok(items) => matches.extend(items),
                Err(err) if source == QuerySource::Auto => {
                    matches.push(QueryMatch {
                        source: "lsp:error".to_string(),
                        path: root.to_path_buf(),
                        line: None,
                        column: None,
                        label: "semantic query unavailable".to_string(),
                        kind: "status".to_string(),
                        detail: err.to_string(),
                        score: usize::MAX.saturating_sub(1),
                    });
                }
                Err(err) => return Err(err),
            },
            None if source == QuerySource::Auto => {}
            None => {
                return Err(AppError::General(
                    "Semantic query requires an LSP config; use query::run_with_config".to_string(),
                ));
            }
        }
    }

    matches.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(dedup_matches(matches, limit.max(1)))
}

pub fn format_matches(matches: &[QueryMatch], format: &str) -> Result<String> {
    match format {
        "text" => {
            if matches.is_empty() {
                return Ok("No query matches\n".to_string());
            }

            let mut output = String::new();
            for item in matches {
                let line = item.line.map(|line| format!(":{line}")).unwrap_or_default();
                let column = item.column.map(|column| format!(":{column}")).unwrap_or_default();
                output.push_str(&format!(
                    "{} {}{}{} [{}] {} | {}\n",
                    item.source,
                    item.path.display(),
                    line,
                    column,
                    item.kind,
                    item.label,
                    item.detail
                ));
            }
            Ok(output)
        }
        "json" => serde_json::to_string_pretty(matches)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|err| AppError::General(err.to_string())),
        other => Err(AppError::General(format!("Unsupported query output format: {other}"))),
    }
}

fn query_index(root: &Path, expression: &QueryExpression) -> Result<Vec<QueryMatch>> {
    let Some(index) = crate::index::load(root)? else {
        return Ok(Vec::new());
    };
    let mut matches = Vec::new();

    for file in index.files {
        let candidate = Candidate {
            source: "index:file",
            path: PathBuf::from(&file.path),
            line: None,
            column: None,
            label: file.path.clone(),
            kind: "file".to_string(),
            language: file.language,
            name: file.path.clone(),
            detail: format!("{} bytes", file.size_bytes),
            status: None,
            priority: None,
            session: None,
            tags: Vec::new(),
        };
        if let Some(score) = score_candidate(&candidate, expression) {
            matches.push(candidate.into_match(score));
        }
    }

    for symbol in index.symbols {
        let candidate = Candidate {
            source: "index:symbol",
            path: PathBuf::from(&symbol.path),
            line: Some(symbol.line),
            column: symbol.column,
            label: symbol.label,
            kind: symbol.kind,
            language: symbol.language,
            name: symbol.name,
            detail: symbol.detail,
            status: None,
            priority: None,
            session: None,
            tags: Vec::new(),
        };
        if let Some(score) = score_candidate(&candidate, expression) {
            matches.push(candidate.into_match(score));
        }
    }

    Ok(matches)
}

fn query_trace(root: &Path, expression: &QueryExpression) -> Result<Vec<QueryMatch>> {
    let root = normalize_path(root);
    let mut matches = Vec::new();
    for entry in crate::trace::list()? {
        if !trace_entry_is_under_root(&entry.path, &root) {
            continue;
        }

        let candidate = Candidate {
            source: "trace",
            path: entry.path.clone(),
            line: entry.line,
            column: entry.column,
            label: entry.label,
            kind: entry.kind,
            language: String::new(),
            name: String::new(),
            detail: entry.note.unwrap_or_default(),
            status: entry.status,
            priority: entry.priority,
            session: entry.session,
            tags: entry.tags,
        };
        if let Some(score) = score_candidate(&candidate, expression) {
            matches.push(candidate.into_match(score));
        }
    }

    Ok(matches)
}

fn query_lsp(
    root: &Path,
    expression: &QueryExpression,
    original_expression: &str,
    config: &LspConfig,
) -> Result<Vec<QueryMatch>> {
    let mut client = crate::lsp::LspClient::start_for_workspace(root, config)?;
    let query = lsp_query_text(expression, original_expression);
    let items = client.workspace_symbols(&query)?;
    let mut matches = Vec::new();

    for item in items {
        let (name, kind) = symbol_name_and_kind(&item.detail);
        let path = item.location.path.clone();
        let candidate = Candidate {
            source: "lsp:workspace-symbol",
            path: path.clone(),
            line: item.location.line,
            column: item.location.column,
            label: name.clone(),
            kind,
            language: language_for_path(&path),
            name,
            detail: item.display_text().to_string(),
            status: None,
            priority: None,
            session: None,
            tags: Vec::new(),
        };
        if let Some(score) = score_candidate(&candidate, expression) {
            matches.push(candidate.into_match(score));
        }
    }

    Ok(matches)
}

fn dedup_matches(matches: Vec<QueryMatch>, limit: usize) -> Vec<QueryMatch> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for item in matches {
        let key = (
            item.path.clone(),
            item.line,
            item.column,
            item.label.to_ascii_lowercase(),
        );
        if seen.insert(key) {
            deduped.push(item);
        }
        if deduped.len() >= limit {
            break;
        }
    }
    deduped
}

fn lsp_query_text(expression: &QueryExpression, original_expression: &str) -> String {
    if !expression.terms.is_empty() {
        return expression.terms.join(" ");
    }
    if let Some(names) = expression.fields.get("name") {
        if let Some(name) = names.first() {
            return name.clone();
        }
    }
    original_expression
        .split_whitespace()
        .filter(|token| !token.contains(':'))
        .collect::<Vec<&str>>()
        .join(" ")
}

fn symbol_name_and_kind(detail: &str) -> (String, String) {
    if let Some((name, rest)) = detail.rsplit_once(" [") {
        return (name.to_string(), rest.trim_end_matches(']').trim().to_string());
    }
    (detail.to_string(), "symbol".to_string())
}

fn language_for_path(path: &Path) -> String {
    match path.extension().and_then(|extension| extension.to_str()).unwrap_or("") {
        "c" => "c",
        "h" => "c-header",
        "cc" | "cpp" | "cxx" => "cpp",
        "hh" | "hpp" | "hxx" => "cpp-header",
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        _ => "unknown",
    }
    .to_string()
}

struct Candidate<'a> {
    source: &'a str,
    path: PathBuf,
    line: Option<usize>,
    column: Option<usize>,
    label: String,
    kind: String,
    language: String,
    name: String,
    detail: String,
    status: Option<String>,
    priority: Option<String>,
    session: Option<String>,
    tags: Vec<String>,
}

impl Candidate<'_> {
    fn into_match(self, score: usize) -> QueryMatch {
        QueryMatch {
            source: self.source.to_string(),
            path: self.path,
            line: self.line,
            column: self.column,
            label: self.label,
            kind: self.kind,
            detail: self.detail,
            score,
        }
    }

    fn haystack(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {} {}",
            self.path.display(),
            self.label,
            self.kind,
            self.language,
            self.name,
            self.detail,
            self.status.as_deref().unwrap_or_default(),
            self.priority.as_deref().unwrap_or_default(),
            self.session.as_deref().unwrap_or_default()
        )
    }
}

fn score_candidate(candidate: &Candidate<'_>, expression: &QueryExpression) -> Option<usize> {
    let haystack = candidate.haystack().to_ascii_lowercase();
    let mut score = 0;

    for term in &expression.terms {
        let term = term.to_ascii_lowercase();
        let position = haystack.find(&term)?;
        score += position;
    }

    for (field, values) in &expression.fields {
        for value in values {
            let value = value.to_ascii_lowercase();
            let field_value = field_value(candidate, field);
            if !field_value
                .iter()
                .any(|candidate_value| candidate_value.contains(&value))
            {
                return None;
            }
            score += field_value
                .iter()
                .filter_map(|candidate_value| candidate_value.find(&value))
                .min()
                .unwrap_or(0);
        }
    }

    Some(score)
}

fn field_value(candidate: &Candidate<'_>, field: &str) -> Vec<String> {
    match field {
        "kind" => vec![candidate.kind.to_ascii_lowercase()],
        "lang" | "language" => vec![candidate.language.to_ascii_lowercase()],
        "path" => vec![candidate.path.display().to_string().to_ascii_lowercase()],
        "name" => vec![
            candidate.name.to_ascii_lowercase(),
            candidate.label.to_ascii_lowercase(),
        ],
        "text" => vec![candidate.haystack().to_ascii_lowercase()],
        "source" => vec![candidate.source.to_ascii_lowercase()],
        "status" => vec![candidate.status.clone().unwrap_or_default().to_ascii_lowercase()],
        "priority" => vec![candidate.priority.clone().unwrap_or_default().to_ascii_lowercase()],
        "session" => vec![candidate.session.clone().unwrap_or_default().to_ascii_lowercase()],
        "tag" | "tags" => candidate.tags.iter().map(|tag| tag.to_ascii_lowercase()).collect(),
        _ => Vec::new(),
    }
}

fn is_supported_field(field: &str) -> bool {
    supported_fields().contains(&field)
}

fn supported_fields() -> &'static [&'static str] {
    &[
        "kind", "lang", "language", "path", "name", "text", "source", "status", "priority", "session", "tag", "tags",
    ]
}

fn display_values(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn tokenize(expression: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;

    for ch in expression.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quoted {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
            continue;
        }
        if ch.is_whitespace() && !quoted {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn trace_entry_is_under_root(path: &Path, root: &Path) -> bool {
    let path = normalize_path(path);
    path.starts_with(root) || !path.is_absolute()
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fielded_query_terms() {
        let parsed = parse_expression(r#"kind:function lang:rust path:src source:index text:"main loop" loose"#);

        assert_eq!(parsed.terms, vec!["loose".to_string()]);
        assert_eq!(parsed.fields["kind"], vec!["function".to_string()]);
        assert_eq!(parsed.fields["lang"], vec!["rust".to_string()]);
        assert_eq!(parsed.fields["source"], vec!["index".to_string()]);
        assert_eq!(parsed.fields["text"], vec!["main loop".to_string()]);
    }

    #[test]
    fn parses_semantic_and_auto_sources() {
        assert_eq!(QuerySource::parse("semantic").unwrap(), QuerySource::Semantic);
        assert_eq!(QuerySource::parse("lsp").unwrap(), QuerySource::Semantic);
        assert_eq!(QuerySource::parse("auto").unwrap(), QuerySource::Auto);
        assert!(QuerySource::parse("remote").is_err());
    }

    #[test]
    fn semantic_source_requires_config_for_plain_run() {
        let err = run(Path::new("."), "main", QuerySource::Semantic, 10).unwrap_err();

        assert!(err.to_string().contains("Semantic query requires"));
    }

    #[test]
    fn scores_candidate_with_field_filters() {
        let expression = parse_expression("kind:function lang:rust source:index main");
        let candidate = Candidate {
            source: "index:symbol",
            path: PathBuf::from("src/main.rs"),
            line: Some(3),
            column: None,
            label: "main".to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            name: "main".to_string(),
            detail: "fn main()".to_string(),
            status: None,
            priority: None,
            session: None,
            tags: Vec::new(),
        };

        assert!(score_candidate(&candidate, &expression).is_some());
        assert!(score_candidate(&candidate, &parse_expression("source:trace main")).is_none());
        assert!(score_candidate(&candidate, &parse_expression("kind:struct main")).is_none());
    }

    #[test]
    fn explains_query_plan() {
        let explanation = explain("source:trace status:open panic", QuerySource::Auto);
        let text = format_explanation(&explanation, "text").unwrap();

        assert_eq!(explanation.terms, vec!["panic".to_string()]);
        assert_eq!(explanation.fields["source"], vec!["trace".to_string()]);
        assert!(explanation.selected_sources.iter().any(|source| source == "index"));
        assert!(explanation.selected_sources.iter().any(|source| source == "trace"));
        assert!(explanation.selected_sources.iter().any(|source| source == "semantic"));
        assert!(text.contains("supported_fields"));
    }
}
