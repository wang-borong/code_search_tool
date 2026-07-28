use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

use globset::{GlobBuilder, GlobMatcher};
use ignore::{WalkBuilder, WalkState};
use regex::bytes::{Regex, RegexBuilder};
use regex_syntax::{
    hir::{Class, Hir, HirKind, Literal},
    ParserBuilder,
};

use crate::core::CodeItem;
use crate::errors::{AppError, Result};

#[derive(Debug, Clone, Default)]
struct FileSearchOptions {
    hidden: bool,
    follow_links: bool,
    no_ignore: bool,
    max_depth: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePatternSyntax {
    Regex,
    Glob,
    FixedStrings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePatternTarget {
    FileName,
    RelativePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePatternCase {
    Sensitive,
    Insensitive,
    Smart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePattern {
    pub pattern: String,
    pub syntax: FilePatternSyntax,
    pub target: FilePatternTarget,
    pub case: FilePatternCase,
}

impl FilePattern {
    pub fn new(
        pattern: impl Into<String>,
        syntax: FilePatternSyntax,
        target: FilePatternTarget,
        case: FilePatternCase,
    ) -> Self {
        Self {
            pattern: pattern.into(),
            syntax,
            target,
            case,
        }
    }
}

struct CompiledFilePattern {
    target: FilePatternTarget,
    matcher: FilePatternMatcher,
}

enum FilePatternMatcher {
    Regex(Regex),
    Glob(GlobMatcher),
}

impl CompiledFilePattern {
    fn compile(pattern: &FilePattern) -> Result<Self> {
        let case_insensitive = match pattern.case {
            FilePatternCase::Sensitive => false,
            FilePatternCase::Insensitive => true,
            FilePatternCase::Smart => !pattern_has_uppercase_char(pattern),
        };

        let matcher = match pattern.syntax {
            FilePatternSyntax::Regex | FilePatternSyntax::FixedStrings => {
                let regex_pattern = if pattern.syntax == FilePatternSyntax::FixedStrings {
                    regex::escape(&pattern.pattern)
                } else {
                    pattern.pattern.clone()
                };
                let regex = RegexBuilder::new(&regex_pattern)
                    .case_insensitive(case_insensitive)
                    .build()
                    .map_err(|err| {
                        AppError::General(format!("Invalid file regex pattern `{}`: {err}", pattern.pattern))
                    })?;
                FilePatternMatcher::Regex(regex)
            }
            FilePatternSyntax::Glob => {
                let mut builder = GlobBuilder::new(&pattern.pattern);
                builder.literal_separator(true).case_insensitive(case_insensitive);
                let glob = builder.build().map_err(|err| {
                    AppError::General(format!("Invalid file glob pattern `{}`: {err}", pattern.pattern))
                })?;
                FilePatternMatcher::Glob(glob.compile_matcher())
            }
        };

        Ok(Self {
            target: pattern.target,
            matcher,
        })
    }

    fn matches_os_str(&self, candidate: &OsStr) -> bool {
        match &self.matcher {
            FilePatternMatcher::Regex(regex) => regex.is_match(&os_str_to_bytes(candidate)),
            FilePatternMatcher::Glob(glob) => glob.is_match(Path::new(candidate)),
        }
    }

    fn matches_relative_path(&self, candidate: &str) -> bool {
        match &self.matcher {
            FilePatternMatcher::Regex(regex) => regex.is_match(candidate.as_bytes()),
            FilePatternMatcher::Glob(glob) => glob.is_match(Path::new(candidate)),
        }
    }
}

fn pattern_has_uppercase_char(pattern: &FilePattern) -> bool {
    if pattern.syntax != FilePatternSyntax::Regex {
        return pattern.pattern.chars().any(char::is_uppercase);
    }

    let mut parser = ParserBuilder::new().utf8(false).build();
    parser
        .parse(&pattern.pattern)
        .map(|hir| hir_has_uppercase_char(&hir))
        .unwrap_or_else(|_| pattern.pattern.chars().any(char::is_uppercase))
}

fn hir_has_uppercase_char(hir: &Hir) -> bool {
    match hir.kind() {
        HirKind::Literal(Literal(bytes)) => match std::str::from_utf8(bytes) {
            Ok(value) => value.chars().any(char::is_uppercase),
            Err(_) => bytes.iter().any(|byte| char::from(*byte).is_uppercase()),
        },
        HirKind::Class(Class::Unicode(ranges)) => ranges
            .iter()
            .any(|range| range.start().is_uppercase() || range.end().is_uppercase()),
        HirKind::Class(Class::Bytes(ranges)) => ranges
            .iter()
            .any(|range| char::from(range.start()).is_uppercase() || char::from(range.end()).is_uppercase()),
        HirKind::Capture(capture) => hir_has_uppercase_char(&capture.sub),
        HirKind::Repetition(repetition) => hir_has_uppercase_char(&repetition.sub),
        HirKind::Concat(hirs) | HirKind::Alternation(hirs) => hirs.iter().any(hir_has_uppercase_char),
        _ => false,
    }
}

pub fn find_files(
    dir: Option<&String>,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<Vec<CodeItem>> {
    find_files_with_pattern(dir, options, default_ignore, ignore_file, None)
}

pub fn find_files_with_pattern(
    dir: Option<&String>,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
    pattern: Option<&FilePattern>,
) -> Result<Vec<CodeItem>> {
    let options = parse_file_options(options);
    let matcher = pattern.map(CompiledFilePattern::compile).transpose()?.map(Arc::new);
    let root = dir
        .map(|d| Path::new(d).to_path_buf())
        .unwrap_or_else(|| Path::new(".").to_path_buf());

    let mut builder = WalkBuilder::new(&root);
    builder
        .hidden(!options.hidden)
        .standard_filters(true)
        .ignore(true)
        .git_ignore(true)
        .git_global(true)
        .follow_links(options.follow_links);

    if let Some(depth) = options.max_depth {
        builder.max_depth(Some(depth));
    }

    if options.no_ignore {
        builder.ignore(false);
        builder.git_ignore(false);
        builder.git_global(false);
        builder.parents(false);
    } else {
        add_ignore_file(&mut builder, ignore_file)?;
        add_default_ignore(&mut builder, &root, default_ignore)?;
    }

    let root = Arc::new(root);
    let (sender, receiver) = mpsc::channel();
    builder.build_parallel().run(|| {
        let sender = sender.clone();
        let root = Arc::clone(&root);
        let matcher = matcher.clone();

        Box::new(move |entry| {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => return WalkState::Continue,
            };

            if !entry.file_type().is_some_and(|file_type| file_type.is_file()) {
                return WalkState::Continue;
            }

            let display_path = match matcher.as_deref() {
                Some(matcher) if matcher.target == FilePatternTarget::FileName => {
                    let Some(file_name) = entry.path().file_name() else {
                        return WalkState::Continue;
                    };
                    if !matcher.matches_os_str(file_name) {
                        return WalkState::Continue;
                    }
                    normalized_relative_path(entry.path(), root.as_path())
                }
                Some(matcher) => {
                    let display_path = normalized_relative_path(entry.path(), root.as_path());
                    if !matcher.matches_relative_path(&display_path) {
                        return WalkState::Continue;
                    }
                    display_path
                }
                None => normalized_relative_path(entry.path(), root.as_path()),
            };
            let item = CodeItem::file_with_display(entry.path().to_path_buf(), display_path);
            if sender.send(item).is_err() {
                return WalkState::Quit;
            }

            WalkState::Continue
        })
    });
    drop(sender);

    let mut items = receiver.into_iter().collect::<Vec<CodeItem>>();

    items.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(items)
}

fn parse_file_options(options: &[String]) -> FileSearchOptions {
    let mut parsed = FileSearchOptions::default();
    let mut i = 0;

    while i < options.len() {
        let option = &options[i];
        match option.as_str() {
            "--hidden" | "hidden" => {
                parsed.hidden = true;
            }
            "--follow" | "-L" | "follow" => {
                parsed.follow_links = true;
            }
            "--no-ignore" | "no-ignore" => {
                parsed.no_ignore = true;
            }
            "--max-depth" | "-d" | "max-depth" => {
                if i + 1 < options.len() {
                    i += 1;
                    parsed.max_depth = options[i].parse::<usize>().ok();
                }
            }
            _ => {
                if let Some(value) = option.strip_prefix("--max-depth=") {
                    parsed.max_depth = value.parse::<usize>().ok();
                } else if let Some(value) = option.strip_prefix("-d") {
                    if !value.is_empty() {
                        parsed.max_depth = value.parse::<usize>().ok();
                    }
                }
            }
        }
        i += 1;
    }

    parsed
}

fn add_ignore_file(builder: &mut WalkBuilder, ignore_file: &Path) -> Result<()> {
    if !ignore_file.exists() {
        return Ok(());
    }

    if let Some(err) = builder.add_ignore(ignore_file) {
        return Err(AppError::General(format!("Failed to add ignore file: {err}")));
    }

    Ok(())
}

fn add_default_ignore(builder: &mut WalkBuilder, root: &Path, default_ignore: &[String]) -> Result<()> {
    if default_ignore.is_empty() {
        return Ok(());
    }

    let mut overrides = ignore::overrides::OverrideBuilder::new(root);
    for pattern in default_ignore {
        let pattern = if pattern.starts_with('!') {
            pattern.clone()
        } else {
            format!("!{pattern}")
        };
        overrides.add(&pattern).map_err(|e| AppError::General(e.to_string()))?;
    }

    builder.overrides(overrides.build().map_err(|e| AppError::General(e.to_string()))?);
    Ok(())
}

fn path_to_relative(path: &Path, root: &Path) -> PathBuf {
    path.strip_prefix(root)
        .or_else(|_| path.strip_prefix("."))
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

fn normalized_relative_path(path: &Path, root: &Path) -> String {
    path_to_relative(path, root).to_string_lossy().replace('\\', "/")
}

#[cfg(unix)]
fn os_str_to_bytes(input: &OsStr) -> Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;

    Cow::Borrowed(input.as_bytes())
}

#[cfg(not(unix))]
fn os_str_to_bytes(input: &OsStr) -> Cow<'_, [u8]> {
    match input.to_string_lossy() {
        Cow::Owned(value) => Cow::Owned(value.into_bytes()),
        Cow::Borrowed(value) => Cow::Borrowed(value.as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_DIR_ID: AtomicUsize = AtomicUsize::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let id = TEST_DIR_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), id));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn create_file(&self, relative: &str) {
            let path = self.path.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            File::create(path).unwrap();
        }

        fn argument(&self) -> String {
            self.path.to_string_lossy().to_string()
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parses_common_file_options() {
        let options = parse_file_options(&[
            "--hidden".to_string(),
            "-L".to_string(),
            "-d".to_string(),
            "3".to_string(),
            "--no-ignore".to_string(),
        ]);

        assert!(options.hidden);
        assert!(options.follow_links);
        assert!(options.no_ignore);
        assert_eq!(options.max_depth, Some(3));
    }

    #[test]
    fn finds_files_with_relative_display_and_real_location() {
        let temp_dir = TestDir::new("fcs_files_test_dir");
        temp_dir.create_file("src/main.rs");

        let dir = temp_dir.argument();
        let ignore_file = temp_dir.path.join("missing.ignore");
        let items = find_files(Some(&dir), &[], &[], &ignore_file).unwrap();

        let item = items
            .iter()
            .find(|item| item.display_text() == "src/main.rs")
            .expect("expected src/main.rs to be listed");
        let expected_path = temp_dir.path.join("src").join("main.rs");
        assert_eq!(item.location.path(), expected_path.as_path());
    }

    #[test]
    fn filters_files_by_basename_regex_before_returning_items() {
        let temp_dir = TestDir::new("fcs_files_regex_test_dir");
        temp_dir.create_file("src/main.rs");
        temp_dir.create_file("src/main.toml");
        temp_dir.create_file("tests/main_test.rs");
        temp_dir.create_file("README.md");

        let dir = temp_dir.argument();
        let ignore_file = temp_dir.path.join("missing.ignore");
        let pattern = FilePattern::new(
            r"^main\.(rs|toml)$",
            FilePatternSyntax::Regex,
            FilePatternTarget::FileName,
            FilePatternCase::Sensitive,
        );
        let items = find_files_with_pattern(Some(&dir), &[], &[], &ignore_file, Some(&pattern)).unwrap();
        let labels = items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>();

        assert_eq!(labels, vec!["src/main.rs", "src/main.toml"]);
    }

    #[test]
    fn filters_files_by_normalized_relative_path() {
        let temp_dir = TestDir::new("fcs_files_path_regex_test_dir");
        temp_dir.create_file("src/main.rs");
        temp_dir.create_file("tests/unit/main_test.rs");

        let dir = temp_dir.argument();
        let ignore_file = temp_dir.path.join("missing.ignore");
        let pattern = FilePattern::new(
            r"^tests/.+_test\.rs$",
            FilePatternSyntax::Regex,
            FilePatternTarget::RelativePath,
            FilePatternCase::Sensitive,
        );
        let items = find_files_with_pattern(Some(&dir), &[], &[], &ignore_file, Some(&pattern)).unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "tests/unit/main_test.rs");
    }

    #[test]
    fn supports_glob_and_fixed_string_patterns() {
        let temp_dir = TestDir::new("fcs_files_pattern_modes_test_dir");
        temp_dir.create_file("src/main.rs");
        temp_dir.create_file("src/main.toml");
        temp_dir.create_file("notes[main].txt");

        let dir = temp_dir.argument();
        let ignore_file = temp_dir.path.join("missing.ignore");
        let glob = FilePattern::new(
            "*.rs",
            FilePatternSyntax::Glob,
            FilePatternTarget::FileName,
            FilePatternCase::Sensitive,
        );
        let glob_items = find_files_with_pattern(Some(&dir), &[], &[], &ignore_file, Some(&glob)).unwrap();
        assert_eq!(glob_items.len(), 1);
        assert_eq!(glob_items[0].label, "src/main.rs");

        let fixed = FilePattern::new(
            "[main]",
            FilePatternSyntax::FixedStrings,
            FilePatternTarget::FileName,
            FilePatternCase::Sensitive,
        );
        let fixed_items = find_files_with_pattern(Some(&dir), &[], &[], &ignore_file, Some(&fixed)).unwrap();
        assert_eq!(fixed_items.len(), 1);
        assert_eq!(fixed_items[0].label, "notes[main].txt");
    }

    #[test]
    fn supports_insensitive_and_smart_case_matching() {
        let insensitive = CompiledFilePattern::compile(&FilePattern::new(
            r"main\.rs$",
            FilePatternSyntax::Regex,
            FilePatternTarget::FileName,
            FilePatternCase::Insensitive,
        ))
        .unwrap();
        assert!(insensitive.matches_os_str(OsStr::new("MAIN.RS")));

        let smart_lower = CompiledFilePattern::compile(&FilePattern::new(
            r"main\.rs$",
            FilePatternSyntax::Regex,
            FilePatternTarget::FileName,
            FilePatternCase::Smart,
        ))
        .unwrap();
        assert!(smart_lower.matches_os_str(OsStr::new("MAIN.RS")));

        let smart_with_anchor = CompiledFilePattern::compile(&FilePattern::new(
            r"\Amain\.rs$",
            FilePatternSyntax::Regex,
            FilePatternTarget::FileName,
            FilePatternCase::Smart,
        ))
        .unwrap();
        assert!(smart_with_anchor.matches_os_str(OsStr::new("MAIN.RS")));

        let smart_upper = CompiledFilePattern::compile(&FilePattern::new(
            r"Main\.rs$",
            FilePatternSyntax::Regex,
            FilePatternTarget::FileName,
            FilePatternCase::Smart,
        ))
        .unwrap();
        assert!(!smart_upper.matches_os_str(OsStr::new("main.rs")));
    }

    #[test]
    fn reports_invalid_file_patterns_before_scanning() {
        let temp_dir = TestDir::new("fcs_files_invalid_pattern_test_dir");
        let dir = temp_dir.argument();
        let ignore_file = temp_dir.path.join("missing.ignore");
        let pattern = FilePattern::new(
            "(",
            FilePatternSyntax::Regex,
            FilePatternTarget::FileName,
            FilePatternCase::Sensitive,
        );

        let err = find_files_with_pattern(Some(&dir), &[], &[], &ignore_file, Some(&pattern)).unwrap_err();
        assert!(err.to_string().contains("Invalid file regex pattern `(`"));
    }
}
