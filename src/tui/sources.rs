use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::config::Config;
use crate::core::CodeItem;
use crate::errors::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceMode {
    Search,
    Files,
    Symbols,
    References,
    Diagnostics,
    Trace,
    Pinned,
    Debug,
}

impl SourceMode {
    pub(super) fn all() -> &'static [SourceMode] {
        &[
            SourceMode::Search,
            SourceMode::Files,
            SourceMode::Symbols,
            SourceMode::References,
            SourceMode::Diagnostics,
            SourceMode::Trace,
            SourceMode::Pinned,
            SourceMode::Debug,
        ]
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            SourceMode::Search => "Text Search",
            SourceMode::Files => "Files",
            SourceMode::Symbols => "Symbols",
            SourceMode::References => "References",
            SourceMode::Diagnostics => "Diagnostics",
            SourceMode::Trace => "Trace",
            SourceMode::Pinned => "Pinned",
            SourceMode::Debug => "Debug",
        }
    }

    pub(super) fn short_label(self) -> &'static str {
        match self {
            SourceMode::Search => "search",
            SourceMode::Files => "files",
            SourceMode::Symbols => "symbols",
            SourceMode::References => "refs",
            SourceMode::Diagnostics => "diag",
            SourceMode::Trace => "trace",
            SourceMode::Pinned => "pins",
            SourceMode::Debug => "debug",
        }
    }

    pub(super) fn from_short_label(value: &str) -> Option<Self> {
        match value {
            "search" | "text" => Some(SourceMode::Search),
            "files" | "file" => Some(SourceMode::Files),
            "symbols" | "symbol" => Some(SourceMode::Symbols),
            "refs" | "references" => Some(SourceMode::References),
            "diag" | "diagnostics" => Some(SourceMode::Diagnostics),
            "trace" => Some(SourceMode::Trace),
            "pin" | "pins" | "pinned" => Some(SourceMode::Pinned),
            "debug" => Some(SourceMode::Debug),
            _ => None,
        }
    }
}

trait SourceProvider {
    fn load(&self, request: &SourceRequest) -> Result<Vec<CodeItem>>;
}

struct TextSearchSource;
struct FilesSource;
struct SymbolsSource;

impl SourceProvider for TextSearchSource {
    fn load(&self, request: &SourceRequest) -> Result<Vec<CodeItem>> {
        if request.query.trim().is_empty() {
            return Ok(Vec::new());
        }

        crate::history::record(
            "tui-search",
            &request.query,
            Some(&request.root.to_string_lossy().to_string()),
        )?;
        let mut options = request.config.search.rg_options.clone();
        options.push("-S".to_string());
        let dir = request.root.to_string_lossy().to_string();
        let results = crate::search::search_with_cancel(
            &request.query,
            Some(&dir),
            &options,
            &request.config.search.ignore,
            &request.ignore_path,
            Some(&request.cancel),
            Some(2000),
        )?;

        Ok(results
            .flat()
            .into_iter()
            .map(|result| {
                let path = resolve_path(&request.root, PathBuf::from(result.path));
                CodeItem::text_match(path, result.line_num, None, result.line_text)
            })
            .collect())
    }
}

impl SourceProvider for FilesSource {
    fn load(&self, request: &SourceRequest) -> Result<Vec<CodeItem>> {
        let dir = request.root.to_string_lossy().to_string();
        let items = crate::files::find_files(Some(&dir), &[], &request.config.search.ignore, &request.ignore_path)?;
        Ok(filter_items(items, &request.query))
    }
}

impl SourceProvider for SymbolsSource {
    fn load(&self, request: &SourceRequest) -> Result<Vec<CodeItem>> {
        let dir = request.root.to_string_lossy().to_string();
        let items = crate::symbols::find_symbols(Some(&dir), &[], &request.config.search.ignore, &request.ignore_path)?;
        Ok(filter_items(items, &request.query))
    }
}

#[derive(Debug)]
pub(super) struct SourceRequest {
    pub(super) id: u64,
    pub(super) mode: SourceMode,
    root: PathBuf,
    ignore_path: PathBuf,
    query: String,
    config: Config,
    cancel: crate::search::SearchCancel,
}

#[derive(Debug)]
pub(super) struct SourceResponse {
    pub(super) id: u64,
    pub(super) mode: SourceMode,
    pub(super) query: String,
    pub(super) result: Result<Vec<CodeItem>>,
}

#[derive(Debug)]
pub(super) struct SourceWorker {
    pub(super) sender: Sender<SourceRequest>,
    pub(super) receiver: Receiver<SourceResponse>,
    pub(super) next_id: u64,
    pub(super) latest_id: u64,
    pub(super) latest_cancel: Option<crate::search::SearchCancel>,
}

impl SourceWorker {
    pub(super) fn start() -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<SourceRequest>();
        let (response_sender, response_receiver) = mpsc::channel::<SourceResponse>();

        thread::spawn(move || {
            while let Ok(mut request) = request_receiver.recv() {
                while let Ok(newer_request) = request_receiver.try_recv() {
                    request = newer_request;
                }
                let result = run_source_request(&request);
                if response_sender
                    .send(SourceResponse {
                        id: request.id,
                        mode: request.mode,
                        query: request.query,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            sender: request_sender,
            receiver: response_receiver,
            next_id: 1,
            latest_id: 0,
            latest_cancel: None,
        }
    }

    pub(super) fn request(
        &mut self,
        mode: SourceMode,
        root: &Path,
        ignore_path: &Path,
        query: &str,
        config: &Config,
    ) -> Result<u64> {
        if let Some(cancel) = self.latest_cancel.take() {
            cancel.cancel();
        }

        let id = self.next_id;
        self.next_id += 1;
        self.latest_id = id;
        let cancel = crate::search::SearchCancel::default();
        self.latest_cancel = Some(cancel.clone());
        self.sender
            .send(SourceRequest {
                id,
                mode,
                root: root.to_path_buf(),
                ignore_path: ignore_path.to_path_buf(),
                query: query.to_string(),
                config: config.clone(),
                cancel,
            })
            .map_err(|err| AppError::General(err.to_string()))?;
        Ok(id)
    }

    pub(super) fn try_recv_latest(&mut self) -> Option<SourceResponse> {
        let mut latest = None;
        while let Ok(response) = self.receiver.try_recv() {
            if response.id == self.latest_id {
                latest = Some(response);
                self.latest_cancel = None;
            }
        }
        latest
    }
}

fn run_source_request(request: &SourceRequest) -> Result<Vec<CodeItem>> {
    let Some(provider) = provider_for(request.mode) else {
        return Ok(Vec::new());
    };

    provider.load(request)
}

fn provider_for(mode: SourceMode) -> Option<Box<dyn SourceProvider + Send>> {
    match mode {
        SourceMode::Search => Some(Box::new(TextSearchSource)),
        SourceMode::Files => Some(Box::new(FilesSource)),
        SourceMode::Symbols => Some(Box::new(SymbolsSource)),
        _ => None,
    }
}

fn filter_items(items: Vec<CodeItem>, query: &str) -> Vec<CodeItem> {
    if query.trim().is_empty() {
        return items;
    }

    let query = query.to_lowercase();
    let mut scored = items
        .into_iter()
        .filter_map(|item| fuzzy_score(item.display_text(), &query).map(|score| (score, item)))
        .collect::<Vec<(usize, CodeItem)>>();
    scored.sort_by_key(|(score, item)| (*score, item.display_text().to_string()));
    scored.into_iter().map(|(_, item)| item).collect()
}

pub(super) fn fuzzy_score(value: &str, query: &str) -> Option<usize> {
    let value = value.to_lowercase();
    if value.contains(query) {
        return Some(value.find(query).unwrap_or(0));
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

pub(super) fn parse_mode(mode: Option<&str>) -> Result<SourceMode> {
    match mode.unwrap_or("search") {
        "search" | "text" => Ok(SourceMode::Search),
        "files" | "file" => Ok(SourceMode::Files),
        "symbols" | "symbol" => Ok(SourceMode::Symbols),
        "refs" | "references" => Ok(SourceMode::References),
        "diag" | "diagnostics" => Ok(SourceMode::Diagnostics),
        "trace" => Ok(SourceMode::Trace),
        "pin" | "pins" | "pinned" => Ok(SourceMode::Pinned),
        "debug" => Ok(SourceMode::Debug),
        other => Err(AppError::General(format!("Unknown TUI mode: {other}"))),
    }
}

pub(super) fn source_mode_after(mode: SourceMode, delta: isize) -> SourceMode {
    let modes = SourceMode::all();
    let index = modes.iter().position(|candidate| *candidate == mode).unwrap_or(0) as isize;
    let next = (index + delta).rem_euclid(modes.len() as isize);
    modes[next as usize]
}

pub(super) fn tracking_mode_after(mode: SourceMode, delta: isize) -> SourceMode {
    let modes = [
        SourceMode::Search,
        SourceMode::References,
        SourceMode::Symbols,
        SourceMode::Diagnostics,
        SourceMode::Debug,
    ];
    let index = modes.iter().position(|candidate| *candidate == mode).unwrap_or(0) as isize;
    let next = (index + delta).rem_euclid(modes.len() as isize);
    modes[next as usize]
}

fn resolve_path(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() || path.exists() {
        path
    } else {
        root.join(path)
    }
}

pub(super) fn resolve_ignore_file(root: &Path) -> PathBuf {
    let local_ignore = root.join(".ignore");
    if local_ignore.exists() {
        return local_ignore;
    }

    let basename = root.file_name().and_then(|name| name.to_str()).unwrap_or("root");
    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    let hash = format!("{:08x}", hasher.finish() as u32);
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("fcs")
        .join(format!("{basename}-{hash}.ignore"))
}
