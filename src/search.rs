use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use grep_regex::RegexMatcherBuilder;
use grep_searcher::{sinks::UTF8, SearcherBuilder};
use ignore::WalkBuilder;
use ratatui::text::Line;
use skim::prelude::*;

use crate::errors::{AppError, Result};

#[derive(Debug, Clone, Default)]
pub struct SearchCancel {
    cancelled: Arc<AtomicBool>,
}

impl SearchCancel {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// A single search result: file path, line number, matched text.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub line_num: usize,
    pub line_text: String,
    /// Full "path:line:text" string for display
    pub display: String,
    /// Match ranges of the search term in line_text (char start, char end)
    pub match_ranges: Vec<(usize, usize)>,
}

impl SkimItem for SearchResult {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.display)
    }

    fn display(&self, context: DisplayContext) -> Line<'_> {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};

        let path_len = self.path.chars().count();
        let line_str = self.line_num.to_string();
        let line_len = line_str.chars().count();
        let char_text_offset = path_len + 2 + line_len;

        // Define base styles for different parts
        let path_style = Style::default().fg(Color::Cyan);
        let separator_style = Style::default().fg(Color::DarkGray);
        let line_style = Style::default().fg(Color::Green);
        let match_term_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
        let text_style = Style::default();

        // Get highlighted positions from context matches
        let highlight_positions: std::collections::HashSet<usize> = match context.matches {
            Matches::CharIndices(ref indices) => indices.iter().copied().collect(),
            Matches::CharRange(start, end) => (start..end).collect(),
            Matches::ByteRange(start, end) => {
                let char_start = self.display.get(0..start).map_or(0, |s| s.chars().count());
                let char_end = self
                    .display
                    .get(0..end)
                    .map_or(self.display.chars().count(), |s| s.chars().count());
                (char_start..char_end).collect()
            }
            Matches::None => std::collections::HashSet::new(),
        };

        let mut spans = Vec::new();
        let mut current_text = String::new();
        let mut current_style = Style::default();
        let mut style_initialized = false;

        for (i, ch) in self.display.chars().enumerate() {
            let is_match_term = if i >= char_text_offset {
                let j = i - char_text_offset;
                self.match_ranges.iter().any(|&(start, end)| start <= j && j < end)
            } else {
                false
            };

            // Determine the style of this character based on its position
            let mut char_style = if i < path_len {
                path_style
            } else if i == path_len {
                separator_style
            } else if i < path_len + 1 + line_len {
                line_style
            } else if i == path_len + 1 + line_len {
                separator_style
            } else if is_match_term {
                match_term_style
            } else {
                text_style
            };

            // Merge with context styles (patch)
            if highlight_positions.contains(&i) {
                char_style = char_style.patch(context.matched_style);
            } else {
                char_style = char_style.patch(context.base_style);
            }

            if !style_initialized {
                current_style = char_style;
                style_initialized = true;
            }

            if char_style != current_style {
                // Push the accumulated span
                if !current_text.is_empty() {
                    spans.push(Span::styled(current_text.clone(), current_style));
                    current_text.clear();
                }
                current_style = char_style;
            }
            current_text.push(ch);
        }

        if !current_text.is_empty() {
            spans.push(Span::styled(current_text, current_style));
        }

        Line::from(spans)
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
        self.by_file.iter().flat_map(|(_, results)| results.clone()).collect()
    }
}

/// Run a regex search over the directory tree using the official ripgrep library crates.
pub fn search(
    pattern: &str,
    dir: Option<&String>,
    rg_opts: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<SearchResults> {
    let paths: &[String] = match dir {
        Some(dir) => std::slice::from_ref(dir),
        None => &[],
    };
    search_paths(pattern, paths, rg_opts, default_ignore, ignore_file)
}

/// Run a regex search over one or more files/directories.
pub fn search_paths(
    pattern: &str,
    paths: &[String],
    rg_opts: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<SearchResults> {
    search_with_cancel_paths(pattern, paths, rg_opts, default_ignore, ignore_file, None, None)
}

pub fn search_with_cancel(
    pattern: &str,
    dir: Option<&String>,
    rg_opts: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
    cancel: Option<&SearchCancel>,
    max_results: Option<usize>,
) -> Result<SearchResults> {
    let paths: &[String] = match dir {
        Some(dir) => std::slice::from_ref(dir),
        None => &[],
    };
    search_with_cancel_paths(
        pattern,
        paths,
        rg_opts,
        default_ignore,
        ignore_file,
        cancel,
        max_results,
    )
}

pub fn search_with_cancel_paths(
    pattern: &str,
    paths: &[String],
    rg_opts: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
    cancel: Option<&SearchCancel>,
    max_results: Option<usize>,
) -> Result<SearchResults> {
    let mut case_insensitive = false;
    let mut smart_case = false;
    let mut fixed_strings = false;
    let mut word_regexp = false;
    let mut line_regexp = false;
    let mut invert_match = false;
    let mut max_count = None;
    let mut follow_links = false;
    let mut max_depth = None;
    let mut no_ignore = false;

    let mut i = 0;
    while i < rg_opts.len() {
        let opt = &rg_opts[i];
        if let Some(stripped) = opt.strip_prefix("--") {
            let (name, val) = if let Some(pos) = stripped.find('=') {
                (&stripped[..pos], Some(&stripped[pos + 1..]))
            } else {
                (stripped, None)
            };

            match name {
                "ignore-case" => {
                    case_insensitive = true;
                    smart_case = false;
                }
                "case-sensitive" => {
                    case_insensitive = false;
                    smart_case = false;
                }
                "smart-case" => {
                    smart_case = true;
                    case_insensitive = false;
                }
                "fixed-strings" => {
                    fixed_strings = true;
                }
                "word-regexp" => {
                    word_regexp = true;
                }
                "line-regexp" => {
                    line_regexp = true;
                }
                "invert-match" => {
                    invert_match = true;
                }
                "follow" => {
                    follow_links = true;
                }
                "no-ignore" => {
                    no_ignore = true;
                }
                "max-count" => {
                    if let Some(v) = val {
                        if let Ok(num) = v.parse::<usize>() {
                            max_count = Some(num);
                        }
                    } else if i + 1 < rg_opts.len() {
                        i += 1;
                        if let Ok(num) = rg_opts[i].parse::<usize>() {
                            max_count = Some(num);
                        }
                    }
                }
                "max-depth" => {
                    if let Some(v) = val {
                        if let Ok(num) = v.parse::<usize>() {
                            max_depth = Some(num);
                        }
                    } else if i + 1 < rg_opts.len() {
                        i += 1;
                        if let Ok(num) = rg_opts[i].parse::<usize>() {
                            max_depth = Some(num);
                        }
                    }
                }
                _ => {}
            }
        } else if opt.starts_with('-') && opt.len() > 1 {
            let mut chars = opt.chars().skip(1).peekable();
            while let Some(ch) = chars.next() {
                match ch {
                    'i' => {
                        case_insensitive = true;
                        smart_case = false;
                    }
                    's' => {
                        case_insensitive = false;
                        smart_case = false;
                    }
                    'S' => {
                        smart_case = true;
                        case_insensitive = false;
                    }
                    'F' => {
                        fixed_strings = true;
                    }
                    'w' => {
                        word_regexp = true;
                    }
                    'x' => {
                        line_regexp = true;
                    }
                    'v' => {
                        invert_match = true;
                    }
                    'L' => {
                        follow_links = true;
                    }
                    'm' => {
                        let mut rest = String::new();
                        while let Some(&c) = chars.peek() {
                            if c.is_ascii_digit() {
                                rest.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        if !rest.is_empty() {
                            if let Ok(num) = rest.parse::<usize>() {
                                max_count = Some(num);
                            }
                        } else if i + 1 < rg_opts.len() {
                            i += 1;
                            if let Ok(num) = rg_opts[i].parse::<usize>() {
                                max_count = Some(num);
                            }
                        }
                    }
                    'd' => {
                        let mut rest = String::new();
                        while let Some(&c) = chars.peek() {
                            if c.is_ascii_digit() {
                                rest.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        if !rest.is_empty() {
                            if let Ok(num) = rest.parse::<usize>() {
                                max_depth = Some(num);
                            }
                        } else if i + 1 < rg_opts.len() {
                            i += 1;
                            if let Ok(num) = rg_opts[i].parse::<usize>() {
                                max_depth = Some(num);
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else {
            match opt.as_str() {
                "no-ignore" => {
                    no_ignore = true;
                }
                "ignore-case" => {
                    case_insensitive = true;
                    smart_case = false;
                }
                "case-sensitive" => {
                    case_insensitive = false;
                    smart_case = false;
                }
                "smart-case" => {
                    smart_case = true;
                    case_insensitive = false;
                }
                "fixed-strings" => {
                    fixed_strings = true;
                }
                "word-regexp" => {
                    word_regexp = true;
                }
                "line-regexp" => {
                    line_regexp = true;
                }
                "invert-match" => {
                    invert_match = true;
                }
                "follow" => {
                    follow_links = true;
                }
                _ => {}
            }
        }
        i += 1;
    }

    let escaped_pattern = if fixed_strings {
        regex::escape(pattern)
    } else {
        pattern.to_string()
    };

    let mut matcher_pattern = escaped_pattern.clone();
    if line_regexp {
        matcher_pattern = format!(r"^(?:{})$", matcher_pattern);
    }

    let mut matcher_builder = RegexMatcherBuilder::new();
    matcher_builder
        .case_insensitive(case_insensitive)
        .case_smart(smart_case)
        .word(word_regexp);

    let matcher = matcher_builder
        .build(&matcher_pattern)
        .map_err(|e| AppError::General(e.to_string()))?;

    let re_case_insensitive = if smart_case {
        !escaped_pattern.chars().any(|c| c.is_uppercase())
    } else {
        case_insensitive
    };

    let mut re_pattern = escaped_pattern;
    if word_regexp {
        re_pattern = format!(r"\b({})\b", re_pattern);
    }
    if line_regexp {
        re_pattern = format!(r"^({})$", re_pattern);
    }

    let re = regex::RegexBuilder::new(&re_pattern)
        .case_insensitive(re_case_insensitive)
        .build()
        .map_err(|e| AppError::General(e.to_string()))?;

    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![Path::new(".").to_path_buf()]
    } else {
        paths.iter().map(|path| Path::new(path).to_path_buf()).collect()
    };

    let mut by_file: std::collections::BTreeMap<String, Vec<SearchResult>> = std::collections::BTreeMap::new();
    let mut total_results = 0usize;

    let mut searcher = SearcherBuilder::new().invert_match(invert_match).build();

    for root in roots {
        if cancel.is_some_and(SearchCancel::is_cancelled) {
            return Err(AppError::General("Search cancelled".to_string()));
        }
        if max_results.is_some_and(|limit| total_results >= limit) {
            break;
        }

        let mut builder = WalkBuilder::new(&root);
        builder
            .hidden(false)
            .standard_filters(true)
            .ignore(true)
            .git_ignore(true)
            .git_global(true)
            .follow_links(follow_links);

        if let Some(depth) = max_depth {
            builder.max_depth(Some(depth));
        }

        if no_ignore {
            builder.ignore(false);
            builder.git_ignore(false);
            builder.git_global(false);
            builder.parents(false);
        } else {
            if ignore_file.exists() {
                if let Some(err) = builder.add_ignore(ignore_file) {
                    return Err(AppError::General(format!("Failed to add ignore file: {err}")));
                }
            }
            if !default_ignore.is_empty() {
                let mut ovr = ignore::overrides::OverrideBuilder::new(&root);
                for pat in default_ignore {
                    let pat = if pat.starts_with('!') {
                        pat.clone()
                    } else {
                        format!("!{}", pat)
                    };
                    ovr.add(&pat).map_err(|e| AppError::General(e.to_string()))?;
                }
                let overrides = ovr.build().map_err(|e| AppError::General(e.to_string()))?;
                builder.overrides(overrides);
            }
        }

        for entry in builder.build() {
            if cancel.is_some_and(SearchCancel::is_cancelled) {
                return Err(AppError::General("Search cancelled".to_string()));
            }
            if max_results.is_some_and(|limit| total_results >= limit) {
                break;
            }

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
                    if cancel.is_some_and(SearchCancel::is_cancelled) {
                        return Ok(false);
                    }
                    if max_results.is_some_and(|limit| total_results >= limit) {
                        return Ok(false);
                    }
                    if let Some(limit) = max_count {
                        if limit == 0 {
                            return Ok(false);
                        }
                    }

                    let line_num = line_num as usize;
                    // strip trailing newlines
                    let clean_line = line_text.trim_end_matches(&['\r', '\n'][..]).to_string();
                    let display = format!("{rel_path_clone}:{line_num}:{clean_line}");

                    let mut match_ranges = Vec::new();
                    for m in re.find_iter(&clean_line) {
                        let char_start = clean_line[..m.start()].chars().count();
                        let char_end = char_start + clean_line[m.start()..m.end()].chars().count();
                        match_ranges.push((char_start, char_end));
                    }

                    file_results.push(SearchResult {
                        path: rel_path_clone.clone(),
                        line_num,
                        line_text: clean_line,
                        display,
                        match_ranges,
                    });
                    total_results += 1;

                    if let Some(limit) = max_count {
                        if file_results.len() >= limit {
                            return Ok(false);
                        }
                    }

                    Ok(true)
                }),
            );

            // If search succeeded and we found matches, insert them.
            if search_res.is_ok() && !file_results.is_empty() {
                by_file.entry(rel_path).or_default().extend(file_results);
            }

            if cancel.is_some_and(SearchCancel::is_cancelled) {
                return Err(AppError::General("Search cancelled".to_string()));
            }
        }
    }

    Ok(SearchResults {
        by_file: by_file.into_iter().collect(),
    })
}

/// Open a file at a specific line using the shared editor adapter.
pub fn open_file(path: &str, line: Option<usize>) -> Result<()> {
    crate::editor::open_file(Path::new(path), line, None, None)
}

fn path_to_relative(path: &Path) -> String {
    path.strip_prefix(".")
        .map(|s| s.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_search_paths_searches_multiple_files() {
        let temp_dir = std::env::temp_dir().join(format!("fcs_search_multi_path_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let install_path = temp_dir.join("install.sh");
        let readme_path = temp_dir.join("README.md");
        std::fs::write(&install_path, "needle in install\n").unwrap();
        std::fs::write(&readme_path, "needle in readme\n").unwrap();

        let paths = vec![
            install_path.to_string_lossy().to_string(),
            readme_path.to_string_lossy().to_string(),
        ];
        let ignore_file = temp_dir.join("nonexistent.ignore");
        let res = search_paths("needle", &paths, &[], &[], &ignore_file).unwrap();
        let flat = res.flat();

        assert_eq!(flat.len(), 2);
        assert!(flat.iter().any(|result| result.path.ends_with("install.sh")));
        assert!(flat.iter().any(|result| result.path.ends_with("README.md")));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_search_options() {
        let temp_dir = std::env::temp_dir().join("fcs_search_test_dir");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_path = temp_dir.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Hello World").unwrap();
        writeln!(file, "hello rust").unwrap();
        writeln!(file, "case SENSITIVE line").unwrap();
        writeln!(file, "exact pattern.* here").unwrap();
        writeln!(file, "word boundary").unwrap();
        writeln!(file, "wholeline").unwrap();
        drop(file);

        let dir_str = temp_dir.to_string_lossy().to_string();
        let ignore_file = temp_dir.join("nonexistent.ignore");

        // 1. Test case sensitive (default or -s)
        let res = search("hello", Some(&dir_str), &["-s".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 1); // Only "hello rust" should match, not "Hello World"

        // 2. Test case insensitive (-i)
        let res = search("hello", Some(&dir_str), &["-i".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 2); // Both "Hello World" and "hello rust"

        // 3. Test smart-case (-S)
        // lowercase pattern -> case insensitive
        let res = search("hello", Some(&dir_str), &["-S".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 2);
        // uppercase pattern -> case sensitive
        let res = search("Hello", Some(&dir_str), &["-S".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 1);

        // 4. Test fixed strings (-F)
        // without -F, "pattern.+" matches "pattern" followed by one or more characters (e.g. pattern.*)
        let res = search("pattern.+", Some(&dir_str), &[], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 1);
        // With -F, it should match the literal "pattern.+" which does not exist in the file (only "pattern.*" exists)
        let res = search("pattern.+", Some(&dir_str), &["-F".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 0);

        // 5. Test word regexp (-w)
        let res = search("bound", Some(&dir_str), &["-w".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 0); // "bound" is part of "boundary", so no word match
        let res = search("boundary", Some(&dir_str), &["-w".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 1); // matches "word boundary"

        // 6. Test line regexp (-x)
        let res = search("whole", Some(&dir_str), &["-x".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 0); // "whole" is substring of "wholeline"
        let res = search("wholeline", Some(&dir_str), &["-x".to_string()], &[], &ignore_file).unwrap();
        assert_eq!(res.flat().len(), 1); // exact line match

        // 7. Test invert match (-v)
        let res = search(
            "hello",
            Some(&dir_str),
            &["-v".to_string(), "-i".to_string()],
            &[],
            &ignore_file,
        )
        .unwrap();
        // hello matches 2 lines, total lines = 6, so inverted matches = 4 lines.
        assert_eq!(res.flat().len(), 4);

        // 8. Test max count (-m)
        let res = search(
            "l",
            Some(&dir_str),
            &["-m".to_string(), "2".to_string()],
            &[],
            &ignore_file,
        )
        .unwrap();
        // "l" matches: Hello World (2 l's), hello rust, case SENSITIVE line, wholeline (total 4 lines). With -m 2, it should limit to 2.
        assert_eq!(res.flat().len(), 2);

        // 9. Test max depth (-d)
        let nested_dir = temp_dir.join("nested").join("deep");
        std::fs::create_dir_all(&nested_dir).unwrap();
        let deep_file = nested_dir.join("deep.txt");
        let mut f = File::create(&deep_file).unwrap();
        writeln!(f, "deep matches").unwrap();
        drop(f);

        // With depth limit 1, we should NOT find "deep matches"
        let res = search(
            "deep matches",
            Some(&dir_str),
            &["-d".to_string(), "1".to_string()],
            &[],
            &ignore_file,
        )
        .unwrap();
        assert_eq!(res.flat().len(), 0);

        // With depth limit 3, we should find it
        let res = search(
            "deep matches",
            Some(&dir_str),
            &["-d".to_string(), "3".to_string()],
            &[],
            &ignore_file,
        )
        .unwrap();
        assert_eq!(res.flat().len(), 1);

        // 10. Test follow symlinks (-L)
        let ext_temp_dir = std::env::temp_dir().join("fcs_search_test_ext");
        let _ = std::fs::remove_dir_all(&ext_temp_dir);
        std::fs::create_dir_all(&ext_temp_dir).unwrap();
        let ext_file = ext_temp_dir.join("ext.txt");
        let mut f = File::create(&ext_file).unwrap();
        writeln!(f, "external file content").unwrap();
        drop(f);

        let symlink_path = temp_dir.join("symlink_dir");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&ext_temp_dir, &symlink_path).unwrap();

            // Without -L, should not follow link
            let res = search("external file content", Some(&dir_str), &[], &[], &ignore_file).unwrap();
            assert_eq!(res.flat().len(), 0);

            // With -L, should follow link
            let res = search(
                "external file content",
                Some(&dir_str),
                &["-L".to_string()],
                &[],
                &ignore_file,
            )
            .unwrap();
            assert_eq!(res.flat().len(), 1);
        }
        let _ = std::fs::remove_dir_all(&ext_temp_dir);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
