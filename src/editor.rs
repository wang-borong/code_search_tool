use std::path::Path;
use std::process::Command;

use crate::core::Location;
use crate::errors::{AppError, Result};

#[derive(Debug, Clone)]
pub struct Editor {
    command: String,
}

impl Editor {
    pub fn from_config(command: Option<&str>) -> Self {
        let command = command
            .filter(|cmd| !cmd.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| std::env::var("VISUAL").ok())
            .or_else(|| std::env::var("EDITOR").ok())
            .unwrap_or_else(|| "nvim".to_string());

        Self { command }
    }

    pub fn open_location(&self, location: &Location) -> Result<()> {
        self.open_file(location.path(), location.line, location.column)
    }

    pub fn open_file(&self, path: &Path, line: Option<usize>, column: Option<usize>) -> Result<()> {
        if !path.exists() {
            return Err(AppError::FileNotFound(path.to_string_lossy().to_string()));
        }

        let mut parts = split_command(&self.command);
        if parts.is_empty() {
            return Err(AppError::General("Editor command is empty".to_string()));
        }

        let program = parts.remove(0);
        let mut command = Command::new(&program);
        command.args(parts);

        append_location_args(&mut command, &program, path, line, column);

        let status = command.status()?;
        if !status.success() {
            return Err(AppError::General(format!(
                "Editor exited with status {}",
                status
                    .code()
                    .map_or_else(|| "unknown".to_string(), |code| code.to_string())
            )));
        }

        Ok(())
    }
}

pub fn open_location(location: &Location, command: Option<&str>) -> Result<()> {
    Editor::from_config(command).open_location(location)
}

pub fn open_file(path: &Path, line: Option<usize>, column: Option<usize>, command: Option<&str>) -> Result<()> {
    Editor::from_config(command).open_file(path, line, column)
}

fn append_location_args(command: &mut Command, program: &str, path: &Path, line: Option<usize>, column: Option<usize>) {
    if is_vscode_like(program) {
        match (line, column) {
            (Some(line), Some(column)) => {
                command.arg("-g").arg(format!("{}:{line}:{column}", path.display()));
            }
            (Some(line), None) => {
                command.arg("-g").arg(format!("{}:{line}", path.display()));
            }
            _ => {
                command.arg(path);
            }
        }
        return;
    }

    if is_vim_like(program) {
        if let Some(line) = line {
            command.arg(format!("+{line}"));
        }
        command.arg(path);
        return;
    }

    command.arg(path);
}

fn is_vim_like(program: &str) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or(program);

    matches!(name, "vi" | "vim" | "nvim" | "view")
}

fn is_vscode_like(program: &str) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or(program);

    matches!(name, "code" | "code-insiders" | "codium" | "code-oss")
}

fn split_command(command: &str) -> Vec<String> {
    command.split_whitespace().map(ToOwned::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_command_keeps_basic_arguments() {
        assert_eq!(split_command("nvim -p"), vec!["nvim".to_string(), "-p".to_string()]);
    }

    #[test]
    fn detects_supported_editor_families() {
        assert!(is_vim_like("/usr/bin/nvim"));
        assert!(is_vscode_like("/usr/bin/code"));
        assert!(!is_vim_like("nano"));
    }
}
