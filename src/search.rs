use std::path::Path;
use std::borrow::Cow;

use ignore::WalkBuilder;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{Searcher, sinks::UTF8};
use skim::prelude::*;

use crate::errors::{AppError, Result};

/// A single search result: file path, line number, matched text.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub line_num: usize,
    pub line_text: String,
    /// Full "path:line:text" string for display
    pub display: String,
}

impl SkimItem for SearchResult {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.display)
    }

    fn preview(&self, context: PreviewContext) -> ItemPreview {
        match crate::preview::preview_to_string(self, context.height) {
            Ok(ansi_text) => ItemPreview::AnsiText(ansi_text),
            Err(e) => ItemPreview::Text(format!("Failed to generate preview: {e}")),
        }
    }
}

/// Search results grouped by file.
#[derive(Debug, Clone)]
pub struct SearchResults {
    pub by_file: Vec<(String, Vec<SearchResult>)>,
}

impl SearchResults {
    /// Flatten into a single list with a "path:line:text" display string.
    pub fn flat(&self) -> Vec<SearchResult> {
        self.by_file
            .iter()
            .flat_map(|(_, results)| results.clone())
            .collect()
    }
}

/// Run a regex search over the directory tree using the official ripgrep library crates.
pub fn search(
    pattern: &str,
    dir: Option<&String>,
    rg_opts: &[String],
) -> Result<SearchResults> {
    let case_insensitive = rg_opts.iter().any(|o| o == "-i" || o == "--ignore-case");
    
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(case_insensitive)
        .build(pattern)
        .map_err(|e| AppError::General(e.to_string()))?;

    let root = dir
        .map(|d| Path::new(d).to_path_buf())
        .unwrap_or_else(|| Path::new(".").to_path_buf());

    let mut builder = WalkBuilder::new(&root);
    builder
        .hidden(false)
        .standard_filters(true)
        .ignore(true)
        .git_ignore(true)
        .git_global(true);

    if rg_opts.iter().any(|o| o == "--no-ignore") {
        builder.ignore(false);
        builder.hidden(false);
    }

    let walker = builder.build();

    let mut by_file: std::collections::BTreeMap<String, Vec<SearchResult>> =
        std::collections::BTreeMap::new();

    let mut searcher = Searcher::new();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let path = entry.path();
        let rel_path = path_to_relative(path);
        let mut file_results = Vec::new();

        // Perform search using ripgrep searcher.
        // Borrow rel_path and file_results instead of moving them.
        let rel_path_clone = rel_path.clone();
        let search_res = searcher.search_path(
            &matcher,
            path,
            UTF8(|line_num, line_text| {
                let line_num = line_num as usize;
                // strip trailing newlines
                let clean_line = line_text.trim_end_matches(&['\r', '\n'][..]).to_string();
                let display = format!("{rel_path_clone}:{line_num}:{clean_line}");
                file_results.push(SearchResult {
                    path: rel_path_clone.clone(),
                    line_num,
                    line_text: clean_line,
                    display,
                });
                Ok(true)
            }),
        );

        // If search succeeded and we found matches, insert them
        if search_res.is_ok() && !file_results.is_empty() {
            by_file.insert(rel_path, file_results);
        }
    }

    Ok(SearchResults {
        by_file: by_file.into_iter().collect(),
    })
}

/// Open a file at a specific line using the `edit` crate, passing line number directly via VISUAL/EDITOR.
pub fn open_file(path: &str, line: Option<usize>) -> Result<()> {
    let file_path = Path::new(path);
    if !file_path.exists() {
        return Err(AppError::FileNotFound(path.to_string()));
    }

    let old_visual = std::env::var("VISUAL").ok();
    let old_editor = std::env::var("EDITOR").ok();

    if let Some(line_num) = line {
        let current_editor = old_visual.clone()
            .or_else(|| old_editor.clone())
            .unwrap_or_else(|| "nvim".to_string());
        // Extract the editor binary (strip any pre-existing arguments)
        let editor_bin = current_editor.split_whitespace().next().unwrap_or("nvim");
        
        let editor_cmd = format!("{} +{}", editor_bin, line_num);
        std::env::set_var("VISUAL", &editor_cmd);
        std::env::set_var("EDITOR", &editor_cmd);
    }

    let res = edit::edit_file(file_path);

    // Restore original env vars
    if let Some(val) = old_visual {
        std::env::set_var("VISUAL", val);
    } else {
        std::env::remove_var("VISUAL");
    }
    if let Some(val) = old_editor {
        std::env::set_var("EDITOR", val);
    } else {
        std::env::remove_var("EDITOR");
    }

    res.map_err(AppError::Io)?;
    Ok(())
}

fn path_to_relative(path: &Path) -> String {
    path.strip_prefix(".")
        .map(|s| s.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"))
}
