use std::fs;
use std::path::Path;

use grep_regex::RegexMatcherBuilder;
use grep_searcher::{sinks::UTF8, SearcherBuilder};
use regex::Regex;

use crate::core::CodeItem;
use crate::errors::{AppError, Result};

const MAX_SYMBOL_FILE_BYTES: u64 = 2 * 1024 * 1024;

struct SymbolPattern {
    kind: &'static str,
    regex: Regex,
}

pub fn find_symbols(
    dir: Option<&String>,
    options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
) -> Result<Vec<CodeItem>> {
    let files = crate::files::find_files(dir, options, default_ignore, ignore_file)?;
    let patterns = build_symbol_patterns()?;
    let matcher = build_symbol_matcher()?;
    let mut symbols = Vec::new();
    let mut searcher = SearcherBuilder::new().build();

    for file in files {
        let path = file.location.path();
        if !is_supported_source_file(path) || is_too_large(path)? {
            continue;
        }

        extract_file_symbols(path, &file.label, &patterns, &matcher, &mut searcher, &mut symbols)?;
    }

    symbols.sort_by(|left, right| left.display_text().cmp(right.display_text()));
    Ok(symbols)
}

fn extract_file_symbols(
    path: &Path,
    display_path: &str,
    patterns: &[SymbolPattern],
    matcher: &grep_regex::RegexMatcher,
    searcher: &mut grep_searcher::Searcher,
    symbols: &mut Vec<CodeItem>,
) -> Result<()> {
    searcher
        .search_path(
            matcher,
            path,
            UTF8(|line_num, line| {
                let line = line.trim_end_matches(&['\r', '\n'][..]).to_string();
                if should_skip_line(&line) {
                    return Ok(true);
                }

                for pattern in patterns {
                    if let Some(captures) = pattern.regex.captures(&line) {
                        if let Some(name) = captures.get(1) {
                            symbols.push(CodeItem::symbol(
                                path.to_path_buf(),
                                display_path.to_string(),
                                line_num as usize,
                                Some(name.start() + 1),
                                name.as_str(),
                                pattern.kind,
                            ));
                            break;
                        }
                    }
                }

                Ok(true)
            }),
        )
        .map_err(|e| AppError::General(e.to_string()))?;

    Ok(())
}

fn build_symbol_patterns() -> Result<Vec<SymbolPattern>> {
    symbol_pattern_specs()
        .iter()
        .map(|(kind, pattern)| {
            Regex::new(pattern)
                .map(|regex| SymbolPattern { kind, regex })
                .map_err(AppError::Regex)
        })
        .collect()
}

fn build_symbol_matcher() -> Result<grep_regex::RegexMatcher> {
    let pattern = symbol_pattern_specs()
        .iter()
        .map(|(_, pattern)| format!("(?:{pattern})"))
        .collect::<Vec<String>>()
        .join("|");

    RegexMatcherBuilder::new()
        .build(&pattern)
        .map_err(|e| AppError::General(e.to_string()))
}

fn symbol_pattern_specs() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "function",
            r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\b",
        ),
        ("struct", r"^\s*(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
        ("enum", r"^\s*(?:pub\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
        ("trait", r"^\s*(?:pub\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
        ("impl", r"^\s*impl(?:\s*<[^>]+>)?\s+([A-Za-z_][A-Za-z0-9_:]*)\b"),
        ("macro", r"^\s*#\s*define\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
        (
            "class",
            r"^\s*(?:template\s*<[^>]+>\s*)?(?:class|struct)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
        ),
        ("enum", r"^\s*enum(?:\s+class)?\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
        (
            "function",
            r"^\s*(?:[A-Za-z_][A-Za-z0-9_:<>\*&\s]+\s+)+([A-Za-z_][A-Za-z0-9_:]*)\s*\([^;{}]*\)\s*(?:const\s*)?(?:noexcept\s*)?(?:override\s*)?(?:final\s*)?(?:\{|;)?\s*$",
        ),
        ("function", r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("),
        ("class", r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
        (
            "function",
            r"^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*\(",
        ),
        ("class", r"^\s*(?:export\s+)?class\s+([A-Za-z_$][A-Za-z0-9_$]*)\b"),
    ]
}

fn should_skip_line(line: &str) -> bool {
    let trimmed = line.trim_start();

    trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('#') && !trimmed.starts_with("#define")
}

fn is_supported_source_file(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or("");

    matches!(
        extension,
        "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" | "hxx" | "rs" | "py" | "js" | "jsx" | "ts" | "tsx"
    )
}

fn is_too_large(path: &Path) -> Result<bool> {
    let metadata = fs::metadata(path)?;
    Ok(metadata.len() > MAX_SYMBOL_FILE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_symbols_from_common_languages() {
        let temp_dir = std::env::temp_dir().join("fcs_symbols_test_dir");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        let rust_path = temp_dir.join("src").join("lib.rs");
        let mut rust_file = fs::File::create(&rust_path).unwrap();
        writeln!(rust_file, "pub struct Config {{}}").unwrap();
        writeln!(rust_file, "pub fn load_config() {{}}").unwrap();

        let c_path = temp_dir.join("src").join("net.c");
        let mut c_file = fs::File::create(&c_path).unwrap();
        writeln!(c_file, "#define MAX_PACKET_SIZE 1500").unwrap();
        writeln!(c_file, "int net_device_init(void) {{").unwrap();
        writeln!(c_file, "\treturn 0;").unwrap();
        writeln!(c_file, "}}").unwrap();

        let dir = temp_dir.to_string_lossy().to_string();
        let ignore_file = temp_dir.join("missing.ignore");
        let symbols = find_symbols(Some(&dir), &[], &[], &ignore_file).unwrap();
        let displays: Vec<&str> = symbols.iter().map(|symbol| symbol.display_text()).collect();

        assert!(displays.iter().any(|display| display.contains("Config [struct]")));
        assert!(displays
            .iter()
            .any(|display| display.contains("load_config [function]")));
        assert!(displays
            .iter()
            .any(|display| display.contains("MAX_PACKET_SIZE [macro]")));
        assert!(displays
            .iter()
            .any(|display| display.contains("net_device_init [function]")));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
