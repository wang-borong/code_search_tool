use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder};
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QueryMode {
    #[default]
    Fuzzy,
    Exact,
    Regex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryRunOptions {
    pub mode: QueryMode,
    pub macros: Vec<String>,
    pub score_explain: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedQuery {
    pub name: String,
    pub expression: String,
    pub source: String,
    pub mode: QueryMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedQueryStore {
    #[serde(default)]
    pub queries: Vec<SavedQuery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryExpression {
    pub terms: Vec<String>,
    pub fields: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub root: QueryNode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryNode {
    #[default]
    All,
    Term(String),
    Field {
        field: String,
        value: String,
    },
    Not(Box<QueryNode>),
    And(Vec<QueryNode>),
    Or(Vec<QueryNode>),
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
    pub execution_plan: String,
    pub filters: Vec<String>,
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

impl QueryMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "fuzzy" | "substring" | "default" => Ok(Self::Fuzzy),
            "exact" => Ok(Self::Exact),
            "regex" | "regexp" => Ok(Self::Regex),
            other => Err(AppError::General(format!(
                "Unsupported query mode: {other}. Use fuzzy, exact, or regex"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fuzzy => "fuzzy",
            Self::Exact => "exact",
            Self::Regex => "regex",
        }
    }
}

impl Default for QueryRunOptions {
    fn default() -> Self {
        Self {
            mode: QueryMode::Fuzzy,
            macros: Vec::new(),
            score_explain: false,
        }
    }
}

pub fn parse_expression(expression: &str) -> QueryExpression {
    let tokens = tokenize(expression);
    let root = QueryParser::new(&tokens).parse();
    let mut parsed = QueryExpression {
        terms: Vec::new(),
        fields: BTreeMap::new(),
        root,
    };
    collect_expression_summary(&parsed.root, &mut parsed.terms, &mut parsed.fields);
    parsed
}

struct QueryParser<'a> {
    tokens: &'a [String],
    position: usize,
}

impl<'a> QueryParser<'a> {
    fn new(tokens: &'a [String]) -> Self {
        Self { tokens, position: 0 }
    }

    fn parse(mut self) -> QueryNode {
        self.parse_or()
    }

    fn parse_or(&mut self) -> QueryNode {
        let mut branches = vec![self.parse_and()];
        while self.is_keyword("or") && self.next_starts_factor() {
            self.position += 1;
            branches.push(self.parse_and());
        }
        make_or(branches)
    }

    fn parse_and(&mut self) -> QueryNode {
        let mut nodes = Vec::new();
        while let Some(token) = self.current() {
            if token == ")" || (self.is_keyword("or") && !nodes.is_empty()) {
                break;
            }
            if self.is_keyword("and") && !nodes.is_empty() && self.next_starts_factor() {
                self.position += 1;
                continue;
            }
            nodes.push(self.parse_not());
        }
        make_and(nodes)
    }

    fn parse_not(&mut self) -> QueryNode {
        if self.is_keyword("not") && self.next_starts_factor() {
            self.position += 1;
            return QueryNode::Not(Box::new(self.parse_not()));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> QueryNode {
        if self.consume("(") {
            let node = self.parse_or();
            self.consume(")");
            return node;
        }

        let Some(token) = self.current().cloned() else {
            return QueryNode::All;
        };
        self.position += 1;
        atom_from_token(&token)
    }

    fn current(&self) -> Option<&String> {
        self.tokens.get(self.position)
    }

    fn consume(&mut self, expected: &str) -> bool {
        if self.current().is_some_and(|token| token == expected) {
            self.position += 1;
            return true;
        }
        false
    }

    fn is_keyword(&self, expected: &str) -> bool {
        self.current().is_some_and(|token| token.eq_ignore_ascii_case(expected))
    }

    fn next_starts_factor(&self) -> bool {
        self.tokens
            .get(self.position + 1)
            .is_some_and(|token| token != ")" && !token.eq_ignore_ascii_case("or"))
    }
}

fn atom_from_token(token: &str) -> QueryNode {
    if let Some((field, value)) = token.split_once(':') {
        let field = field.trim().to_ascii_lowercase();
        let value = value.trim();
        if is_supported_field(&field) && !value.is_empty() {
            return QueryNode::Field {
                field,
                value: value.to_string(),
            };
        }
    }
    if token.trim().is_empty() {
        QueryNode::All
    } else {
        QueryNode::Term(token.to_string())
    }
}

fn make_and(nodes: Vec<QueryNode>) -> QueryNode {
    let nodes = flatten_nodes(nodes, true);
    match nodes.len() {
        0 => QueryNode::All,
        1 => nodes.into_iter().next().unwrap_or(QueryNode::All),
        _ => QueryNode::And(nodes),
    }
}

fn make_or(nodes: Vec<QueryNode>) -> QueryNode {
    let nodes = flatten_nodes(nodes, false);
    match nodes.len() {
        0 => QueryNode::All,
        1 => nodes.into_iter().next().unwrap_or(QueryNode::All),
        _ => QueryNode::Or(nodes),
    }
}

fn flatten_nodes(nodes: Vec<QueryNode>, flatten_and: bool) -> Vec<QueryNode> {
    let mut flattened = Vec::new();
    for node in nodes {
        match node {
            QueryNode::All if flatten_and => {}
            QueryNode::And(children) if flatten_and => flattened.extend(children),
            QueryNode::Or(children) if !flatten_and => flattened.extend(children),
            other => flattened.push(other),
        }
    }
    flattened
}

fn collect_expression_summary(node: &QueryNode, terms: &mut Vec<String>, fields: &mut BTreeMap<String, Vec<String>>) {
    match node {
        QueryNode::All => {}
        QueryNode::Term(term) => terms.push(term.clone()),
        QueryNode::Field { field, value } => {
            fields.entry(field.clone()).or_default().push(value.clone());
        }
        QueryNode::Not(child) => collect_expression_summary(child, terms, fields),
        QueryNode::And(children) | QueryNode::Or(children) => {
            for child in children {
                collect_expression_summary(child, terms, fields);
            }
        }
    }
}

fn render_query_plan(node: &QueryNode) -> String {
    match node {
        QueryNode::All => "ALL".to_string(),
        QueryNode::Term(term) => format!("text:{term}"),
        QueryNode::Field { field, value } => format!("{field}:{value}"),
        QueryNode::Not(child) => format!("NOT({})", render_query_plan(child)),
        QueryNode::And(children) => {
            let parts = children.iter().map(render_query_plan).collect::<Vec<String>>();
            format!("AND({})", parts.join(", "))
        }
        QueryNode::Or(children) => {
            let parts = children.iter().map(render_query_plan).collect::<Vec<String>>();
            format!("OR({})", parts.join(", "))
        }
    }
}

fn summarize_filters(node: &QueryNode) -> Vec<String> {
    let mut filters = Vec::new();
    collect_filter_summary(node, false, &mut filters);
    filters
}

fn collect_filter_summary(node: &QueryNode, negated: bool, filters: &mut Vec<String>) {
    match node {
        QueryNode::All => {}
        QueryNode::Term(term) => {
            filters.push(format_filter(negated, &format!("text contains \"{term}\"")));
        }
        QueryNode::Field { field, value } => {
            filters.push(format_filter(negated, &format!("{field}:{value}")));
        }
        QueryNode::Not(child) => collect_filter_summary(child, !negated, filters),
        QueryNode::And(children) => {
            for child in children {
                collect_filter_summary(child, negated, filters);
            }
        }
        QueryNode::Or(children) => {
            let parts = children.iter().map(render_query_plan).collect::<Vec<String>>();
            if negated {
                filters.push(format!("exclude any of: {}", parts.join(" OR ")));
            } else {
                filters.push(format!("any of: {}", parts.join(" OR ")));
            }
        }
    }
}

fn format_filter(negated: bool, filter: &str) -> String {
    if negated {
        format!("exclude {filter}")
    } else {
        format!("require {filter}")
    }
}

pub fn run(root: &Path, expression: &str, source: QuerySource, limit: usize) -> Result<Vec<QueryMatch>> {
    run_with_config(root, expression, source, limit, None)
}

pub fn explain(expression: &str, source: QuerySource) -> QueryExplanation {
    explain_with_options(expression, source, &QueryRunOptions::default())
}

pub fn explain_with_options(expression: &str, source: QuerySource, options: &QueryRunOptions) -> QueryExplanation {
    let expanded = expand_query_macros(expression, &options.macros).unwrap_or_else(|_| expression.to_string());
    let parsed = parse_expression(&expanded);
    QueryExplanation {
        expression: expanded,
        source: source.as_str().to_string(),
        selected_sources: source.selected_sources(),
        terms: parsed.terms,
        fields: parsed.fields,
        execution_plan: render_query_plan(&parsed.root),
        filters: summarize_filters(&parsed.root),
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
            output.push_str(&format!("execution_plan: {}\n", explanation.execution_plan));
            output.push_str("fields:\n");
            if explanation.fields.is_empty() {
                output.push_str("  none\n");
            } else {
                for (field, values) in &explanation.fields {
                    output.push_str(&format!("  {field}: {}\n", display_values(values)));
                }
            }
            output.push_str("filters:\n");
            if explanation.filters.is_empty() {
                output.push_str("  none\n");
            } else {
                for filter in &explanation.filters {
                    output.push_str(&format!("  - {filter}\n"));
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

pub fn expand_query_macros(expression: &str, macros: &[String]) -> Result<String> {
    let mut expanded = expression.to_string();
    for macro_name in macros {
        let token = if macro_name.starts_with('@') {
            macro_name.clone()
        } else {
            format!("@{macro_name}")
        };
        expanded.push(' ');
        expanded.push_str(&token);
    }

    for (token, replacement) in builtin_query_macros() {
        expanded = expanded.replace(token, replacement);
    }

    let unknown = expanded
        .split_whitespace()
        .find(|part| part.starts_with('@'))
        .map(str::to_string);
    if let Some(unknown) = unknown {
        return Err(AppError::General(format!("Unsupported query macro: {unknown}")));
    }

    Ok(expanded)
}

pub fn list_saved_queries(root: &Path) -> Result<Vec<SavedQuery>> {
    let mut store = load_query_store(root)?;
    store.queries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(store.queries)
}

pub fn save_query(root: &Path, name: &str, expression: &str, source: QuerySource, mode: QueryMode) -> Result<()> {
    validate_saved_query_name(name)?;
    let mut store = load_query_store(root)?;
    store.queries.retain(|query| query.name != name);
    store.queries.push(SavedQuery {
        name: name.to_string(),
        expression: expression.to_string(),
        source: source.as_str().to_string(),
        mode,
    });
    store.queries.sort_by(|left, right| left.name.cmp(&right.name));
    write_query_store(root, &store)
}

pub fn load_saved_query(root: &Path, name: &str) -> Result<SavedQuery> {
    load_query_store(root)?
        .queries
        .into_iter()
        .find(|query| query.name == name)
        .ok_or_else(|| AppError::General(format!("Saved query not found: {name}")))
}

pub fn delete_saved_query(root: &Path, name: &str) -> Result<bool> {
    let mut store = load_query_store(root)?;
    let before = store.queries.len();
    store.queries.retain(|query| query.name != name);
    let deleted = store.queries.len() != before;
    if deleted {
        write_query_store(root, &store)?;
    }
    Ok(deleted)
}

pub fn run_with_config(
    root: &Path,
    expression: &str,
    source: QuerySource,
    limit: usize,
    lsp_config: Option<&LspConfig>,
) -> Result<Vec<QueryMatch>> {
    run_with_options(root, expression, source, limit, lsp_config, &QueryRunOptions::default())
}

pub fn run_with_options(
    root: &Path,
    expression: &str,
    source: QuerySource,
    limit: usize,
    lsp_config: Option<&LspConfig>,
    options: &QueryRunOptions,
) -> Result<Vec<QueryMatch>> {
    let expanded = expand_query_macros(expression, &options.macros)?;
    validate_expression_for_mode(&expanded, options.mode)?;
    let parsed = parse_expression(&expanded);
    let mut matches = Vec::new();

    if source.includes_index() {
        matches.extend(query_index_with_options(root, &parsed, options)?);
    }
    if source.includes_trace() {
        matches.extend(query_trace_with_options(root, &parsed, options)?);
    }
    if source.includes_lsp() {
        match lsp_config {
            Some(config) => match query_lsp_with_options(root, &parsed, &expanded, config, options) {
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
                Err(_) if source == QuerySource::Semantic => {
                    matches.extend(semantic_fallback_matches_with_options(
                        root,
                        &parsed,
                        "semantic unavailable",
                        options,
                    )?);
                }
                Err(err) => return Err(err),
            },
            None if source == QuerySource::Auto => {}
            None if source == QuerySource::Semantic => {
                matches.extend(semantic_fallback_matches_with_options(
                    root,
                    &parsed,
                    "semantic config missing",
                    options,
                )?);
            }
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
    let mut matches = dedup_matches(matches, limit.max(1));
    if options.score_explain {
        for item in &mut matches {
            item.detail = format!("{}; score={} mode={}", item.detail, item.score, options.mode.as_str());
        }
    }
    Ok(matches)
}

fn semantic_fallback_matches_with_options(
    root: &Path,
    expression: &QueryExpression,
    reason: &str,
    options: &QueryRunOptions,
) -> Result<Vec<QueryMatch>> {
    let mut fallback = query_index_with_options(root, expression, options)?;
    for item in &mut fallback {
        item.source = format!("fallback:{}", item.source);
        item.detail = format!("{reason}; {}", item.detail);
    }
    if fallback.is_empty() {
        fallback.push(QueryMatch {
            source: "fallback:index:empty".to_string(),
            path: root.to_path_buf(),
            line: None,
            column: None,
            label: "semantic query unavailable".to_string(),
            kind: "status".to_string(),
            detail: format!("{reason}; no index fallback results"),
            score: usize::MAX.saturating_sub(1),
        });
    }
    Ok(fallback)
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

fn query_index_with_options(
    root: &Path,
    expression: &QueryExpression,
    options: &QueryRunOptions,
) -> Result<Vec<QueryMatch>> {
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
        if let Some(score) = score_candidate_with_mode(&candidate, expression, options.mode)? {
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
        if let Some(score) = score_candidate_with_mode(&candidate, expression, options.mode)? {
            matches.push(candidate.into_match(score));
        }
    }

    Ok(matches)
}

fn query_trace_with_options(
    root: &Path,
    expression: &QueryExpression,
    options: &QueryRunOptions,
) -> Result<Vec<QueryMatch>> {
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
        if let Some(score) = score_candidate_with_mode(&candidate, expression, options.mode)? {
            matches.push(candidate.into_match(score));
        }
    }

    Ok(matches)
}

fn query_lsp_with_options(
    root: &Path,
    expression: &QueryExpression,
    original_expression: &str,
    config: &LspConfig,
    options: &QueryRunOptions,
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
        if let Some(score) = score_candidate_with_mode(&candidate, expression, options.mode)? {
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

fn validate_expression_for_mode(expression: &str, mode: QueryMode) -> Result<()> {
    if mode != QueryMode::Regex {
        return Ok(());
    }

    let parsed = parse_expression(expression);
    validate_regex_node(&parsed.root)
}

fn validate_regex_node(node: &QueryNode) -> Result<()> {
    match node {
        QueryNode::All => Ok(()),
        QueryNode::Term(term) => validate_regex_pattern(term),
        QueryNode::Field { value, .. } => validate_regex_pattern(value),
        QueryNode::Not(child) => validate_regex_node(child),
        QueryNode::And(children) | QueryNode::Or(children) => {
            for child in children {
                validate_regex_node(child)?;
            }
            Ok(())
        }
    }
}

fn validate_regex_pattern(pattern: &str) -> Result<()> {
    build_query_regex(pattern)
        .map(|_| ())
        .map_err(|err| AppError::General(format!("Invalid query regex `{pattern}`: {err}")))
}

fn builtin_query_macros() -> Vec<(&'static str, &'static str)> {
    vec![
        ("@functions", "kind:function"),
        ("@function", "kind:function"),
        ("@structs", "(kind:struct OR kind:class OR kind:interface)"),
        ("@tests", "(path:test OR path:tests OR name:test OR name:should)"),
        ("@todo", "(text:TODO OR text:FIXME OR text:todo)"),
        ("@rust", "lang:rust"),
        ("@c", "(lang:c OR lang:cpp OR lang:h)"),
        ("@debug", "(tag:debug OR source:trace OR kind:breakpoint)"),
    ]
}

fn query_store_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join("queries.toml"))
}

fn load_query_store(root: &Path) -> Result<SavedQueryStore> {
    let path = query_store_path(root)?;
    if !path.exists() {
        return Ok(SavedQueryStore::default());
    }
    let contents = fs::read_to_string(&path)?;
    toml::from_str(&contents).map_err(|err| AppError::General(format!("Corrupt saved query store: {err}")))
}

fn write_query_store(root: &Path, store: &SavedQueryStore) -> Result<()> {
    let path = query_store_path(root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(store).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn validate_saved_query_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(AppError::General("Saved query name cannot be empty".to_string()));
    }
    if name.chars().any(char::is_whitespace) {
        return Err(AppError::General(
            "Saved query name cannot contain whitespace".to_string(),
        ));
    }
    Ok(())
}

fn lsp_query_text(expression: &QueryExpression, original_expression: &str) -> String {
    let mut terms = Vec::new();
    collect_lsp_query_terms(&expression.root, false, &mut terms);
    if terms.is_empty() {
        terms.extend(expression.terms.clone());
        if let Some(names) = expression.fields.get("name") {
            terms.extend(names.clone());
        }
        if let Some(text_values) = expression.fields.get("text") {
            terms.extend(text_values.clone());
        }
    }
    let mut seen = BTreeSet::new();
    let terms = terms
        .into_iter()
        .filter(|term| !term.trim().is_empty())
        .filter(|term| seen.insert(term.clone()))
        .collect::<Vec<String>>();
    if !terms.is_empty() {
        return terms.join(" ");
    }
    original_expression
        .split_whitespace()
        .filter(|token| !token.contains(':'))
        .collect::<Vec<&str>>()
        .join(" ")
}

fn collect_lsp_query_terms(node: &QueryNode, negated: bool, terms: &mut Vec<String>) {
    match node {
        QueryNode::All => {}
        QueryNode::Term(term) if !negated => terms.push(term.clone()),
        QueryNode::Term(_) => {}
        QueryNode::Field { field, value } if !negated && (field == "name" || field == "text") => {
            terms.push(value.clone());
        }
        QueryNode::Field { .. } => {}
        QueryNode::Not(child) => collect_lsp_query_terms(child, !negated, terms),
        QueryNode::And(children) | QueryNode::Or(children) => {
            for child in children {
                collect_lsp_query_terms(child, negated, terms);
            }
        }
    }
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
    if matches!(expression.root, QueryNode::All) && (!expression.terms.is_empty() || !expression.fields.is_empty()) {
        return score_flat_expression(candidate, expression);
    }
    score_node(candidate, &expression.root)
}

fn score_candidate_with_mode(
    candidate: &Candidate<'_>,
    expression: &QueryExpression,
    mode: QueryMode,
) -> Result<Option<usize>> {
    if mode == QueryMode::Fuzzy {
        return Ok(score_candidate(candidate, expression));
    }
    if matches!(expression.root, QueryNode::All) && (!expression.terms.is_empty() || !expression.fields.is_empty()) {
        return score_flat_expression_with_mode(candidate, expression, mode);
    }
    score_node_with_mode(candidate, &expression.root, mode)
}

fn score_flat_expression_with_mode(
    candidate: &Candidate<'_>,
    expression: &QueryExpression,
    mode: QueryMode,
) -> Result<Option<usize>> {
    let mut score = 0;

    for term in &expression.terms {
        let Some(term_score) = match_term(candidate, term, mode)? else {
            return Ok(None);
        };
        score += term_score;
    }

    for (field, values) in &expression.fields {
        for value in values {
            let Some(field_score) = match_field(candidate, field, value, mode)? else {
                return Ok(None);
            };
            score += field_score;
        }
    }

    Ok(Some(score))
}

fn score_flat_expression(candidate: &Candidate<'_>, expression: &QueryExpression) -> Option<usize> {
    let mut score = 0;

    for term in &expression.terms {
        score += score_term(candidate, term)?;
    }

    for (field, values) in &expression.fields {
        for value in values {
            score += score_field(candidate, field, value)?;
        }
    }

    Some(score)
}

fn score_node(candidate: &Candidate<'_>, node: &QueryNode) -> Option<usize> {
    match node {
        QueryNode::All => Some(0),
        QueryNode::Term(term) => score_term(candidate, term),
        QueryNode::Field { field, value } => score_field(candidate, field, value),
        QueryNode::Not(child) => {
            if score_node(candidate, child).is_some() {
                None
            } else {
                Some(0)
            }
        }
        QueryNode::And(children) => {
            let mut score = 0;
            for child in children {
                score += score_node(candidate, child)?;
            }
            Some(score)
        }
        QueryNode::Or(children) => children.iter().filter_map(|child| score_node(candidate, child)).min(),
    }
}

fn score_node_with_mode(candidate: &Candidate<'_>, node: &QueryNode, mode: QueryMode) -> Result<Option<usize>> {
    if mode == QueryMode::Fuzzy {
        return Ok(score_node(candidate, node));
    }

    match node {
        QueryNode::All => Ok(Some(0)),
        QueryNode::Term(term) => match_term(candidate, term, mode),
        QueryNode::Field { field, value } => match_field(candidate, field, value, mode),
        QueryNode::Not(child) => {
            if score_node_with_mode(candidate, child, mode)?.is_some() {
                Ok(None)
            } else {
                Ok(Some(0))
            }
        }
        QueryNode::And(children) => {
            let mut score = 0;
            for child in children {
                let Some(child_score) = score_node_with_mode(candidate, child, mode)? else {
                    return Ok(None);
                };
                score += child_score;
            }
            Ok(Some(score))
        }
        QueryNode::Or(children) => {
            let mut best = None;
            for child in children {
                if let Some(score) = score_node_with_mode(candidate, child, mode)? {
                    best = Some(best.map_or(score, |current: usize| current.min(score)));
                }
            }
            Ok(best)
        }
    }
}

fn match_term(candidate: &Candidate<'_>, term: &str, mode: QueryMode) -> Result<Option<usize>> {
    match mode {
        QueryMode::Fuzzy => Ok(score_term(candidate, term)),
        QueryMode::Exact => Ok(exact_term_score(candidate, term)),
        QueryMode::Regex => regex_score(&candidate.haystack(), term),
    }
}

fn match_field(candidate: &Candidate<'_>, field: &str, value: &str, mode: QueryMode) -> Result<Option<usize>> {
    match mode {
        QueryMode::Fuzzy => Ok(score_field(candidate, field, value)),
        QueryMode::Exact => Ok(exact_field_score(candidate, field, value)),
        QueryMode::Regex => regex_field_score(candidate, field, value),
    }
}

fn score_term(candidate: &Candidate<'_>, term: &str) -> Option<usize> {
    let haystack = candidate.haystack().to_ascii_lowercase();
    haystack.find(&term.to_ascii_lowercase())
}

fn score_field(candidate: &Candidate<'_>, field: &str, value: &str) -> Option<usize> {
    let value = value.to_ascii_lowercase();
    field_value(candidate, field)
        .iter()
        .filter_map(|candidate_value| candidate_value.find(&value))
        .min()
}

fn exact_term_score(candidate: &Candidate<'_>, term: &str) -> Option<usize> {
    let needle = term.to_ascii_lowercase();
    candidate
        .haystack()
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.'))
        .position(|token| token.eq_ignore_ascii_case(&needle))
}

fn exact_field_score(candidate: &Candidate<'_>, field: &str, value: &str) -> Option<usize> {
    field_value(candidate, field)
        .iter()
        .position(|candidate_value| candidate_value.eq_ignore_ascii_case(value))
}

fn regex_score(haystack: &str, pattern: &str) -> Result<Option<usize>> {
    let regex = build_query_regex(pattern)
        .map_err(|err| AppError::General(format!("Invalid query regex `{pattern}`: {err}")))?;
    Ok(regex.find(haystack).map(|matched| matched.start()))
}

fn regex_field_score(candidate: &Candidate<'_>, field: &str, pattern: &str) -> Result<Option<usize>> {
    let regex = build_query_regex(pattern)
        .map_err(|err| AppError::General(format!("Invalid query regex `{pattern}`: {err}")))?;
    Ok(field_value(candidate, field)
        .iter()
        .filter_map(|candidate_value| regex.find(candidate_value).map(|matched| matched.start()))
        .min())
}

fn build_query_regex(pattern: &str) -> std::result::Result<Regex, regex::Error> {
    RegexBuilder::new(pattern).case_insensitive(true).build()
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
        if !quoted && (ch == '(' || ch == ')') {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(ch.to_string());
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

    fn test_candidate(source: &'static str, path: &str, label: &str, kind: &str, language: &str) -> Candidate<'static> {
        Candidate {
            source,
            path: PathBuf::from(path),
            line: Some(3),
            column: None,
            label: label.to_string(),
            kind: kind.to_string(),
            language: language.to_string(),
            name: label.to_string(),
            detail: format!("{kind} {label}"),
            status: None,
            priority: None,
            session: None,
            tags: Vec::new(),
        }
    }

    fn temp_query_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fcs_query_{name}_{}", std::process::id()))
    }

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
    fn parses_grouped_or_not_query() {
        let parsed = parse_expression("kind:function (name:main or name:init) not path:target");

        assert!(parsed.terms.is_empty());
        assert_eq!(parsed.fields["kind"], vec!["function".to_string()]);
        assert_eq!(parsed.fields["name"], vec!["main".to_string(), "init".to_string()]);
        assert_eq!(parsed.fields["path"], vec!["target".to_string()]);
        assert_eq!(
            render_query_plan(&parsed.root),
            "AND(kind:function, OR(name:main, name:init), NOT(path:target))"
        );
    }

    #[test]
    fn parses_semantic_and_auto_sources() {
        assert_eq!(QuerySource::parse("semantic").unwrap(), QuerySource::Semantic);
        assert_eq!(QuerySource::parse("lsp").unwrap(), QuerySource::Semantic);
        assert_eq!(QuerySource::parse("auto").unwrap(), QuerySource::Auto);
        assert!(QuerySource::parse("remote").is_err());
    }

    #[test]
    fn parses_query_modes_and_aliases() {
        assert_eq!(QueryMode::parse("fuzzy").unwrap(), QueryMode::Fuzzy);
        assert_eq!(QueryMode::parse("substring").unwrap(), QueryMode::Fuzzy);
        assert_eq!(QueryMode::parse("exact").unwrap(), QueryMode::Exact);
        assert_eq!(QueryMode::parse("regexp").unwrap(), QueryMode::Regex);
        assert!(QueryMode::parse("phonetic").is_err());
    }

    #[test]
    fn expands_builtin_query_macros() {
        let expanded = expand_query_macros(
            "panic",
            &["functions".to_string(), "@rust".to_string(), "todo".to_string()],
        )
        .unwrap();

        assert!(expanded.contains("panic"));
        assert!(expanded.contains("kind:function"));
        assert!(expanded.contains("lang:rust"));
        assert!(expanded.contains("TODO"));
        assert!(expand_query_macros("main @missing", &[]).is_err());
    }

    #[test]
    fn semantic_source_falls_back_to_index_without_config() {
        let temp_dir = std::env::temp_dir().join(format!("fcs_query_semantic_fallback_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();
        std::fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        std::fs::write(temp_dir.join("src").join("main.rs"), "pub fn main() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");
        crate::index::build(&temp_dir, &[], &[], &ignore_file).unwrap();

        let matches = run(&temp_dir, "kind:function name:main", QuerySource::Semantic, 10).unwrap();

        assert!(matches
            .iter()
            .any(|item| { item.source == "fallback:index:symbol" && item.detail.contains("semantic config missing") }));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn semantic_source_reports_empty_fallback_without_config() {
        let temp_dir = std::env::temp_dir().join(format!("fcs_query_semantic_empty_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let matches = run(&temp_dir, "name:missing", QuerySource::Semantic, 10).unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].source, "fallback:index:empty");
        assert!(matches[0].detail.contains("semantic config missing"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn lsp_query_text_uses_positive_or_terms() {
        let parsed = parse_expression("kind:function (name:main or name:init) not path:target not name:skip");

        assert_eq!(lsp_query_text(&parsed, ""), "main init");
    }

    #[test]
    fn scores_candidate_with_field_filters() {
        let expression = parse_expression("kind:function lang:rust source:index main");
        let candidate = test_candidate("index:symbol", "src/main.rs", "main", "function", "rust");

        assert!(score_candidate(&candidate, &expression).is_some());
        assert!(score_candidate(&candidate, &parse_expression("source:trace main")).is_none());
        assert!(score_candidate(&candidate, &parse_expression("kind:struct main")).is_none());
    }

    #[test]
    fn scores_exact_and_regex_modes() {
        let candidate = test_candidate("index:symbol", "src/main.rs", "smoke_added_symbol", "function", "rust");

        assert!(score_candidate_with_mode(
            &candidate,
            &parse_expression("name:smoke_added_symbol"),
            QueryMode::Exact
        )
        .unwrap()
        .is_some());
        assert!(
            score_candidate_with_mode(&candidate, &parse_expression("name:smoke_added"), QueryMode::Exact)
                .unwrap()
                .is_none()
        );
        assert!(score_candidate_with_mode(
            &candidate,
            &parse_expression(r#"path:src/.*\.rs name:smoke_.*_symbol"#),
            QueryMode::Regex
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn rejects_invalid_regex_mode_patterns() {
        assert!(validate_expression_for_mode("name:[", QueryMode::Regex).is_err());
        assert!(validate_expression_for_mode("name:main", QueryMode::Regex).is_ok());
    }

    #[test]
    fn scores_legacy_flat_expression_without_ast() {
        let mut fields = BTreeMap::new();
        fields.insert("kind".to_string(), vec!["function".to_string()]);
        let expression = QueryExpression {
            terms: vec!["main".to_string()],
            fields,
            root: QueryNode::All,
        };
        let candidate = test_candidate("index:symbol", "src/main.rs", "main", "function", "rust");

        assert!(score_candidate(&candidate, &expression).is_some());
    }

    #[test]
    fn scores_candidate_with_grouped_or_and_not_filters() {
        let expression = parse_expression("kind:function (name:main or name:init) not path:target");
        let main_candidate = test_candidate("index:symbol", "src/main.rs", "main", "function", "rust");
        let init_candidate = test_candidate("index:symbol", "src/init.rs", "init", "function", "rust");
        let target_candidate = test_candidate("index:symbol", "target/main.rs", "main", "function", "rust");
        let other_candidate = test_candidate("index:symbol", "src/start.rs", "start", "function", "rust");

        assert!(score_candidate(&main_candidate, &expression).is_some());
        assert!(score_candidate(&init_candidate, &expression).is_some());
        assert!(score_candidate(&target_candidate, &expression).is_none());
        assert!(score_candidate(&other_candidate, &expression).is_none());
        assert!(score_candidate(&main_candidate, &parse_expression("source:trace or source:index")).is_some());
    }

    #[test]
    fn saved_queries_round_trip_and_delete() {
        let temp_dir = temp_query_dir("saved_round_trip");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        save_query(&temp_dir, "zeta", "name:beta", QuerySource::Index, QueryMode::Exact).unwrap();
        save_query(
            &temp_dir,
            "alpha",
            "@functions name:main",
            QuerySource::Auto,
            QueryMode::Fuzzy,
        )
        .unwrap();

        let saved = list_saved_queries(&temp_dir).unwrap();
        let loaded = load_saved_query(&temp_dir, "alpha").unwrap();

        assert_eq!(
            saved.iter().map(|query| query.name.as_str()).collect::<Vec<&str>>(),
            vec!["alpha", "zeta"]
        );
        assert_eq!(loaded.expression, "@functions name:main");
        assert_eq!(loaded.source, "auto");
        assert_eq!(loaded.mode, QueryMode::Fuzzy);
        assert!(save_query(&temp_dir, "bad name", "main", QuerySource::Index, QueryMode::Fuzzy).is_err());
        assert!(delete_saved_query(&temp_dir, "zeta").unwrap());
        assert!(!delete_saved_query(&temp_dir, "zeta").unwrap());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn run_with_options_appends_score_explanation() {
        let temp_dir = temp_query_dir("score_explain");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "pub fn main() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");
        crate::index::build(&temp_dir, &[], &[], &ignore_file).unwrap();

        let matches = run_with_options(
            &temp_dir,
            "kind:function name:main.*",
            QuerySource::Index,
            10,
            None,
            &QueryRunOptions {
                mode: QueryMode::Regex,
                macros: Vec::new(),
                score_explain: true,
            },
        )
        .unwrap();

        assert!(matches.iter().any(|item| {
            item.detail.contains("main [function]")
                && item.detail.contains("score=")
                && item.detail.contains("mode=regex")
        }));

        let _ = fs::remove_dir_all(&temp_dir);
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
        assert_eq!(explanation.execution_plan, "AND(source:trace, status:open, text:panic)");
        assert!(explanation
            .filters
            .iter()
            .any(|filter| filter == "require source:trace"));
        assert!(text.contains("execution_plan: AND(source:trace, status:open, text:panic)"));
        assert!(text.contains("filters:"));
        assert!(text.contains("supported_fields"));
    }

    #[test]
    fn explains_grouped_filters() {
        let explanation = explain(
            "kind:function (name:main or name:init) not path:target",
            QuerySource::All,
        );
        let text = format_explanation(&explanation, "text").unwrap();

        assert_eq!(
            explanation.execution_plan,
            "AND(kind:function, OR(name:main, name:init), NOT(path:target))"
        );
        assert!(explanation
            .filters
            .iter()
            .any(|filter| filter == "any of: name:main OR name:init"));
        assert!(explanation.filters.iter().any(|filter| filter == "exclude path:target"));
        assert!(text.contains("execution_plan: AND(kind:function, OR(name:main, name:init), NOT(path:target))"));
        assert!(text.contains("  - any of: name:main OR name:init"));
        assert!(text.contains("  - exclude path:target"));
    }
}
