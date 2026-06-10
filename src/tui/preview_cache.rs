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
    entries: HashMap<PreviewKey, String>,
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

    pub(super) fn text_with_scroll(&mut self, location: &Location, height: u16, scroll: isize) -> String {
        let key = PreviewKey {
            path: location.path.clone(),
            line: location.line.unwrap_or(1),
            height: height.max(1),
            scroll,
        };

        if let Some(text) = self.entries.get(&key) {
            return text.clone();
        }

        let text = read_preview_window(location, height.max(1), scroll);
        self.insert(key, text.clone());
        text
    }

    fn insert(&mut self, key: PreviewKey, text: String) {
        if let Some(entry) = self.entries.get_mut(&key) {
            *entry = text;
            return;
        }

        self.keys.push_back(key.clone());
        self.entries.insert(key, text);

        while self.entries.len() > self.capacity {
            let Some(old_key) = self.keys.pop_front() else {
                break;
            };
            self.entries.remove(&old_key);
        }
    }
}

fn read_preview_window(location: &Location, height: u16, scroll: isize) -> String {
    let line = location.line.unwrap_or(1);
    let path = location.path();
    let Ok(file) = std::fs::File::open(path) else {
        return format!("Could not read {}", path.display());
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
    let mut output = String::new();
    let reader = std::io::BufReader::new(file);

    for (index, text) in reader.lines().enumerate() {
        let number = index + 1;
        if number < start {
            continue;
        }
        if number > end {
            break;
        }
        let marker = if number == line { ">" } else { " " };
        let text = text.unwrap_or_default();
        output.push_str(&format!("{marker} {number:>5} | {text}\n"));
    }

    if output.is_empty() {
        format!("No preview lines in {}", path.display())
    } else {
        output
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
