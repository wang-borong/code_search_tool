use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::core::CodeItem;
use crate::errors::{AppError, Result};

const INDEX_VERSION: u32 = 2;
const INDEX_FILE_NAME: &str = "code_index.toml";
const INDEX_TMP_EXTENSION: &str = "tmp";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeIndex {
    pub version: u32,
    pub root: String,
    pub built_at_unix: u64,
    pub options: IndexOptionsSnapshot,
    pub files: Vec<IndexedFile>,
    pub symbols: Vec<IndexedSymbol>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexOptionsSnapshot {
    #[serde(default)]
    pub file_options: Vec<String>,
    #[serde(default)]
    pub default_ignore: Vec<String>,
    #[serde(default)]
    pub ignore_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexedFile {
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix: u64,
    #[serde(default)]
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexedSymbol {
    pub path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub label: String,
    pub detail: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub range: IndexedSymbolRange,
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexedSymbolRange {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexBuildReport {
    pub path: PathBuf,
    pub file_count: usize,
    pub symbol_count: usize,
    pub unchanged_files: usize,
    pub changed_files: usize,
    pub removed_files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexSchemaStatus {
    Missing,
    Current,
    Migrated,
    Future,
    Corrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStatus {
    pub path: PathBuf,
    pub exists: bool,
    pub version: Option<u32>,
    pub schema_status: IndexSchemaStatus,
    pub is_stale: bool,
    pub is_corrupt: bool,
    pub message: Option<String>,
    pub file_count: usize,
    pub symbol_count: usize,
    pub built_at_unix: Option<u64>,
    pub changed_tracked_files: usize,
    pub missing_tracked_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexCount {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStats {
    pub path: PathBuf,
    pub exists: bool,
    pub file_count: usize,
    pub symbol_count: usize,
    pub source_size_bytes: u64,
    pub index_size_bytes: u64,
    pub built_at_unix: Option<u64>,
    pub languages: Vec<IndexCount>,
    pub symbol_kinds: Vec<IndexCount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexCompactReport {
    pub path: PathBuf,
    pub dry_run: bool,
    pub original_bytes: u64,
    pub compacted_bytes: u64,
    pub size_delta_bytes: i64,
    pub wrote: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPrewarmReport {
    pub path: PathBuf,
    pub loaded: bool,
    pub file_count: usize,
    pub symbol_count: usize,
    pub bytes_touched: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRefreshReport {
    pub path: PathBuf,
    pub rebuilt: bool,
    pub reason: String,
    pub build_report: Option<IndexBuildReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexListKind {
    Files,
    Symbols,
}

impl IndexListKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "files" | "file" => Ok(Self::Files),
            "symbols" | "symbol" => Ok(Self::Symbols),
            other => Err(AppError::General(format!(
                "Unsupported index list kind: {other}. Use files or symbols"
            ))),
        }
    }
}

impl CodeIndex {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.symbols.is_empty()
    }
}

pub fn build(
    root: &Path,
    file_options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<IndexBuildReport> {
    let root = normalize_root(root);
    let previous = load_recoverable(&root)?;
    let previous_files = previous
        .as_ref()
        .map(|index| file_metadata_by_path(&index.files))
        .unwrap_or_default();
    let previous_symbols = previous
        .as_ref()
        .map(|index| symbols_by_path(&index.symbols))
        .unwrap_or_default();
    let previous_paths = previous_files.keys().cloned().collect::<HashSet<String>>();
    let root_arg = root.to_string_lossy().to_string();
    let file_items = crate::files::find_files(Some(&root_arg), file_options, default_ignore, ignore_file)?;
    let files = index_files(&root, &file_items)?;
    let current_paths = files.iter().map(|file| file.path.clone()).collect::<HashSet<String>>();
    let unchanged_files = files
        .iter()
        .filter(|file| {
            previous_files
                .get(&file.path)
                .is_some_and(|previous| is_same_file_snapshot(previous, file))
        })
        .count();
    let removed_files = previous_paths.difference(&current_paths).count();
    let changed_files = files.len().saturating_sub(unchanged_files);
    let changed_paths = files
        .iter()
        .filter(|file| {
            !previous_files
                .get(&file.path)
                .is_some_and(|previous| is_same_file_snapshot(previous, file))
        })
        .map(|file| file.path.clone())
        .collect::<HashSet<String>>();
    let mut symbols = index_symbols_incremental(
        &root,
        &files,
        &changed_paths,
        &previous_symbols,
        file_options,
        default_ignore,
        ignore_file,
    )?;
    finalize_symbol_metadata(&mut symbols);
    let index = CodeIndex {
        version: INDEX_VERSION,
        root: root.to_string_lossy().to_string(),
        built_at_unix: now_unix(),
        options: IndexOptionsSnapshot {
            file_options: file_options.to_vec(),
            default_ignore: default_ignore.to_vec(),
            ignore_file: ignore_file.to_string_lossy().to_string(),
        },
        files,
        symbols,
    };
    let path = index_path(&root)?;
    write_index(&path, &index)?;

    Ok(IndexBuildReport {
        path,
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        unchanged_files,
        changed_files,
        removed_files,
    })
}

pub fn status(root: &Path) -> Result<IndexStatus> {
    let root = normalize_root(root);
    let path = index_path(&root)?;
    if !path.exists() {
        return Ok(IndexStatus {
            path,
            exists: false,
            version: None,
            schema_status: IndexSchemaStatus::Missing,
            is_stale: false,
            is_corrupt: false,
            message: None,
            file_count: 0,
            symbol_count: 0,
            built_at_unix: None,
            changed_tracked_files: 0,
            missing_tracked_files: 0,
        });
    }

    let read_state = read_index_state(&path)?;
    let (index, schema_status, mut message) = match read_state {
        IndexReadState::Ready { index, schema_status } => (index, schema_status, None),
        IndexReadState::Future { version } => {
            return Ok(IndexStatus {
                path,
                exists: true,
                version: Some(version),
                schema_status: IndexSchemaStatus::Future,
                is_stale: true,
                is_corrupt: false,
                message: Some(format!(
                    "Index schema {version} is newer than supported schema {INDEX_VERSION}"
                )),
                file_count: 0,
                symbol_count: 0,
                built_at_unix: None,
                changed_tracked_files: 0,
                missing_tracked_files: 0,
            });
        }
        IndexReadState::Corrupt { error } => {
            return Ok(IndexStatus {
                path,
                exists: true,
                version: None,
                schema_status: IndexSchemaStatus::Corrupt,
                is_stale: true,
                is_corrupt: true,
                message: Some(error),
                file_count: 0,
                symbol_count: 0,
                built_at_unix: None,
                changed_tracked_files: 0,
                missing_tracked_files: 0,
            });
        }
    };
    let mut changed_tracked_files = 0;
    let mut missing_tracked_files = 0;
    let mut root_mismatch = false;
    if !index.root.is_empty() && normalize_root(Path::new(&index.root)) != root {
        root_mismatch = true;
        message = Some("Index root does not match current workspace root".to_string());
    }

    for file in &index.files {
        let path = indexed_path(&root, &file.path);
        match file_metadata(&root, &path) {
            Ok(current) if is_same_file_snapshot(&current, file) => {}
            Ok(_) => changed_tracked_files += 1,
            Err(_) => missing_tracked_files += 1,
        }
    }

    Ok(IndexStatus {
        path,
        exists: true,
        version: Some(index.version),
        schema_status,
        is_stale: schema_status != IndexSchemaStatus::Current
            || root_mismatch
            || changed_tracked_files > 0
            || missing_tracked_files > 0,
        is_corrupt: false,
        message,
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        built_at_unix: Some(index.built_at_unix),
        changed_tracked_files,
        missing_tracked_files,
    })
}

pub fn load(root: &Path) -> Result<Option<CodeIndex>> {
    let path = index_path(root)?;
    if !path.exists() {
        return Ok(None);
    }

    read_index(&path).map(Some)
}

pub fn stats(root: &Path) -> Result<IndexStats> {
    let root = normalize_root(root);
    let path = index_path(&root)?;
    let index_size_bytes = fs::metadata(&path).ok().map_or(0, |metadata| metadata.len());
    let Some(index) = load(&root)? else {
        return Ok(IndexStats {
            path,
            exists: false,
            file_count: 0,
            symbol_count: 0,
            source_size_bytes: 0,
            index_size_bytes,
            built_at_unix: None,
            languages: Vec::new(),
            symbol_kinds: Vec::new(),
        });
    };

    Ok(IndexStats {
        path,
        exists: true,
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        source_size_bytes: index.files.iter().map(|file| file.size_bytes).sum(),
        index_size_bytes,
        built_at_unix: Some(index.built_at_unix),
        languages: count_values(index.files.iter().map(|file| file.language.as_str())),
        symbol_kinds: count_values(index.symbols.iter().map(|symbol| symbol.kind.as_str())),
    })
}

pub fn compact(root: &Path, dry_run: bool) -> Result<IndexCompactReport> {
    let root = normalize_root(root);
    let path = index_path(&root)?;
    let original = fs::read_to_string(&path)?;
    let index = migrate_index_contents(&original)?;
    let compacted = toml::to_string(&index).map_err(|err| AppError::General(err.to_string()))?;
    let original_bytes = original.len() as u64;
    let compacted_bytes = compacted.len() as u64;

    if !dry_run {
        write_index_contents(&path, &compacted)?;
    }

    Ok(IndexCompactReport {
        path,
        dry_run,
        original_bytes,
        compacted_bytes,
        size_delta_bytes: compacted_bytes as i64 - original_bytes as i64,
        wrote: !dry_run,
    })
}

pub fn prewarm(root: &Path) -> Result<IndexPrewarmReport> {
    let root = normalize_root(root);
    let path = index_path(&root)?;
    let Some(index) = load(&root)? else {
        return Ok(IndexPrewarmReport {
            path,
            loaded: false,
            file_count: 0,
            symbol_count: 0,
            bytes_touched: 0,
        });
    };

    let bytes_touched = index
        .files
        .iter()
        .map(|file| file.path.len() as u64 + file.language.len() as u64 + file.size_bytes)
        .sum::<u64>()
        + index
            .symbols
            .iter()
            .map(|symbol| {
                symbol.path.len() as u64
                    + symbol.label.len() as u64
                    + symbol.detail.len() as u64
                    + symbol.name.len() as u64
                    + symbol.kind.len() as u64
            })
            .sum::<u64>();

    Ok(IndexPrewarmReport {
        path,
        loaded: true,
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        bytes_touched,
    })
}

pub fn refresh(
    root: &Path,
    file_options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<IndexRefreshReport> {
    let root = normalize_root(root);
    let status = status(&root)?;
    let needs_refresh = !status.exists
        || status.is_stale
        || status.is_corrupt
        || status.changed_tracked_files > 0
        || status.missing_tracked_files > 0;
    if !needs_refresh {
        return Ok(IndexRefreshReport {
            path: status.path,
            rebuilt: false,
            reason: "fresh".to_string(),
            build_report: None,
        });
    }

    let reason = refresh_reason(&status);
    let build_report = build(&root, file_options, default_ignore, ignore_file)?;
    Ok(IndexRefreshReport {
        path: build_report.path.clone(),
        rebuilt: true,
        reason,
        build_report: Some(build_report),
    })
}

pub fn migrate_index_contents(contents: &str) -> Result<CodeIndex> {
    match parse_index_contents(contents) {
        IndexReadState::Ready { index, .. } => Ok(index),
        IndexReadState::Future { version } => Err(AppError::General(format!(
            "Index schema {version} is newer than supported schema {INDEX_VERSION}"
        ))),
        IndexReadState::Corrupt { error } => Err(AppError::General(error)),
    }
}

pub fn needs_schema_migration(contents: &str) -> Result<bool> {
    let value = contents
        .parse::<toml::Value>()
        .map_err(|err| AppError::General(format!("Corrupt index TOML: {err}")))?;
    let version = value.get("version").and_then(toml::Value::as_integer).unwrap_or(1);

    if version < 0 {
        return Err(AppError::General("Index schema version cannot be negative".to_string()));
    }

    Ok(version as u32 != INDEX_VERSION)
}

pub fn list(root: &Path, kind: IndexListKind, limit: usize) -> Result<Vec<String>> {
    let Some(index) = load(root)? else {
        return Ok(Vec::new());
    };
    let limit = limit.max(1);
    let entries = match kind {
        IndexListKind::Files => index
            .files
            .iter()
            .take(limit)
            .map(|file| format!("{} [{}] ({} bytes)", file.path, file.language, file.size_bytes))
            .collect(),
        IndexListKind::Symbols => index.symbols.iter().take(limit).map(format_symbol_entry).collect(),
    };

    Ok(entries)
}

pub fn query(root: &Path, kind: IndexListKind, query: &str, limit: usize) -> Result<Vec<String>> {
    let Some(index) = load(root)? else {
        return Ok(Vec::new());
    };
    let query = query.trim();
    if query.is_empty() {
        return list(root, kind, limit);
    }

    let mut scored = match kind {
        IndexListKind::Files => index
            .files
            .iter()
            .filter_map(|file| fuzzy_score(&file.path, query).map(|score| (score, file.path.clone())))
            .collect::<Vec<(usize, String)>>(),
        IndexListKind::Symbols => index
            .symbols
            .iter()
            .filter_map(|symbol| {
                let entry = format_symbol_entry(symbol);
                let haystack = format!("{} {} {} {}", symbol.path, symbol.name, symbol.kind, symbol.detail);
                fuzzy_score(&haystack, query).map(|score| (score, entry))
            })
            .collect::<Vec<(usize, String)>>(),
    };
    scored.sort_by_key(|(score, entry)| (*score, entry.clone()));

    Ok(scored.into_iter().take(limit.max(1)).map(|(_, entry)| entry).collect())
}

pub fn index_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(INDEX_FILE_NAME))
}

fn write_index(path: &Path, index: &CodeIndex) -> Result<()> {
    let contents = toml::to_string_pretty(index).map_err(|err| AppError::General(err.to_string()))?;
    write_index_contents(path, &contents)
}

fn write_index_contents(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension(INDEX_TMP_EXTENSION);
    fs::write(&tmp_path, contents)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

fn read_index(path: &Path) -> Result<CodeIndex> {
    let contents = fs::read_to_string(path)?;
    migrate_index_contents(&contents)
}

fn load_recoverable(root: &Path) -> Result<Option<CodeIndex>> {
    let path = index_path(root)?;
    if !path.exists() {
        return Ok(None);
    }

    match read_index_state(&path)? {
        IndexReadState::Ready { index, .. } => Ok(Some(index)),
        IndexReadState::Future { .. } | IndexReadState::Corrupt { .. } => Ok(None),
    }
}

enum IndexReadState {
    Ready {
        index: CodeIndex,
        schema_status: IndexSchemaStatus,
    },
    Future {
        version: u32,
    },
    Corrupt {
        error: String,
    },
}

#[derive(Debug, Deserialize)]
struct CodeIndexCompat {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    root: String,
    #[serde(default)]
    built_at_unix: u64,
    #[serde(default)]
    options: IndexOptionsSnapshot,
    #[serde(default)]
    files: Vec<IndexedFile>,
    #[serde(default)]
    symbols: Vec<IndexedSymbol>,
}

fn read_index_state(path: &Path) -> Result<IndexReadState> {
    let contents = fs::read_to_string(path)?;
    Ok(parse_index_contents(&contents))
}

fn parse_index_contents(contents: &str) -> IndexReadState {
    let parsed = match toml::from_str::<CodeIndexCompat>(contents) {
        Ok(index) => index,
        Err(err) => {
            return IndexReadState::Corrupt {
                error: format!("Corrupt index TOML: {err}"),
            };
        }
    };

    let source_version = if parsed.version == 0 { 1 } else { parsed.version };
    if source_version > INDEX_VERSION {
        return IndexReadState::Future {
            version: source_version,
        };
    }

    let mut index = CodeIndex {
        version: INDEX_VERSION,
        root: parsed.root,
        built_at_unix: parsed.built_at_unix,
        options: parsed.options,
        files: parsed
            .files
            .into_iter()
            .map(migrate_indexed_file)
            .collect::<Vec<IndexedFile>>(),
        symbols: parsed.symbols,
    };
    finalize_symbol_metadata(&mut index.symbols);

    let schema_status = if source_version == INDEX_VERSION {
        IndexSchemaStatus::Current
    } else {
        IndexSchemaStatus::Migrated
    };

    IndexReadState::Ready { index, schema_status }
}

fn migrate_indexed_file(mut file: IndexedFile) -> IndexedFile {
    if file.language.is_empty() {
        file.language = language_for_path(Path::new(&file.path));
    }
    file
}

fn index_symbols_incremental(
    root: &Path,
    files: &[IndexedFile],
    changed_paths: &HashSet<String>,
    previous_symbols: &HashMap<String, Vec<IndexedSymbol>>,
    file_options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<Vec<IndexedSymbol>> {
    let mut symbols = Vec::new();

    for file in files {
        if !changed_paths.contains(&file.path) {
            if let Some(previous) = previous_symbols.get(&file.path) {
                symbols.extend(previous.iter().cloned());
                continue;
            }
        }

        symbols.extend(index_symbols_for_file(
            root,
            file,
            file_options,
            default_ignore,
            ignore_file,
        )?);
    }

    Ok(symbols)
}

fn index_symbols_for_file(
    root: &Path,
    file: &IndexedFile,
    file_options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<Vec<IndexedSymbol>> {
    let path = indexed_path(root, &file.path);
    let path_arg = path.to_string_lossy().to_string();
    let items = crate::symbols::find_symbols(Some(&path_arg), file_options, default_ignore, ignore_file)?;

    Ok(index_symbols(root, &items))
}

fn index_files(root: &Path, items: &[CodeItem]) -> Result<Vec<IndexedFile>> {
    let mut files = Vec::with_capacity(items.len());
    for item in items {
        files.push(file_metadata(root, item.location.path())?);
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn index_symbols(root: &Path, items: &[CodeItem]) -> Vec<IndexedSymbol> {
    let mut symbols = items
        .iter()
        .map(|item| indexed_symbol_from_item(root, item))
        .collect::<Vec<IndexedSymbol>>();
    symbols.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.detail.cmp(&right.detail))
    });
    symbols
}

fn indexed_symbol_from_item(root: &Path, item: &CodeItem) -> IndexedSymbol {
    let path = relative_path(root, item.location.path());
    let line = item.location.line.unwrap_or(1);
    let column = item.location.column;
    let (name, kind) = parse_symbol_detail(&item.detail);

    IndexedSymbol {
        path: path.clone(),
        line,
        column,
        label: item.label.clone(),
        detail: item.detail.clone(),
        name: name.clone(),
        kind,
        language: language_for_path(Path::new(&path)),
        range: symbol_range(line, column, &name),
        parent: None,
    }
}

fn file_metadata(root: &Path, path: &Path) -> Result<IndexedFile> {
    let metadata = fs::metadata(path)?;
    let modified_unix = metadata.modified().ok().and_then(system_time_to_unix).unwrap_or(0);

    Ok(IndexedFile {
        path: relative_path(root, path),
        size_bytes: metadata.len(),
        modified_unix,
        language: language_for_path(path),
    })
}

fn file_metadata_by_path(files: &[IndexedFile]) -> HashMap<String, IndexedFile> {
    files
        .iter()
        .map(|file| (file.path.clone(), file.clone()))
        .collect::<HashMap<String, IndexedFile>>()
}

fn count_values<'a>(values: impl Iterator<Item = &'a str>) -> Vec<IndexCount> {
    let mut counts = BTreeMap::<String, usize>::new();
    for value in values {
        let name = if value.is_empty() { "unknown" } else { value };
        *counts.entry(name.to_string()).or_default() += 1;
    }

    counts
        .into_iter()
        .map(|(name, count)| IndexCount { name, count })
        .collect()
}

fn refresh_reason(status: &IndexStatus) -> String {
    if !status.exists {
        return "missing".to_string();
    }
    if status.is_corrupt {
        return "corrupt".to_string();
    }
    if status.schema_status != IndexSchemaStatus::Current {
        return format!("schema={}", schema_status_label(status.schema_status));
    }
    if status.changed_tracked_files > 0 || status.missing_tracked_files > 0 {
        return format!(
            "changed={} missing={}",
            status.changed_tracked_files, status.missing_tracked_files
        );
    }
    if status.is_stale {
        return "stale".to_string();
    }
    "fresh".to_string()
}

fn schema_status_label(status: IndexSchemaStatus) -> &'static str {
    match status {
        IndexSchemaStatus::Missing => "missing",
        IndexSchemaStatus::Current => "current",
        IndexSchemaStatus::Migrated => "migrated",
        IndexSchemaStatus::Future => "future",
        IndexSchemaStatus::Corrupt => "corrupt",
    }
}

fn symbols_by_path(symbols: &[IndexedSymbol]) -> HashMap<String, Vec<IndexedSymbol>> {
    let mut grouped = HashMap::<String, Vec<IndexedSymbol>>::new();
    for symbol in symbols {
        grouped.entry(symbol.path.clone()).or_default().push(symbol.clone());
    }
    grouped
}

fn is_same_file_snapshot(left: &IndexedFile, right: &IndexedFile) -> bool {
    left.path == right.path && left.size_bytes == right.size_bytes && left.modified_unix == right.modified_unix
}

fn finalize_symbol_metadata(symbols: &mut [IndexedSymbol]) {
    symbols.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.detail.cmp(&right.detail))
    });

    let mut parent_by_path = HashMap::<String, String>::new();
    for symbol in symbols {
        if symbol.name.is_empty() || symbol.kind.is_empty() {
            let (name, kind) = parse_symbol_detail(&symbol.detail);
            symbol.name = name;
            symbol.kind = kind;
        }
        if symbol.language.is_empty() {
            symbol.language = language_for_path(Path::new(&symbol.path));
        }
        if symbol.range.start_line == 0 {
            symbol.range = symbol_range(symbol.line, symbol.column, &symbol.name);
        }

        if is_container_symbol(&symbol.kind) {
            symbol.parent = None;
            parent_by_path.insert(symbol.path.clone(), symbol.detail.clone());
        } else if symbol.parent.is_none() {
            symbol.parent = parent_by_path.get(&symbol.path).cloned();
        }
    }
}

fn parse_symbol_detail(detail: &str) -> (String, String) {
    if let Some((name, rest)) = detail.rsplit_once(" [") {
        return (name.to_string(), rest.trim_end_matches(']').trim().to_string());
    }

    (detail.to_string(), "symbol".to_string())
}

fn symbol_range(line: usize, column: Option<usize>, name: &str) -> IndexedSymbolRange {
    let start_column = column.unwrap_or(1).max(1);
    let end_column = start_column + name.chars().count().max(1);

    IndexedSymbolRange {
        start_line: line,
        start_column,
        end_line: line,
        end_column,
    }
}

fn is_container_symbol(kind: &str) -> bool {
    matches!(kind, "class" | "struct" | "enum" | "trait" | "impl")
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
        "java" => "java",
        "go" => "go",
        _ => "unknown",
    }
    .to_string()
}

fn format_symbol_entry(symbol: &IndexedSymbol) -> String {
    let parent = symbol
        .parent
        .as_ref()
        .map(|value| format!(" parent={value}"))
        .unwrap_or_default();

    format!(
        "{}:{}:{} [{}] range={}:{}-{}:{}{}",
        symbol.path,
        symbol.line,
        symbol.detail,
        symbol.language,
        symbol.range.start_line,
        symbol.range.start_column,
        symbol.range.end_line,
        symbol.range.end_column,
        parent
    )
}

fn fuzzy_score(value: &str, query: &str) -> Option<usize> {
    let value = value.to_lowercase();
    let query = query.to_lowercase();
    if value.contains(&query) {
        return Some(value.find(&query).unwrap_or(0));
    }

    let mut score = 0;
    let mut cursor = 0;
    for ch in query.chars() {
        let relative = value[cursor..].find(ch)?;
        score += relative;
        cursor += relative + ch.len_utf8();
    }
    Some(score + value.len())
}

fn indexed_path(root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
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

fn now_unix() -> u64 {
    system_time_to_unix(SystemTime::now()).unwrap_or(0)
}

fn system_time_to_unix(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fcs_index_{name}_{}", std::process::id()))
    }

    #[test]
    fn builds_and_loads_file_and_symbol_index() {
        let temp_dir = temp_workspace_dir("build_load");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "pub fn main() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");

        let report = build(&temp_dir, &[], &[], &ignore_file).unwrap();
        let index = load(&temp_dir).unwrap().unwrap();
        let status = status(&temp_dir).unwrap();
        let symbols = list(&temp_dir, IndexListKind::Symbols, 10).unwrap();
        let queried = query(&temp_dir, IndexListKind::Symbols, "main", 10).unwrap();

        assert!(report.path.ends_with(INDEX_FILE_NAME));
        assert!(report.file_count >= 2);
        assert_eq!(report.removed_files, 0);
        assert_eq!(index.version, INDEX_VERSION);
        assert!(!index.is_empty());
        assert!(index
            .files
            .iter()
            .any(|file| file.path == "src/main.rs" && file.language == "rust"));
        assert!(index.symbols.iter().any(|symbol| {
            symbol.name == "main"
                && symbol.kind == "function"
                && symbol.language == "rust"
                && symbol.range.start_line == 1
                && symbol.range.start_column >= 1
        }));
        assert!(symbols.iter().any(|symbol| symbol.contains("main [function]")));
        assert!(queried.iter().any(|symbol| symbol.contains("main [function]")));
        assert!(status.exists);
        assert_eq!(status.version, Some(INDEX_VERSION));
        assert_eq!(status.changed_tracked_files, 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn status_detects_changed_tracked_file() {
        let temp_dir = temp_workspace_dir("changed");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        let source_path = temp_dir.join("src").join("lib.rs");
        fs::write(&source_path, "pub fn first() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");

        build(&temp_dir, &[], &[], &ignore_file).unwrap();
        fs::write(&source_path, "pub fn first() {}\npub fn second() {}\n").unwrap();
        let status = status(&temp_dir).unwrap();

        assert!(status.changed_tracked_files >= 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn incremental_build_reuses_unchanged_symbols_and_refreshes_changed_files() {
        let temp_dir = temp_workspace_dir("incremental");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        let stable_path = temp_dir.join("src").join("stable.rs");
        let changed_path = temp_dir.join("src").join("changed.rs");
        fs::write(&stable_path, "pub struct App {}\npub fn run() {}\n").unwrap();
        fs::write(&changed_path, "pub fn before() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");

        build(&temp_dir, &[], &[], &ignore_file).unwrap();
        fs::write(&changed_path, "pub fn before() {}\npub fn after_change() {}\n").unwrap();
        let report = build(&temp_dir, &[], &[], &ignore_file).unwrap();
        let index = load(&temp_dir).unwrap().unwrap();

        assert_eq!(report.changed_files, 1);
        assert!(report.unchanged_files >= 2);
        assert!(index.symbols.iter().any(|symbol| {
            symbol.path == "src/stable.rs" && symbol.name == "run" && symbol.parent.as_deref() == Some("App [struct]")
        }));
        assert!(index
            .symbols
            .iter()
            .any(|symbol| symbol.path == "src/changed.rs" && symbol.name == "after_change"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn status_reports_corrupt_index_without_failing() {
        let temp_dir = temp_workspace_dir("corrupt");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let path = index_path(&temp_dir).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "this is not valid toml = [").unwrap();

        let status = status(&temp_dir).unwrap();

        assert!(status.exists);
        assert!(status.is_stale);
        assert!(status.is_corrupt);
        assert_eq!(status.schema_status, IndexSchemaStatus::Corrupt);
        assert!(status.message.unwrap().contains("Corrupt index TOML"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn migrates_legacy_index_missing_v2_metadata() {
        let contents = r#"
version = 1
root = "/tmp/fcs_legacy"
built_at_unix = 1

[[files]]
path = "src/lib.rs"
size_bytes = 10
modified_unix = 1

[[symbols]]
path = "src/lib.rs"
line = 2
label = "src/lib.rs"
detail = "Config [struct]"
"#;

        let index = migrate_index_contents(contents).unwrap();

        assert_eq!(index.version, INDEX_VERSION);
        assert_eq!(index.files[0].language, "rust");
        assert_eq!(index.symbols[0].name, "Config");
        assert_eq!(index.symbols[0].kind, "struct");
        assert_eq!(index.symbols[0].language, "rust");
        assert_eq!(index.symbols[0].range.start_line, 2);
        assert!(needs_schema_migration(contents).unwrap());
    }

    #[test]
    fn build_recovers_from_corrupt_previous_index() {
        let temp_dir = temp_workspace_dir("corrupt_rebuild");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "pub fn main() {}\n").unwrap();
        let path = index_path(&temp_dir).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "broken = [").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");

        let report = build(&temp_dir, &[], &[], &ignore_file).unwrap();
        let status = status(&temp_dir).unwrap();

        assert!(report.file_count >= 2);
        assert!(!status.is_corrupt);
        assert_eq!(status.schema_status, IndexSchemaStatus::Current);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
