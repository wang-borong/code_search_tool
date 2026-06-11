use std::collections::{HashMap, VecDeque};
use std::io::BufRead;
use std::path::PathBuf;

use crate::core::Location;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreviewKey {
    path: PathBuf,
    line: usize,
    height: u16,
    scroll: isize,
}

#[derive(Debug)]
pub(super) struct PreviewCache {
    capacity: usize,
    keys: VecDeque<PreviewKey>,
    entries: HashMap<PreviewKey, PreviewWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreviewLine {
    pub(super) number: usize,
    pub(super) text: String,
    pub(super) is_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreviewWindow {
    pub(super) path: PathBuf,
    pub(super) target_line: usize,
    pub(super) target_column: Option<usize>,
    pub(super) lines: Vec<PreviewLine>,
    pub(super) message: Option<String>,
}

impl PreviewWindow {
    pub(super) fn message(message: impl Into<String>) -> Self {
        Self {
            path: PathBuf::new(),
            target_line: 1,
            target_column: None,
            lines: Vec::new(),
            message: Some(message.into()),
        }
    }

    #[cfg(test)]
    pub(super) fn plain_text(&self) -> String {
        if let Some(message) = &self.message {
            return message.clone();
        }

        let mut output = String::new();
        for line in &self.lines {
            let marker = if line.is_target { ">" } else { " " };
            output.push_str(&format!("{marker} {:>5} | {}\n", line.number, line.text));
        }

        if output.is_empty() {
            format!("No preview lines in {}", self.path.display())
        } else {
            output
        }
    }
}

impl PreviewCache {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            keys: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn text(&mut self, location: &Location, height: u16) -> String {
        self.text_with_scroll(location, height, 0)
    }

    #[cfg(test)]
    pub(super) fn text_with_scroll(&mut self, location: &Location, height: u16, scroll: isize) -> String {
        self.window_with_scroll(location, height, scroll).plain_text()
    }

    pub(super) fn window_with_scroll(&mut self, location: &Location, height: u16, scroll: isize) -> PreviewWindow {
        let key = PreviewKey {
            path: location.path.clone(),
            line: location.line.unwrap_or(1),
            height: height.max(1),
            scroll,
        };

        if let Some(window) = self.entries.get(&key) {
            return window.clone();
        }

        let window = read_preview_window(location, height.max(1), scroll);
        self.insert(key, window.clone());
        window
    }

    fn insert(&mut self, key: PreviewKey, window: PreviewWindow) {
        if let Some(entry) = self.entries.get_mut(&key) {
            *entry = window;
            return;
        }

        self.keys.push_back(key.clone());
        self.entries.insert(key, window);

        while self.entries.len() > self.capacity {
            let Some(old_key) = self.keys.pop_front() else {
                break;
            };
            self.entries.remove(&old_key);
        }
    }
}

fn read_preview_window(location: &Location, height: u16, scroll: isize) -> PreviewWindow {
    let line = location.line.unwrap_or(1);
    let path = location.path();
    let Ok(file) = std::fs::File::open(path) else {
        return PreviewWindow::message(format!("Could not read {}", path.display()));
    };

    let context = (height as usize).saturating_sub(2).max(3);
    let before = context / 2;
    let base_start = line.saturating_sub(before).max(1);
    let start = if scroll < 0 {
        base_start.saturating_sub(scroll.unsigned_abs()).max(1)
    } else {
        base_start.saturating_add(scroll as usize).max(1)
    };
    let end = start + context;
    let mut lines = Vec::new();
    let reader = std::io::BufReader::new(file);

    for (index, text) in reader.lines().enumerate() {
        let number = index + 1;
        if number < start {
            continue;
        }
        if number > end {
            break;
        }
        let text = text.unwrap_or_default();
        lines.push(PreviewLine {
            number,
            text,
            is_target: number == line,
        });
    }

    if lines.is_empty() {
        PreviewWindow::message(format!("No preview lines in {}", path.display()))
    } else {
        PreviewWindow {
            path: path.to_path_buf(),
            target_line: line,
            target_column: location.column,
            lines,
            message: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_reuses_preview_text() {
        let dir = std::env::temp_dir().join(format!("fcs_preview_cache_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.rs");
        std::fs::write(&path, "one\ntwo\nthree\nfour\n").unwrap();

        let mut cache = PreviewCache::new(2);
        let location = Location::new(&path, Some(2), None);
        let first = cache.text(&location, 8);
        let second = cache.text(&location, 8);

        assert_eq!(first, second);
        assert!(first.contains(">     2 | two"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_scroll_changes_window() {
        let dir = std::env::temp_dir().join(format!("fcs_preview_scroll_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.rs");
        std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\nsix\nseven\n").unwrap();

        let mut cache = PreviewCache::new(4);
        let location = Location::new(&path, Some(4), None);
        let first = cache.text_with_scroll(&location, 5, 0);
        let second = cache.text_with_scroll(&location, 5, 2);

        assert_ne!(first, second);
        assert!(second.contains("six"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
