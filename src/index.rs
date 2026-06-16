use std::collections::{hash_map::DefaultHasher, BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use memmap2::Mmap;
use serde::{Deserialize, Serialize};

use crate::core::{CodeItem, CodeItemKind, Location};
use crate::errors::{AppError, Result};

const INDEX_VERSION: u32 = 3;
const INDEX_FILE_NAME: &str = "code_index.toml";
const INDEX_META_FILE_NAME: &str = "code_index_meta.toml";
const INDEX_FILES_FILE_NAME: &str = "code_index_files.toml";
const INDEX_SYMBOLS_FILE_NAME: &str = "code_index_symbols.jsonl";
const INDEX_SYMBOLS_MMAP_FILE_NAME: &str = "code_index_symbols.mmidx";
const INDEX_SHARD_DIR_NAME: &str = "code_index_shards";
const INDEX_SHARD_MANIFEST_FILE_NAME: &str = "manifest.toml";
const INDEX_TMP_EXTENSION: &str = "tmp";
const INDEX_DAEMON_HEARTBEAT_FILE_NAME: &str = "index-daemon.toml";
const MAX_DAEMON_REPORT_CYCLES: usize = 128;
const INDEX_SYMBOLS_MMAP_MAGIC: &str = "FCS_SYMBOLS_V1";

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
    #[serde(default)]
    pub content_hash: String,
    #[serde(default)]
    pub symbol_count: usize,
    #[serde(default)]
    pub last_indexed_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_error: Option<String>,
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
    pub added_files: usize,
    pub reindexed_files: usize,
    pub reused_symbol_files: usize,
    pub changed_paths_sample: Vec<String>,
    pub removed_paths_sample: Vec<String>,
    pub elapsed_ms: u128,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IndexSourceSnapshot {
    source_index: String,
    source_index_size_bytes: u64,
    source_index_modified_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IndexMetadataSnapshot {
    version: u32,
    root: String,
    built_at_unix: u64,
    options: IndexOptionsSnapshot,
    file_count: usize,
    symbol_count: usize,
    source_size_bytes: u64,
    index_size_bytes: u64,
    languages: Vec<IndexCount>,
    symbol_kinds: Vec<IndexCount>,
    source: IndexSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IndexFilesSnapshot {
    version: u32,
    root: String,
    built_at_unix: u64,
    options: IndexOptionsSnapshot,
    file_count: usize,
    symbol_count: usize,
    files: Vec<IndexedFile>,
    source: IndexSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexShardBucket {
    pub name: String,
    pub files: usize,
    pub symbols: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexShardReport {
    pub path: PathBuf,
    pub exists: bool,
    pub file_count: usize,
    pub symbol_count: usize,
    pub index_size_bytes: u64,
    pub target_symbols_per_shard: usize,
    pub recommended_shards: usize,
    pub buckets: Vec<IndexShardBucket>,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexShardInfo {
    pub name: String,
    pub file_name: String,
    pub files: usize,
    pub symbols: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexShardManifest {
    pub version: u32,
    pub root: String,
    pub source_index: String,
    pub source_built_at_unix: u64,
    #[serde(default)]
    pub source_index_size_bytes: u64,
    #[serde(default)]
    pub source_index_modified_unix: u64,
    pub file_count: usize,
    pub symbol_count: usize,
    pub shards: Vec<IndexShardInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexShardQueryReport {
    pub entries: Vec<String>,
    pub shard_count: usize,
    pub shards_scanned: usize,
    pub fallback_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexShardBuildReport {
    pub manifest_path: PathBuf,
    pub shard_dir: PathBuf,
    pub shard_count: usize,
    pub file_count: usize,
    pub symbol_count: usize,
    pub wrote: bool,
    pub shards: Vec<IndexShardInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexShardStatus {
    pub manifest_path: PathBuf,
    pub exists: bool,
    pub stale: bool,
    pub reason: String,
    pub shard_count: usize,
    pub file_count: usize,
    pub symbol_count: usize,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDaemonOptions {
    pub interval_ms: u64,
    pub max_cycles: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDaemonCycle {
    pub cycle: usize,
    pub timestamp_unix: u64,
    pub elapsed_ms: u128,
    pub rebuilt: bool,
    pub reason: String,
    pub file_count: usize,
    pub symbol_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDaemonReport {
    pub root: PathBuf,
    pub heartbeat_path: PathBuf,
    pub cycles: Vec<IndexDaemonCycle>,
    pub rebuilds: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDaemonHeartbeat {
    pub root: String,
    pub pid: u32,
    pub started_at_unix: u64,
    pub updated_at_unix: u64,
    pub interval_ms: u64,
    pub cycles: usize,
    pub rebuilds: usize,
    pub last_rebuilt: bool,
    pub last_reason: String,
    #[serde(default)]
    pub last_file_count: usize,
    #[serde(default)]
    pub last_symbol_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDaemonStatus {
    pub path: PathBuf,
    pub exists: bool,
    pub heartbeat: Option<IndexDaemonHeartbeat>,
    pub stale: bool,
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
    let started = Instant::now();
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
    let mut files = index_files(&root, &file_items)?;
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
    let added_files = current_paths.difference(&previous_paths).count();
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
    let reindexed_files = changed_paths.len();
    let reused_symbol_files = files.len().saturating_sub(reindexed_files);
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
    apply_file_symbol_counts(&mut files, &symbols);
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
    write_workspace_index(&root, &path, &index)?;

    Ok(IndexBuildReport {
        path,
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        unchanged_files,
        changed_files,
        removed_files,
        added_files,
        reindexed_files,
        reused_symbol_files,
        changed_paths_sample: sorted_sample(changed_paths.iter(), 5),
        removed_paths_sample: sorted_sample(previous_paths.difference(&current_paths), 5),
        elapsed_ms: started.elapsed().as_millis(),
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

    if let Some(snapshot) = read_index_files_snapshot(&root, &path)? {
        return status_from_file_snapshot(&root, path, snapshot);
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
        match file_status_metadata(&root, &path) {
            Ok(current) if is_same_file_status_snapshot(&current, file) => {}
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
    let root = normalize_root(root);
    let path = index_path(&root)?;
    if !path.exists() {
        return Ok(None);
    }

    if let Some(index) = read_code_index_sidecars(&root, &path)? {
        return Ok(Some(index));
    }

    read_index(&path).map(Some)
}

pub fn stats(root: &Path) -> Result<IndexStats> {
    let root = normalize_root(root);
    let path = index_path(&root)?;
    let index_size_bytes = fs::metadata(&path).ok().map_or(0, |metadata| metadata.len());
    if path.exists() {
        if let Some(metadata) = read_index_metadata_snapshot(&root, &path)? {
            return Ok(IndexStats {
                path,
                exists: true,
                file_count: metadata.file_count,
                symbol_count: metadata.symbol_count,
                source_size_bytes: metadata.source_size_bytes,
                index_size_bytes: metadata.index_size_bytes,
                built_at_unix: Some(metadata.built_at_unix),
                languages: metadata.languages,
                symbol_kinds: metadata.symbol_kinds,
            });
        }
    }

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

pub fn shard_report(root: &Path, target_symbols_per_shard: usize) -> Result<IndexShardReport> {
    if target_symbols_per_shard == 0 {
        return Err(AppError::General(
            "target_symbols_per_shard must be greater than zero".to_string(),
        ));
    }

    let root = normalize_root(root);
    let path = index_path(&root)?;
    let index_size_bytes = fs::metadata(&path).ok().map_or(0, |metadata| metadata.len());
    let Some(index) = load(&root)? else {
        return Ok(IndexShardReport {
            path,
            exists: false,
            file_count: 0,
            symbol_count: 0,
            index_size_bytes,
            target_symbols_per_shard,
            recommended_shards: 0,
            buckets: Vec::new(),
            recommendation: "build the index before planning shards".to_string(),
        });
    };

    let mut buckets = BTreeMap::<String, IndexShardBucket>::new();
    for file in &index.files {
        let key = shard_key(&file.path);
        let bucket = buckets.entry(key.clone()).or_insert_with(|| IndexShardBucket {
            name: key,
            files: 0,
            symbols: 0,
        });
        bucket.files += 1;
    }
    for symbol in &index.symbols {
        let key = shard_key(&symbol.path);
        let bucket = buckets.entry(key.clone()).or_insert_with(|| IndexShardBucket {
            name: key,
            files: 0,
            symbols: 0,
        });
        bucket.symbols += 1;
    }

    let symbol_count = index.symbols.len();
    let recommended_shards = if symbol_count == 0 {
        1
    } else {
        symbol_count.div_ceil(target_symbols_per_shard)
    };
    let recommendation = if recommended_shards <= 1 {
        "single-file index is still appropriate".to_string()
    } else {
        format!(
            "consider {} shard(s) at roughly {} symbols per shard",
            recommended_shards, target_symbols_per_shard
        )
    };
    let mut buckets = buckets.into_values().collect::<Vec<IndexShardBucket>>();
    buckets.sort_by(|left, right| {
        right
            .symbols
            .cmp(&left.symbols)
            .then_with(|| right.files.cmp(&left.files))
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(IndexShardReport {
        path,
        exists: true,
        file_count: index.files.len(),
        symbol_count,
        index_size_bytes,
        target_symbols_per_shard,
        recommended_shards,
        buckets,
        recommendation,
    })
}

pub fn build_shards(root: &Path, target_symbols_per_shard: usize) -> Result<IndexShardBuildReport> {
    if target_symbols_per_shard == 0 {
        return Err(AppError::General(
            "target_symbols_per_shard must be greater than zero".to_string(),
        ));
    }

    let root = normalize_root(root);
    let index_path = index_path(&root)?;
    let Some(index) = load(&root)? else {
        return Err(AppError::General(
            "Cannot build index shards before the main index exists".to_string(),
        ));
    };

    let shard_dir = shard_dir(&root)?;
    fs::create_dir_all(&shard_dir)?;
    let mut buckets = BTreeMap::<String, (Vec<IndexedFile>, Vec<IndexedSymbol>)>::new();
    for file in &index.files {
        buckets.entry(shard_key(&file.path)).or_default().0.push(file.clone());
    }
    for symbol in &index.symbols {
        buckets
            .entry(shard_key(&symbol.path))
            .or_default()
            .1
            .push(symbol.clone());
    }

    let mut shards = Vec::new();
    for (name, (files, symbols)) in buckets {
        let file_name = format!("{}.toml", sanitize_shard_name(&name));
        let shard_path = shard_dir.join(&file_name);
        let shard = CodeIndex {
            version: index.version,
            root: index.root.clone(),
            built_at_unix: index.built_at_unix,
            options: index.options.clone(),
            files,
            symbols,
        };
        write_index(&shard_path, &shard)?;
        shards.push(IndexShardInfo {
            name,
            file_name,
            files: shard.files.len(),
            symbols: shard.symbols.len(),
        });
    }
    shards.sort_by(|left, right| {
        right
            .symbols
            .cmp(&left.symbols)
            .then_with(|| right.files.cmp(&left.files))
            .then_with(|| left.name.cmp(&right.name))
    });
    let source_metadata = fs::metadata(&index_path).ok();
    let source_index_size_bytes = source_metadata.as_ref().map_or(0, |metadata| metadata.len());
    let source_index_modified_unix = source_metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_unix)
        .unwrap_or(0);

    let manifest = IndexShardManifest {
        version: INDEX_VERSION,
        root: root.to_string_lossy().to_string(),
        source_index: index_path.display().to_string(),
        source_built_at_unix: index.built_at_unix,
        source_index_size_bytes,
        source_index_modified_unix,
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        shards: shards.clone(),
    };
    let manifest_path = shard_manifest_path(&root)?;
    write_shard_manifest(&manifest_path, &manifest)?;

    Ok(IndexShardBuildReport {
        manifest_path,
        shard_dir,
        shard_count: shards.len(),
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        wrote: true,
        shards,
    })
}

pub fn shard_status(root: &Path) -> Result<IndexShardStatus> {
    let root = normalize_root(root);
    let manifest_path = shard_manifest_path(&root)?;
    if !manifest_path.exists() {
        return Ok(IndexShardStatus {
            manifest_path,
            exists: false,
            stale: true,
            reason: "shard manifest missing".to_string(),
            shard_count: 0,
            file_count: 0,
            symbol_count: 0,
        });
    }

    let manifest = read_shard_manifest(&manifest_path)?;
    let shard_dir = shard_dir(&root)?;
    let missing_shard = manifest
        .shards
        .iter()
        .any(|shard| !shard_dir.join(&shard.file_name).exists());

    if manifest.source_index_size_bytes > 0 && manifest.source_index_modified_unix > 0 {
        let source_index = if manifest.source_index.trim().is_empty() {
            index_path(&root)?
        } else {
            PathBuf::from(&manifest.source_index)
        };
        let source_metadata = fs::metadata(source_index).ok();
        let source_index_size_bytes = source_metadata.as_ref().map_or(0, |metadata| metadata.len());
        let source_index_modified_unix = source_metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_to_unix)
            .unwrap_or(0);
        let source_missing = source_metadata.is_none();
        let stale = manifest.version != INDEX_VERSION
            || normalize_root(Path::new(&manifest.root)) != root
            || source_missing
            || manifest.source_index_size_bytes != source_index_size_bytes
            || manifest.source_index_modified_unix != source_index_modified_unix
            || missing_shard;
        let reason = if missing_shard {
            "one or more shard files are missing".to_string()
        } else if source_missing {
            "main index missing".to_string()
        } else if stale {
            "shards do not match the current main index metadata".to_string()
        } else {
            "fresh".to_string()
        };

        return Ok(IndexShardStatus {
            manifest_path,
            exists: true,
            stale,
            reason,
            shard_count: manifest.shards.len(),
            file_count: manifest.file_count,
            symbol_count: manifest.symbol_count,
        });
    }

    let Some(index) = load(&root)? else {
        return Ok(IndexShardStatus {
            manifest_path,
            exists: true,
            stale: true,
            reason: "main index missing".to_string(),
            shard_count: manifest.shards.len(),
            file_count: manifest.file_count,
            symbol_count: manifest.symbol_count,
        });
    };

    let stale = manifest.version != INDEX_VERSION
        || normalize_root(Path::new(&manifest.root)) != root
        || manifest.source_built_at_unix != index.built_at_unix
        || manifest.file_count != index.files.len()
        || manifest.symbol_count != index.symbols.len()
        || missing_shard;
    let reason = if missing_shard {
        "one or more shard files are missing".to_string()
    } else if stale {
        "shards do not match the current main index".to_string()
    } else {
        "fresh".to_string()
    };

    Ok(IndexShardStatus {
        manifest_path,
        exists: true,
        stale,
        reason,
        shard_count: manifest.shards.len(),
        file_count: manifest.file_count,
        symbol_count: manifest.symbol_count,
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
        write_index_sidecars(&root, &path, &index)?;
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
        if ensure_index_sidecars(&root, &status.path)? {
            return Ok(IndexRefreshReport {
                path: status.path,
                rebuilt: false,
                reason: "sidecars refreshed".to_string(),
                build_report: None,
            });
        }
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

pub fn run_polling_daemon(
    root: &Path,
    file_options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
    options: IndexDaemonOptions,
) -> Result<IndexDaemonReport> {
    if options.max_cycles == Some(0) {
        return Err(AppError::General(
            "index daemon --max-cycles must be greater than zero".to_string(),
        ));
    }

    let root = normalize_root(root);
    let heartbeat_path = daemon_heartbeat_path(&root)?;
    let started_at_unix = now_unix();
    let pid = std::process::id();
    let mut report = IndexDaemonReport {
        root: root.clone(),
        heartbeat_path: heartbeat_path.clone(),
        cycles: Vec::new(),
        rebuilds: 0,
    };

    loop {
        let cycle_number = report.cycles.last().map_or(1, |cycle| cycle.cycle + 1);
        let started = Instant::now();
        let refresh_report = refresh(&root, file_options, default_ignore, ignore_file)?;
        let elapsed_ms = started.elapsed().as_millis();
        let status = status(&root)?;
        let cycle = IndexDaemonCycle {
            cycle: cycle_number,
            timestamp_unix: now_unix(),
            elapsed_ms,
            rebuilt: refresh_report.rebuilt,
            reason: refresh_report.reason,
            file_count: status.file_count,
            symbol_count: status.symbol_count,
        };
        if cycle.rebuilt {
            report.rebuilds += 1;
        }
        write_daemon_heartbeat(
            &heartbeat_path,
            &IndexDaemonHeartbeat {
                root: root.to_string_lossy().to_string(),
                pid,
                started_at_unix,
                updated_at_unix: cycle.timestamp_unix,
                interval_ms: options.interval_ms,
                cycles: cycle.cycle,
                rebuilds: report.rebuilds,
                last_rebuilt: cycle.rebuilt,
                last_reason: cycle.reason.clone(),
                last_file_count: cycle.file_count,
                last_symbol_count: cycle.symbol_count,
            },
        )?;
        push_daemon_cycle(&mut report.cycles, cycle);

        if options
            .max_cycles
            .is_some_and(|max_cycles| report.cycles.last().is_some_and(|cycle| cycle.cycle >= max_cycles))
        {
            break;
        }
        if options.interval_ms > 0 {
            thread::sleep(Duration::from_millis(options.interval_ms));
        }
    }

    Ok(report)
}

pub fn daemon_status(root: &Path) -> Result<IndexDaemonStatus> {
    let root = normalize_root(root);
    let path = daemon_heartbeat_path(&root)?;
    if !path.exists() {
        return Ok(IndexDaemonStatus {
            path,
            exists: false,
            heartbeat: None,
            stale: false,
        });
    }

    let contents = fs::read_to_string(&path)?;
    let heartbeat = toml::from_str::<IndexDaemonHeartbeat>(&contents)
        .map_err(|err| AppError::General(format!("Corrupt index daemon heartbeat: {err}")))?;
    let grace_secs = ((heartbeat.interval_ms / 1000).max(1) * 3).max(5);
    let stale = now_unix().saturating_sub(heartbeat.updated_at_unix) > grace_secs;
    Ok(IndexDaemonStatus {
        path,
        exists: true,
        heartbeat: Some(heartbeat),
        stale,
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
    let root = normalize_root(root);
    let limit = limit.max(1);
    let path = index_path(&root)?;
    if kind == IndexListKind::Files {
        if let Some(snapshot) = read_index_files_snapshot(&root, &path)? {
            return Ok(snapshot.files.iter().take(limit).map(format_file_entry).collect());
        }
    }
    if kind == IndexListKind::Symbols {
        if let Some(entries) = list_symbols_mmap(&root, &path, limit)? {
            return Ok(entries);
        }
        if let Some(entries) = list_symbols_jsonl(&root, &path, limit)? {
            return Ok(entries);
        }
    }

    let Some(index) = load(&root)? else {
        return Ok(Vec::new());
    };
    let entries = match kind {
        IndexListKind::Files => index.files.iter().take(limit).map(format_file_entry).collect(),
        IndexListKind::Symbols => index.symbols.iter().take(limit).map(format_symbol_entry).collect(),
    };

    Ok(entries)
}

pub fn query(root: &Path, kind: IndexListKind, query: &str, limit: usize) -> Result<Vec<String>> {
    let root = normalize_root(root);
    let path = index_path(&root)?;
    if kind == IndexListKind::Files {
        if let Some(snapshot) = read_index_files_snapshot(&root, &path)? {
            return Ok(format_scored_entries(score_index_files(&snapshot.files, query), limit));
        }
    }
    if kind == IndexListKind::Symbols {
        if let Some(entries) = query_symbols_mmap(&root, &path, query, limit)? {
            return Ok(entries);
        }
        if let Some(entries) = query_symbols_jsonl(&root, &path, query, limit)? {
            return Ok(entries);
        }
    }

    let Some(index) = load(&root)? else {
        return Ok(Vec::new());
    };
    Ok(format_scored_entries(
        score_code_index_entries(&index, kind, query),
        limit,
    ))
}

pub fn query_code_items(root: &Path, kind: IndexListKind, query: &str, limit: usize) -> Result<Option<Vec<CodeItem>>> {
    query_code_items_with_cancel(root, kind, query, limit, None)
}

pub fn query_code_items_with_cancel(
    root: &Path,
    kind: IndexListKind,
    query: &str,
    limit: usize,
    cancel: Option<&crate::search::SearchCancel>,
) -> Result<Option<Vec<CodeItem>>> {
    let root = normalize_root(root);
    let path = index_path(&root)?;
    match kind {
        IndexListKind::Files => {
            if let Some(snapshot) = read_index_files_snapshot(&root, &path)? {
                return Ok(Some(scored_file_items(&root, &snapshot.files, query, limit, cancel)?));
            }
        }
        IndexListKind::Symbols => {
            if let Some(items) = query_symbol_items_mmap(&root, &path, query, limit, cancel)? {
                return Ok(Some(items));
            }
            if let Some(items) = query_symbol_items_jsonl(&root, &path, query, limit, cancel)? {
                return Ok(Some(items));
            }
        }
    }

    let Some(index) = load(&root)? else {
        return Ok(None);
    };
    let items = match kind {
        IndexListKind::Files => scored_file_items(&root, &index.files, query, limit, cancel)?,
        IndexListKind::Symbols => scored_symbol_items(&root, index.symbols.iter(), query, limit, cancel)?,
    };
    Ok(Some(items))
}

pub fn query_shards(root: &Path, kind: IndexListKind, query_text: &str, limit: usize) -> Result<Vec<String>> {
    Ok(query_shards_report(root, kind, query_text, limit)?.entries)
}

pub fn query_shards_report(
    root: &Path,
    kind: IndexListKind,
    query_text: &str,
    limit: usize,
) -> Result<IndexShardQueryReport> {
    let root = normalize_root(root);
    let status = shard_status(&root)?;
    if !status.exists || status.stale {
        return Ok(IndexShardQueryReport {
            entries: query(&root, kind, query_text, limit)?,
            shard_count: status.shard_count,
            shards_scanned: 0,
            fallback_used: true,
        });
    }

    let manifest = read_shard_manifest(&status.manifest_path)?;
    let shard_dir = shard_dir(&root)?;
    let main_index_path = index_path(&root)?;
    match kind {
        IndexListKind::Files => {
            if let Some(snapshot) = read_index_files_snapshot(&root, &main_index_path)? {
                return Ok(IndexShardQueryReport {
                    entries: format_scored_entries(score_index_files(&snapshot.files, query_text), limit),
                    shard_count: manifest.shards.len(),
                    shards_scanned: 0,
                    fallback_used: false,
                });
            }
        }
        IndexListKind::Symbols => {
            if let Some(entries) = query_symbols_mmap(&root, &main_index_path, query_text, limit)? {
                return Ok(IndexShardQueryReport {
                    entries,
                    shard_count: manifest.shards.len(),
                    shards_scanned: 0,
                    fallback_used: false,
                });
            }
            if let Some(entries) = query_symbols_jsonl(&root, &main_index_path, query_text, limit)? {
                return Ok(IndexShardQueryReport {
                    entries,
                    shard_count: manifest.shards.len(),
                    shards_scanned: 0,
                    fallback_used: false,
                });
            }
        }
    }

    let candidate_shards = candidate_shards_for_query(&manifest, query_text);
    let shards_to_scan = if candidate_shards.is_empty() {
        manifest.shards.iter().collect::<Vec<_>>()
    } else {
        candidate_shards
    };
    let mut scored = score_shard_entries(&shard_dir, &shards_to_scan, kind, query_text)?;
    let mut shards_scanned = shards_to_scan.len();

    if scored.is_empty() && shards_scanned != manifest.shards.len() {
        let all_shards = manifest.shards.iter().collect::<Vec<_>>();
        scored = score_shard_entries(&shard_dir, &all_shards, kind, query_text)?;
        shards_scanned = all_shards.len();
    }

    Ok(IndexShardQueryReport {
        entries: format_scored_entries(scored, limit),
        shard_count: manifest.shards.len(),
        shards_scanned,
        fallback_used: false,
    })
}

fn score_shard_entries(
    shard_dir: &Path,
    shards: &[&IndexShardInfo],
    kind: IndexListKind,
    query_text: &str,
) -> Result<Vec<(usize, String)>> {
    let mut scored = Vec::new();
    for shard in shards {
        let shard_path = shard_dir.join(&shard.file_name);
        let index = read_index(&shard_path)?;
        scored.extend(score_code_index_entries(&index, kind, query_text));
    }
    Ok(scored)
}

fn candidate_shards_for_query<'a>(manifest: &'a IndexShardManifest, query_text: &str) -> Vec<&'a IndexShardInfo> {
    let hints = query_shard_hints(query_text);
    if hints.is_empty() {
        return Vec::new();
    }

    manifest
        .shards
        .iter()
        .filter(|shard| {
            let name = shard.name.to_lowercase();
            let file_name = shard.file_name.trim_end_matches(".toml").to_lowercase();
            hints.iter().any(|hint| hint == &name || hint == &file_name)
        })
        .collect()
}

fn query_shard_hints(query_text: &str) -> HashSet<String> {
    query_text
        .split_whitespace()
        .filter_map(query_token_shard_hint)
        .collect()
}

fn query_token_shard_hint(token: &str) -> Option<String> {
    let value = token.split_once(':').map_or(token, |(_, value)| value);
    let value =
        value.trim_matches(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '\\')));
    if value.is_empty() {
        return None;
    }

    let normalized = value.replace('\\', "/");
    if !normalized.contains('/') {
        return Some(normalized.to_lowercase());
    }

    normalized
        .split('/')
        .find(|part| !part.trim().is_empty())
        .map(|part| part.to_lowercase())
}

fn score_code_index_entries(index: &CodeIndex, kind: IndexListKind, query: &str) -> Vec<(usize, String)> {
    let query = query.trim();
    if query.is_empty() {
        return match kind {
            IndexListKind::Files => index.files.iter().map(|file| (0, format_file_entry(file))).collect(),
            IndexListKind::Symbols => index
                .symbols
                .iter()
                .map(|symbol| (0, format_symbol_entry(symbol)))
                .collect(),
        };
    }

    match kind {
        IndexListKind::Files => score_index_files(&index.files, query),
        IndexListKind::Symbols => index
            .symbols
            .iter()
            .filter_map(|symbol| {
                let entry = format_symbol_entry(symbol);
                let haystack = format!("{} {} {} {}", symbol.path, symbol.name, symbol.kind, symbol.detail);
                fuzzy_score(&haystack, query).map(|score| (score, entry))
            })
            .collect(),
    }
}

fn score_index_files(files: &[IndexedFile], query: &str) -> Vec<(usize, String)> {
    let query = query.trim();
    if query.is_empty() {
        return files.iter().map(|file| (0, format_file_entry(file))).collect();
    }

    files
        .iter()
        .filter_map(|file| {
            let haystack = format!("{} {}", file.path, file.language);
            fuzzy_score(&haystack, query).map(|score| (score, format_file_entry(file)))
        })
        .collect()
}

fn scored_file_items(
    root: &Path,
    files: &[IndexedFile],
    query: &str,
    limit: usize,
    cancel: Option<&crate::search::SearchCancel>,
) -> Result<Vec<CodeItem>> {
    let query = query.trim();
    let limit = limit.max(1);
    let mut scored = Vec::new();
    for file in files {
        if cancel.is_some_and(crate::search::SearchCancel::is_cancelled) {
            return Err(AppError::General("Search cancelled".to_string()));
        }
        let item = file_to_code_item(root, file);
        if query.is_empty() {
            scored.push((0, item.display_text().to_string(), item));
            if scored.len() >= limit {
                break;
            }
            continue;
        }
        let haystack = format!("{} {}", file.path, file.language);
        if let Some(score) = fuzzy_score(&haystack, query) {
            push_bounded_scored_item(&mut scored, (score, item.display_text().to_string(), item), limit);
        }
    }

    Ok(format_scored_items(scored, limit))
}

fn scored_symbol_items<'a>(
    root: &Path,
    symbols: impl Iterator<Item = &'a IndexedSymbol>,
    query: &str,
    limit: usize,
    cancel: Option<&crate::search::SearchCancel>,
) -> Result<Vec<CodeItem>> {
    let query = query.trim();
    let limit = limit.max(1);
    let mut scored = Vec::new();
    for symbol in symbols {
        if cancel.is_some_and(crate::search::SearchCancel::is_cancelled) {
            return Err(AppError::General("Search cancelled".to_string()));
        }
        let item = symbol_to_code_item(root, symbol);
        if query.is_empty() {
            scored.push((0, item.display_text().to_string(), item));
            if scored.len() >= limit {
                break;
            }
            continue;
        }
        let haystack = format!("{} {} {} {}", symbol.path, symbol.name, symbol.kind, symbol.detail);
        if let Some(score) = fuzzy_score(&haystack, query) {
            push_bounded_scored_item(&mut scored, (score, item.display_text().to_string(), item), limit);
        }
    }

    Ok(format_scored_items(scored, limit))
}

fn format_scored_entries(mut scored: Vec<(usize, String)>, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    scored.sort_by_key(|(score, entry)| (*score, entry.clone()));
    scored
        .into_iter()
        .filter_map(|(_, entry)| if seen.insert(entry.clone()) { Some(entry) } else { None })
        .take(limit.max(1))
        .collect()
}

fn format_scored_items(mut scored: Vec<(usize, String, CodeItem)>, limit: usize) -> Vec<CodeItem> {
    let mut seen = HashSet::new();
    scored.sort_by_key(|(score, display, _)| (*score, display.clone()));
    scored
        .into_iter()
        .filter(|(_, display, _)| seen.insert(display.clone()))
        .take(limit.max(1))
        .map(|(_, _, item)| item)
        .collect()
}

fn push_bounded_scored(scored: &mut Vec<(usize, String)>, entry: (usize, String), limit: usize) {
    scored.push(entry);
    if scored.len() > limit.max(1) * 4 {
        scored.sort_by_key(|(score, entry)| (*score, entry.clone()));
        scored.truncate(limit.max(1));
    }
}

fn push_bounded_scored_item(
    scored: &mut Vec<(usize, String, CodeItem)>,
    entry: (usize, String, CodeItem),
    limit: usize,
) {
    scored.push(entry);
    if scored.len() > limit.max(1) * 4 {
        scored.sort_by_key(|(score, display, _)| (*score, display.clone()));
        scored.truncate(limit.max(1));
    }
}

pub fn index_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(INDEX_FILE_NAME))
}

fn index_meta_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(INDEX_META_FILE_NAME))
}

fn index_files_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(INDEX_FILES_FILE_NAME))
}

fn index_symbols_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(INDEX_SYMBOLS_FILE_NAME))
}

fn index_symbols_mmap_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(INDEX_SYMBOLS_MMAP_FILE_NAME))
}

fn daemon_heartbeat_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(INDEX_DAEMON_HEARTBEAT_FILE_NAME))
}

fn shard_dir(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(INDEX_SHARD_DIR_NAME))
}

fn shard_manifest_path(root: &Path) -> Result<PathBuf> {
    Ok(shard_dir(root)?.join(INDEX_SHARD_MANIFEST_FILE_NAME))
}

fn write_shard_manifest(path: &Path, manifest: &IndexShardManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(manifest).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn read_shard_manifest(path: &Path) -> Result<IndexShardManifest> {
    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents).map_err(|err| AppError::General(format!("Corrupt index shard manifest: {err}")))
}

fn write_daemon_heartbeat(path: &Path, heartbeat: &IndexDaemonHeartbeat) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(heartbeat).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn status_from_file_snapshot(root: &Path, path: PathBuf, snapshot: IndexFilesSnapshot) -> Result<IndexStatus> {
    let mut changed_tracked_files = 0;
    let mut missing_tracked_files = 0;
    for file in &snapshot.files {
        let path = indexed_path(root, &file.path);
        match file_status_metadata(root, &path) {
            Ok(current) if is_same_file_status_snapshot(&current, file) => {}
            Ok(_) => changed_tracked_files += 1,
            Err(_) => missing_tracked_files += 1,
        }
    }

    Ok(IndexStatus {
        path,
        exists: true,
        version: Some(snapshot.version),
        schema_status: IndexSchemaStatus::Current,
        is_stale: changed_tracked_files > 0 || missing_tracked_files > 0,
        is_corrupt: false,
        message: None,
        file_count: snapshot.file_count,
        symbol_count: snapshot.symbol_count,
        built_at_unix: Some(snapshot.built_at_unix),
        changed_tracked_files,
        missing_tracked_files,
    })
}

fn push_daemon_cycle(cycles: &mut Vec<IndexDaemonCycle>, cycle: IndexDaemonCycle) {
    if cycles.len() >= MAX_DAEMON_REPORT_CYCLES {
        cycles.remove(0);
    }
    cycles.push(cycle);
}

fn write_workspace_index(root: &Path, path: &Path, index: &CodeIndex) -> Result<()> {
    write_index(path, index)?;
    write_index_sidecars(root, path, index)
}

fn write_index(path: &Path, index: &CodeIndex) -> Result<()> {
    let contents = toml::to_string_pretty(index).map_err(|err| AppError::General(err.to_string()))?;
    write_index_contents(path, &contents)
}

fn write_index_sidecars(root: &Path, path: &Path, index: &CodeIndex) -> Result<()> {
    let source = index_source_snapshot(path)?;
    let metadata = IndexMetadataSnapshot {
        version: INDEX_VERSION,
        root: index.root.clone(),
        built_at_unix: index.built_at_unix,
        options: index.options.clone(),
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        source_size_bytes: index.files.iter().map(|file| file.size_bytes).sum(),
        index_size_bytes: source.source_index_size_bytes,
        languages: count_values(index.files.iter().map(|file| file.language.as_str())),
        symbol_kinds: count_values(index.symbols.iter().map(|symbol| symbol.kind.as_str())),
        source: source.clone(),
    };
    let files = IndexFilesSnapshot {
        version: INDEX_VERSION,
        root: index.root.clone(),
        built_at_unix: index.built_at_unix,
        options: index.options.clone(),
        file_count: index.files.len(),
        symbol_count: index.symbols.len(),
        files: index.files.clone(),
        source: source.clone(),
    };
    write_toml_atomic(&index_files_path(root)?, &files)?;
    write_index_symbols_jsonl(&index_symbols_path(root)?, &index.symbols)?;
    write_index_symbols_mmap(&index_symbols_mmap_path(root)?, &source, &index.symbols)?;
    write_toml_atomic(&index_meta_path(root)?, &metadata)
}

fn ensure_index_sidecars(root: &Path, path: &Path) -> Result<bool> {
    let metadata_ready = read_index_metadata_snapshot(root, path)?.is_some();
    let files_ready = read_index_files_snapshot(root, path)?.is_some();
    let symbols_ready = metadata_ready && index_symbols_path(root)?.exists();
    let symbols_mmap_ready = metadata_ready && index_symbols_mmap_valid_path(root, path)?.is_some();
    if metadata_ready && files_ready && symbols_ready && symbols_mmap_ready {
        return Ok(false);
    }
    let Some(index) = load(root)? else {
        return Ok(false);
    };
    write_index_sidecars(root, path, &index)?;
    Ok(true)
}

fn write_toml_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let contents = toml::to_string_pretty(value).map_err(|err| AppError::General(err.to_string()))?;
    write_index_contents(path, &contents)
}

fn write_index_symbols_jsonl(path: &Path, symbols: &[IndexedSymbol]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension(INDEX_TMP_EXTENSION);
    let file = fs::File::create(&tmp_path)?;
    let mut writer = BufWriter::new(file);
    for symbol in symbols {
        serde_json::to_writer(&mut writer, symbol).map_err(|err| AppError::General(err.to_string()))?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

fn write_index_symbols_mmap(path: &Path, source: &IndexSourceSnapshot, symbols: &[IndexedSymbol]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension(INDEX_TMP_EXTENSION);
    let file = fs::File::create(&tmp_path)?;
    let mut writer = BufWriter::new(file);
    writeln!(
        writer,
        "{}\t{}\t{}\t{}",
        INDEX_SYMBOLS_MMAP_MAGIC,
        source.source_index_size_bytes,
        source.source_index_modified_unix,
        symbols.len()
    )?;
    for symbol in symbols {
        writeln!(
            writer,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            escape_mmap_field(&symbol.path),
            symbol.line,
            symbol.column.unwrap_or(0),
            escape_mmap_field(&symbol.label),
            escape_mmap_field(&symbol.detail),
            escape_mmap_field(&symbol.name),
            escape_mmap_field(&symbol.kind),
            escape_mmap_field(&symbol.language),
            symbol.range.start_line,
            symbol.range.start_column,
            symbol.range.end_line,
            symbol.range.end_column,
            escape_mmap_field(symbol.parent.as_deref().unwrap_or(""))
        )?;
    }
    writer.flush()?;
    fs::rename(tmp_path, path)?;
    Ok(())
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

fn read_index_metadata_snapshot(root: &Path, index_path: &Path) -> Result<Option<IndexMetadataSnapshot>> {
    let path = index_meta_path(root)?;
    if !path.exists() {
        return Ok(None);
    }
    let Ok(contents) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Ok(snapshot) = toml::from_str::<IndexMetadataSnapshot>(&contents) else {
        return Ok(None);
    };
    if sidecar_matches_source(root, index_path, snapshot.version, &snapshot.root, &snapshot.source)? {
        return Ok(Some(snapshot));
    }
    Ok(None)
}

fn read_code_index_sidecars(root: &Path, index_path: &Path) -> Result<Option<CodeIndex>> {
    let Some(metadata) = read_index_metadata_snapshot(root, index_path)? else {
        return Ok(None);
    };
    let Some(files) = read_index_files_snapshot(root, index_path)? else {
        return Ok(None);
    };
    let mut symbols = match read_symbols_mmap_snapshot(root, index_path)? {
        Some(symbols) => symbols,
        None => match read_symbols_jsonl_snapshot(root, index_path)? {
            Some(symbols) => symbols,
            None => return Ok(None),
        },
    };

    if files.files.len() != metadata.file_count || symbols.len() != metadata.symbol_count {
        return Ok(None);
    }

    finalize_symbol_metadata(&mut symbols);
    Ok(Some(CodeIndex {
        version: INDEX_VERSION,
        root: metadata.root,
        built_at_unix: metadata.built_at_unix,
        options: metadata.options,
        files: files.files,
        symbols,
    }))
}

fn read_index_files_snapshot(root: &Path, index_path: &Path) -> Result<Option<IndexFilesSnapshot>> {
    let path = index_files_path(root)?;
    if !path.exists() {
        return Ok(None);
    }
    let Ok(contents) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Ok(snapshot) = toml::from_str::<IndexFilesSnapshot>(&contents) else {
        return Ok(None);
    };
    if sidecar_matches_source(root, index_path, snapshot.version, &snapshot.root, &snapshot.source)? {
        return Ok(Some(snapshot));
    }
    Ok(None)
}

fn read_symbols_mmap_snapshot(root: &Path, index_path: &Path) -> Result<Option<Vec<IndexedSymbol>>> {
    let Some(map) = map_symbols_mmap(root, index_path)? else {
        return Ok(None);
    };
    let Some(body) = symbols_mmap_body(root, index_path, &map)? else {
        return Ok(None);
    };
    let mut symbols = Vec::new();
    for record in mmap_record_lines(body) {
        let Some(symbol) = parse_mmap_symbol_record(record) else {
            return Ok(None);
        };
        symbols.push(symbol);
    }
    Ok(Some(symbols))
}

fn list_symbols_mmap(root: &Path, index_path: &Path, limit: usize) -> Result<Option<Vec<String>>> {
    let Some(map) = map_symbols_mmap(root, index_path)? else {
        return Ok(None);
    };
    let Some(body) = symbols_mmap_body(root, index_path, &map)? else {
        return Ok(None);
    };
    let limit = limit.max(1);
    let mut entries = Vec::new();
    for record in mmap_record_lines(body).take(limit) {
        let Some(symbol) = parse_mmap_symbol_record(record) else {
            return Ok(None);
        };
        entries.push(format_symbol_entry(&symbol));
    }
    Ok(Some(entries))
}

fn query_symbols_mmap(root: &Path, index_path: &Path, query: &str, limit: usize) -> Result<Option<Vec<String>>> {
    let Some(map) = map_symbols_mmap(root, index_path)? else {
        return Ok(None);
    };
    let Some(body) = symbols_mmap_body(root, index_path, &map)? else {
        return Ok(None);
    };
    let query = query.trim();
    let limit = limit.max(1);
    let mut scored = Vec::new();
    for record in mmap_record_lines(body) {
        let Some(symbol) = parse_mmap_symbol_record(record) else {
            return Ok(None);
        };
        if query.is_empty() {
            scored.push((0, format_symbol_entry(&symbol)));
            if scored.len() >= limit {
                break;
            }
            continue;
        }
        let entry = format_symbol_entry(&symbol);
        let haystack = format!("{} {} {} {}", symbol.path, symbol.name, symbol.kind, symbol.detail);
        if let Some(score) = fuzzy_score(&haystack, query) {
            push_bounded_scored(&mut scored, (score, entry), limit);
        }
    }
    Ok(Some(format_scored_entries(scored, limit)))
}

fn query_symbol_items_mmap(
    root: &Path,
    index_path: &Path,
    query: &str,
    limit: usize,
    cancel: Option<&crate::search::SearchCancel>,
) -> Result<Option<Vec<CodeItem>>> {
    let Some(map) = map_symbols_mmap(root, index_path)? else {
        return Ok(None);
    };
    let Some(body) = symbols_mmap_body(root, index_path, &map)? else {
        return Ok(None);
    };
    let query = query.trim();
    let limit = limit.max(1);
    let mut scored = Vec::new();
    for record in mmap_record_lines(body) {
        if cancel.is_some_and(crate::search::SearchCancel::is_cancelled) {
            return Err(AppError::General("Search cancelled".to_string()));
        }
        let Some(symbol) = parse_mmap_symbol_record(record) else {
            return Ok(None);
        };
        let item = symbol_to_code_item(root, &symbol);
        if query.is_empty() {
            scored.push((0, item.display_text().to_string(), item));
            if scored.len() >= limit {
                break;
            }
            continue;
        }
        let haystack = format!("{} {} {} {}", symbol.path, symbol.name, symbol.kind, symbol.detail);
        if let Some(score) = fuzzy_score(&haystack, query) {
            push_bounded_scored_item(&mut scored, (score, item.display_text().to_string(), item), limit);
        }
    }
    Ok(Some(format_scored_items(scored, limit)))
}

fn map_symbols_mmap(root: &Path, index_path: &Path) -> Result<Option<Mmap>> {
    let Some(path) = index_symbols_mmap_valid_path(root, index_path)? else {
        return Ok(None);
    };
    let Ok(file) = fs::File::open(path) else {
        return Ok(None);
    };
    if file.metadata().ok().is_none_or(|metadata| metadata.len() == 0) {
        return Ok(None);
    }
    // SAFETY: the mmap is read-only, scoped to this process, and the file is only
    // replaced atomically by fcs writers; readers validate the header before use.
    let map = unsafe { Mmap::map(&file) }.map_err(|err| AppError::General(err.to_string()))?;
    Ok(Some(map))
}

fn index_symbols_mmap_valid_path(root: &Path, index_path: &Path) -> Result<Option<PathBuf>> {
    if read_index_metadata_snapshot(root, index_path)?.is_none() {
        return Ok(None);
    }
    let path = index_symbols_mmap_path(root)?;
    let Ok(file) = fs::File::open(&path) else {
        return Ok(None);
    };
    let mut reader = BufReader::new(file);
    let mut header = String::new();
    if reader.read_line(&mut header).is_err() {
        return Ok(None);
    }
    if symbols_mmap_header_matches(root, index_path, header.trim_end_matches(['\r', '\n']))? {
        return Ok(Some(path));
    }
    Ok(None)
}

fn symbols_mmap_body<'a>(root: &Path, index_path: &Path, map: &'a [u8]) -> Result<Option<&'a [u8]>> {
    let Some(header_end) = map.iter().position(|byte| *byte == b'\n') else {
        return Ok(None);
    };
    let Ok(header) = std::str::from_utf8(&map[..header_end]) else {
        return Ok(None);
    };
    if !symbols_mmap_header_matches(root, index_path, header)? {
        return Ok(None);
    }
    Ok(Some(&map[header_end + 1..]))
}

fn symbols_mmap_header_matches(root: &Path, index_path: &Path, header: &str) -> Result<bool> {
    let mut parts = header.split('\t');
    if parts.next() != Some(INDEX_SYMBOLS_MMAP_MAGIC) {
        return Ok(false);
    }
    let source_size = parts.next().and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
    let source_mtime = parts.next().and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
    let symbol_count = parts.next().and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
    let current = index_source_snapshot(index_path)?;
    let Some(metadata) = read_index_metadata_snapshot(root, index_path)? else {
        return Ok(false);
    };
    Ok(metadata.symbol_count == symbol_count
        && source_size == current.source_index_size_bytes
        && source_mtime == current.source_index_modified_unix)
}

fn mmap_record_lines(body: &[u8]) -> impl Iterator<Item = &[u8]> {
    body.split(|byte| *byte == b'\n').filter(|line| !line.is_empty())
}

fn parse_mmap_symbol_record(record: &[u8]) -> Option<IndexedSymbol> {
    let fields = record.split(|byte| *byte == b'\t').collect::<Vec<&[u8]>>();
    if fields.len() != 13 {
        return None;
    }

    let column = parse_usize_field(fields[2])?;
    let parent = decode_mmap_field(fields[12])?;
    Some(IndexedSymbol {
        path: decode_mmap_field(fields[0])?,
        line: parse_usize_field(fields[1])?,
        column: (column > 0).then_some(column),
        label: decode_mmap_field(fields[3])?,
        detail: decode_mmap_field(fields[4])?,
        name: decode_mmap_field(fields[5])?,
        kind: decode_mmap_field(fields[6])?,
        language: decode_mmap_field(fields[7])?,
        range: IndexedSymbolRange {
            start_line: parse_usize_field(fields[8])?,
            start_column: parse_usize_field(fields[9])?,
            end_line: parse_usize_field(fields[10])?,
            end_column: parse_usize_field(fields[11])?,
        },
        parent: (!parent.is_empty()).then_some(parent),
    })
}

fn parse_usize_field(value: &[u8]) -> Option<usize> {
    std::str::from_utf8(value).ok()?.parse::<usize>().ok()
}

fn escape_mmap_field(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn decode_mmap_field(value: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(value).ok()?;
    if !text.contains('\\') {
        return Some(text.to_string());
    }

    let mut decoded = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }
        match chars.next()? {
            '\\' => decoded.push('\\'),
            't' => decoded.push('\t'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            other => {
                decoded.push('\\');
                decoded.push(other);
            }
        }
    }
    Some(decoded)
}

fn read_symbols_jsonl_snapshot(root: &Path, index_path: &Path) -> Result<Option<Vec<IndexedSymbol>>> {
    let Some(path) = index_symbols_jsonl_path(root, index_path)? else {
        return Ok(None);
    };
    let Ok(file) = fs::File::open(path) else {
        return Ok(None);
    };
    let reader = BufReader::new(file);
    let mut symbols = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let Ok(symbol) = serde_json::from_str::<IndexedSymbol>(&line) else {
            return Ok(None);
        };
        symbols.push(symbol);
    }
    Ok(Some(symbols))
}

fn list_symbols_jsonl(root: &Path, index_path: &Path, limit: usize) -> Result<Option<Vec<String>>> {
    let Some(path) = index_symbols_jsonl_path(root, index_path)? else {
        return Ok(None);
    };
    let Ok(file) = fs::File::open(path) else {
        return Ok(None);
    };
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines().take(limit.max(1)) {
        let line = line?;
        let Ok(symbol) = serde_json::from_str::<IndexedSymbol>(&line) else {
            return Ok(None);
        };
        entries.push(format_symbol_entry(&symbol));
    }
    Ok(Some(entries))
}

fn query_symbols_jsonl(root: &Path, index_path: &Path, query: &str, limit: usize) -> Result<Option<Vec<String>>> {
    let Some(path) = index_symbols_jsonl_path(root, index_path)? else {
        return Ok(None);
    };
    let Ok(file) = fs::File::open(path) else {
        return Ok(None);
    };
    let reader = BufReader::new(file);
    let query = query.trim();
    let limit = limit.max(1);
    let mut scored = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let Ok(symbol) = serde_json::from_str::<IndexedSymbol>(&line) else {
            return Ok(None);
        };
        if query.is_empty() {
            scored.push((0, format_symbol_entry(&symbol)));
            if scored.len() >= limit {
                break;
            }
            continue;
        }
        let entry = format_symbol_entry(&symbol);
        let haystack = format!("{} {} {} {}", symbol.path, symbol.name, symbol.kind, symbol.detail);
        if let Some(score) = fuzzy_score(&haystack, query) {
            push_bounded_scored(&mut scored, (score, entry), limit);
        }
    }
    Ok(Some(format_scored_entries(scored, limit)))
}

fn query_symbol_items_jsonl(
    root: &Path,
    index_path: &Path,
    query: &str,
    limit: usize,
    cancel: Option<&crate::search::SearchCancel>,
) -> Result<Option<Vec<CodeItem>>> {
    let Some(path) = index_symbols_jsonl_path(root, index_path)? else {
        return Ok(None);
    };
    let Ok(file) = fs::File::open(path) else {
        return Ok(None);
    };
    let reader = BufReader::new(file);
    let query = query.trim();
    let limit = limit.max(1);
    let mut scored = Vec::new();

    for line in reader.lines() {
        if cancel.is_some_and(crate::search::SearchCancel::is_cancelled) {
            return Err(AppError::General("Search cancelled".to_string()));
        }
        let line = line?;
        let Ok(symbol) = serde_json::from_str::<IndexedSymbol>(&line) else {
            return Ok(None);
        };
        let item = symbol_to_code_item(root, &symbol);
        if query.is_empty() {
            scored.push((0, item.display_text().to_string(), item));
            if scored.len() >= limit {
                break;
            }
            continue;
        }
        let haystack = format!("{} {} {} {}", symbol.path, symbol.name, symbol.kind, symbol.detail);
        if let Some(score) = fuzzy_score(&haystack, query) {
            push_bounded_scored_item(&mut scored, (score, item.display_text().to_string(), item), limit);
        }
    }

    Ok(Some(format_scored_items(scored, limit)))
}

fn index_symbols_jsonl_path(root: &Path, index_path: &Path) -> Result<Option<PathBuf>> {
    if read_index_metadata_snapshot(root, index_path)?.is_none() {
        return Ok(None);
    }
    let path = index_symbols_path(root)?;
    if path.exists() {
        return Ok(Some(path));
    }
    Ok(None)
}

fn sidecar_matches_source(
    root: &Path,
    index_path: &Path,
    version: u32,
    sidecar_root: &str,
    source: &IndexSourceSnapshot,
) -> Result<bool> {
    if version != INDEX_VERSION || sidecar_root.trim().is_empty() || normalize_root(Path::new(sidecar_root)) != root {
        return Ok(false);
    }
    let source_index = if source.source_index.trim().is_empty() {
        index_path.to_path_buf()
    } else {
        PathBuf::from(&source.source_index)
    };
    if normalize_root(&source_index) != normalize_root(index_path) {
        return Ok(false);
    }
    let Ok(current) = index_source_snapshot(index_path) else {
        return Ok(false);
    };
    Ok(source.source_index_size_bytes == current.source_index_size_bytes
        && source.source_index_modified_unix == current.source_index_modified_unix)
}

fn index_source_snapshot(path: &Path) -> Result<IndexSourceSnapshot> {
    let metadata = fs::metadata(path)?;
    let modified_unix = metadata.modified().ok().and_then(system_time_to_unix).unwrap_or(0);
    Ok(IndexSourceSnapshot {
        source_index: path.display().to_string(),
        source_index_size_bytes: metadata.len(),
        source_index_modified_unix: modified_unix,
    })
}

fn read_index(path: &Path) -> Result<CodeIndex> {
    let contents = fs::read_to_string(path)?;
    migrate_index_contents(&contents)
}

fn load_recoverable(root: &Path) -> Result<Option<CodeIndex>> {
    let root = normalize_root(root);
    let path = index_path(&root)?;
    if !path.exists() {
        return Ok(None);
    }

    if let Some(index) = read_code_index_sidecars(&root, &path)? {
        return Ok(Some(index));
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
    if file.scan_error.as_deref() == Some("") {
        file.scan_error = None;
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
        content_hash: content_hash(path)?,
        symbol_count: 0,
        last_indexed_unix: now_unix(),
        scan_error: None,
    })
}

fn file_status_metadata(root: &Path, path: &Path) -> Result<IndexedFile> {
    let metadata = fs::metadata(path)?;
    let modified_unix = metadata.modified().ok().and_then(system_time_to_unix).unwrap_or(0);
    let relative = relative_path(root, path);

    Ok(IndexedFile {
        path: relative.clone(),
        size_bytes: metadata.len(),
        modified_unix,
        language: language_for_path(Path::new(&relative)),
        content_hash: String::new(),
        symbol_count: 0,
        last_indexed_unix: 0,
        scan_error: None,
    })
}

fn file_metadata_by_path(files: &[IndexedFile]) -> HashMap<String, IndexedFile> {
    files
        .iter()
        .map(|file| (file.path.clone(), file.clone()))
        .collect::<HashMap<String, IndexedFile>>()
}

fn shard_key(path: &str) -> String {
    path.split(['/', '\\'])
        .find(|part| !part.trim().is_empty())
        .unwrap_or("<root>")
        .to_string()
}

fn sanitize_shard_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches('_').is_empty() {
        "root".to_string()
    } else {
        sanitized
    }
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
    if !left.content_hash.is_empty() && !right.content_hash.is_empty() {
        return left.path == right.path && left.content_hash == right.content_hash;
    }

    left.path == right.path && left.size_bytes == right.size_bytes && left.modified_unix == right.modified_unix
}

fn is_same_file_status_snapshot(left: &IndexedFile, right: &IndexedFile) -> bool {
    left.path == right.path && left.size_bytes == right.size_bytes && left.modified_unix == right.modified_unix
}

fn apply_file_symbol_counts(files: &mut [IndexedFile], symbols: &[IndexedSymbol]) {
    let mut counts = HashMap::<String, usize>::new();
    for symbol in symbols {
        *counts.entry(symbol.path.clone()).or_default() += 1;
    }
    let indexed_at = now_unix();
    for file in files {
        file.symbol_count = counts.get(&file.path).copied().unwrap_or(0);
        file.last_indexed_unix = indexed_at;
        file.scan_error = None;
    }
}

fn sorted_sample<'a>(values: impl Iterator<Item = &'a String>, limit: usize) -> Vec<String> {
    let mut sample = values.cloned().collect::<Vec<String>>();
    sample.sort();
    sample.truncate(limit);
    sample
}

fn content_hash(path: &Path) -> Result<String> {
    let contents = fs::read(path)?;
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
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

fn format_file_entry(file: &IndexedFile) -> String {
    format!("{} [{}] ({} bytes)", file.path, file.language, file.size_bytes)
}

fn file_to_code_item(root: &Path, file: &IndexedFile) -> CodeItem {
    CodeItem::file_with_display(indexed_path(root, &file.path), file.path.clone())
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

fn symbol_to_code_item(root: &Path, symbol: &IndexedSymbol) -> CodeItem {
    let display = format!("{}:{}:{}", symbol.path, symbol.line, symbol.detail);
    CodeItem::from_parts(
        CodeItemKind::Symbol,
        symbol.path.clone(),
        symbol.detail.clone(),
        Location::new(indexed_path(root, &symbol.path), Some(symbol.line), symbol.column),
        display,
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
        let stats = stats(&temp_dir).unwrap();
        let files = list(&temp_dir, IndexListKind::Files, 10).unwrap();
        let symbols = list(&temp_dir, IndexListKind::Symbols, 10).unwrap();
        let queried_files = query(&temp_dir, IndexListKind::Files, "src/main", 10).unwrap();
        let queried = query(&temp_dir, IndexListKind::Symbols, "main", 10).unwrap();
        let symbol_items = query_code_items(&temp_dir, IndexListKind::Symbols, "main", 10)
            .unwrap()
            .expect("index-backed symbol items should be available");

        assert!(report.path.ends_with(INDEX_FILE_NAME));
        assert!(index_meta_path(&temp_dir).unwrap().exists());
        assert!(index_files_path(&temp_dir).unwrap().exists());
        assert!(index_symbols_path(&temp_dir).unwrap().exists());
        assert!(index_symbols_mmap_path(&temp_dir).unwrap().exists());
        assert!(report.file_count >= 2);
        assert_eq!(report.removed_files, 0);
        assert_eq!(index.version, INDEX_VERSION);
        assert!(!index.is_empty());
        assert!(index.files.iter().any(|file| file.path == "src/main.rs"
            && file.language == "rust"
            && !file.content_hash.is_empty()
            && file.last_indexed_unix > 0));
        assert!(index.symbols.iter().any(|symbol| {
            symbol.name == "main"
                && symbol.kind == "function"
                && symbol.language == "rust"
                && symbol.range.start_line == 1
                && symbol.range.start_column >= 1
        }));
        assert!(symbols.iter().any(|symbol| symbol.contains("main [function]")));
        assert!(queried.iter().any(|symbol| symbol.contains("main [function]")));
        assert!(symbol_items.iter().any(|item| {
            item.display_text().contains("src/main.rs:1:main [function]")
                && item.location.path() == temp_dir.join("src").join("main.rs")
        }));
        assert!(files.iter().any(|file| file.contains("src/main.rs")));
        assert!(queried_files.iter().any(|file| file.contains("src/main.rs")));
        assert_eq!(stats.file_count, report.file_count);
        assert_eq!(stats.symbol_count, report.symbol_count);
        assert!(status.exists);
        assert_eq!(status.version, Some(INDEX_VERSION));
        assert_eq!(status.changed_tracked_files, 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn shard_report_sorts_heaviest_buckets_first() {
        let temp_dir = temp_workspace_dir("shards");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("hot")).unwrap();
        fs::create_dir_all(temp_dir.join("cold")).unwrap();
        let cache_dir = crate::workspace::cache_dir_for_root(&temp_dir).unwrap();
        let _ = fs::remove_dir_all(&cache_dir);
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(
            temp_dir.join("hot").join("lib.rs"),
            "pub fn alpha() {}\npub fn beta() {}\npub fn gamma() {}\n",
        )
        .unwrap();
        fs::write(temp_dir.join("cold").join("lib.rs"), "pub fn delta() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");

        build(&temp_dir, &[], &[], &ignore_file).unwrap();
        let report = shard_report(&temp_dir, 2).unwrap();
        let fallback_query = query_shards(&temp_dir, IndexListKind::Symbols, "alpha", 10).unwrap();
        let fallback_report = query_shards_report(&temp_dir, IndexListKind::Symbols, "alpha", 10).unwrap();
        let build_report = build_shards(&temp_dir, 2).unwrap();
        let status = shard_status(&temp_dir).unwrap();
        let sidecar_shard_query = query_shards_report(&temp_dir, IndexListKind::Symbols, "gamma", 10).unwrap();
        fs::remove_file(index_symbols_mmap_path(&temp_dir).unwrap()).unwrap();
        fs::remove_file(index_symbols_path(&temp_dir).unwrap()).unwrap();
        let shard_query = query_shards(&temp_dir, IndexListKind::Symbols, "gamma", 10).unwrap();
        let full_shard_query = query_shards_report(&temp_dir, IndexListKind::Symbols, "gamma", 10).unwrap();
        let hinted_shard_query =
            query_shards_report(&temp_dir, IndexListKind::Symbols, "hot/lib.rs gamma", 10).unwrap();

        assert_eq!(report.recommended_shards, 2);
        assert_eq!(report.buckets[0].name, "hot");
        assert!(report.buckets[0].symbols > report.buckets[1].symbols);
        assert!(fallback_query.iter().any(|entry| entry.contains("alpha [function]")));
        assert!(fallback_report.fallback_used);
        assert!(build_report.wrote);
        assert!(build_report.shard_count >= 2);
        assert!(build_report.shards.iter().any(|shard| shard.name == "hot"));
        assert!(build_report.shards.iter().any(|shard| shard.name == "cold"));
        assert!(build_report.manifest_path.exists());
        assert!(status.exists);
        assert!(!status.stale);
        assert_eq!(status.shard_count, build_report.shard_count);
        assert!(sidecar_shard_query
            .entries
            .iter()
            .any(|entry| entry.contains("gamma [function]")));
        assert_eq!(sidecar_shard_query.shards_scanned, 0);
        assert!(shard_query.iter().any(|entry| entry.contains("gamma [function]")));
        assert_eq!(full_shard_query.shards_scanned, build_report.shard_count);
        assert!(!full_shard_query.fallback_used);
        assert!(hinted_shard_query
            .entries
            .iter()
            .any(|entry| entry.contains("gamma [function]")));
        assert_eq!(hinted_shard_query.shards_scanned, 1);
        assert!(!hinted_shard_query.fallback_used);
        assert!(shard_report(&temp_dir, 0).is_err());
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&cache_dir);
    }

    #[test]
    fn file_snapshot_prefers_content_hash_over_size_and_mtime() {
        let left = IndexedFile {
            path: "src/lib.rs".to_string(),
            size_bytes: 10,
            modified_unix: 100,
            language: "rust".to_string(),
            content_hash: "aaaa".to_string(),
            symbol_count: 1,
            last_indexed_unix: 100,
            scan_error: None,
        };
        let mut right = left.clone();

        assert!(is_same_file_snapshot(&left, &right));

        right.content_hash = "bbbb".to_string();

        assert!(!is_same_file_snapshot(&left, &right));
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
    fn refresh_recreates_missing_lightweight_sidecars_without_rebuild() {
        let temp_dir = temp_workspace_dir("sidecar_refresh");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("lib.rs"), "pub fn first() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");

        build(&temp_dir, &[], &[], &ignore_file).unwrap();
        fs::remove_file(index_meta_path(&temp_dir).unwrap()).unwrap();
        fs::remove_file(index_files_path(&temp_dir).unwrap()).unwrap();
        fs::remove_file(index_symbols_mmap_path(&temp_dir).unwrap()).unwrap();
        let report = refresh(&temp_dir, &[], &[], &ignore_file).unwrap();

        assert!(!report.rebuilt);
        assert_eq!(report.reason, "sidecars refreshed");
        assert!(index_meta_path(&temp_dir).unwrap().exists());
        assert!(index_files_path(&temp_dir).unwrap().exists());
        assert!(index_symbols_mmap_path(&temp_dir).unwrap().exists());

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

    #[test]
    fn polling_daemon_writes_readable_heartbeat() {
        let temp_dir = temp_workspace_dir("daemon");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "pub fn main() {}\n").unwrap();
        let ignore_file = temp_dir.join("missing.ignore");

        let report = run_polling_daemon(
            &temp_dir,
            &[],
            &[],
            &ignore_file,
            IndexDaemonOptions {
                interval_ms: 0,
                max_cycles: Some(1),
            },
        )
        .unwrap();
        let status = daemon_status(&temp_dir).unwrap();

        assert_eq!(report.cycles.len(), 1);
        assert!(report.heartbeat_path.exists());
        assert!(status.exists);
        assert_eq!(status.heartbeat.unwrap().cycles, 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
