use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use fcs::core::Location;
use fcs::errors::AppError;
use fcs::search::SearchResult;

pub(super) fn resolve_ignore_file(directory: Option<&String>) -> PathBuf {
    let target_dir = match directory {
        Some(directory) => Path::new(directory)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(directory)),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };

    let local_ignore = target_dir.join(".ignore");
    if local_ignore.exists() {
        return local_ignore;
    }

    let basename = target_dir.file_name().and_then(|name| name.to_str()).unwrap_or("root");
    let mut hasher = DefaultHasher::new();
    target_dir.hash(&mut hasher);
    let hash = format!("{:08x}", hasher.finish() as u32);

    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("fcs")
        .join(format!("{basename}-{hash}.ignore"))
}

pub(super) fn parse_preview_arg(value: &str) -> Result<(String, usize, usize), AppError> {
    let parts: Vec<&str> = value.splitn(3, ':').collect();
    if parts.len() < 2 {
        return Err(AppError::InvalidPreview(
            "Usage: fcs preview <path>:<line>[:height]".to_string(),
        ));
    }

    let path = parts[0].to_string();
    let line = parts[1]
        .parse::<usize>()
        .map_err(|err| AppError::InvalidPreview(format!("Invalid line number: {err}")))?;
    let height = parts.get(2).and_then(|height| height.parse().ok()).unwrap_or(24);

    Ok((path, line, height))
}

pub(super) fn parse_location_arg(value: &str) -> Result<Location, AppError> {
    let parts: Vec<&str> = value.rsplitn(3, ':').collect();
    match parts.as_slice() {
        [line, path] => {
            let line = line
                .parse::<usize>()
                .map_err(|err| AppError::InvalidPreview(format!("Invalid line number: {err}")))?;
            Ok(Location::new(*path, Some(line), None))
        }
        [column, line, path] => {
            let line = line
                .parse::<usize>()
                .map_err(|err| AppError::InvalidPreview(format!("Invalid line number: {err}")))?;
            let column = column
                .parse::<usize>()
                .map_err(|err| AppError::InvalidPreview(format!("Invalid column number: {err}")))?;
            Ok(Location::new(*path, Some(line), Some(column)))
        }
        _ => Err(AppError::InvalidPreview("Usage: <path>:<line>[:column]".to_string())),
    }
}

pub(super) fn parse_file_arg(value: &str) -> PathBuf {
    if let Ok(location) = parse_location_arg(value) {
        return location.path;
    }

    PathBuf::from(value)
}

pub(super) fn make_result(path: &str, line: usize, text: &str) -> SearchResult {
    SearchResult {
        path: path.to_string(),
        line_num: line,
        line_text: text.to_string(),
        display: format!("{path}:{line}:{text}"),
        match_ranges: Vec::new(),
    }
}

pub(super) fn resolve_location_for_root(location: Location, root: &Path) -> Location {
    Location::new(
        resolve_path_for_root(location.path, root),
        location.line,
        location.column,
    )
}

pub(super) fn resolve_path_for_root(path: PathBuf, root: &Path) -> PathBuf {
    if path.is_absolute() || path.exists() {
        path
    } else {
        root.join(path)
    }
}
