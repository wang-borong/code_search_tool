use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::core::CodeItem;
use crate::errors::{AppError, Result};

#[derive(Debug, Clone, Default)]
struct FileSearchOptions {
    hidden: bool,
    follow_links: bool,
    no_ignore: bool,
    max_depth: Option<usize>,
}

pub fn find_files(
    dir: Option<&String>,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<Vec<CodeItem>> {
    let options = parse_file_options(options);
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

    let mut items = Vec::new();
    for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        if !entry.file_type().is_some_and(|file_type| file_type.is_file()) {
            continue;
        }

        let display_path = path_to_relative(entry.path(), &root)
            .to_string_lossy()
            .replace('\\', "/");
        items.push(CodeItem::file_with_display(entry.path().to_path_buf(), display_path));
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

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
        let temp_dir = std::env::temp_dir().join("fcs_files_test_dir");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();
        File::create(temp_dir.join("src").join("main.rs")).unwrap();

        let dir = temp_dir.to_string_lossy().to_string();
        let ignore_file = temp_dir.join("missing.ignore");
        let items = find_files(Some(&dir), &[], &[], &ignore_file).unwrap();

        let item = items
            .iter()
            .find(|item| item.display_text() == "src/main.rs")
            .expect("expected src/main.rs to be listed");
        let expected_path = temp_dir.join("src").join("main.rs");
        assert_eq!(item.location.path(), expected_path.as_path());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
