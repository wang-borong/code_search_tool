use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::errors::Result;

pub struct IgnoreFile {
    path: PathBuf,
}

impl IgnoreFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn ensure_parent(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    pub fn init(&self, defaults: bool) -> Result<()> {
        if self.path.exists() {
            return Ok(());
        }

        self.ensure_parent()?;
        let mut file = File::create(&self.path)?;
        if defaults {
            writeln!(file, "# Default ignore patterns")?;
            writeln!(file, "*.log")?;
            writeln!(file, "*.tmp")?;
            writeln!(file, "node_modules/")?;
            writeln!(file, ".git/")?;
        }
        Ok(())
    }

    pub fn add(&self, patterns: &[String]) -> Result<()> {
        let mut existing = self.read_lines()?;

        for pat in patterns {
            if !existing.iter().any(|l| l.trim() == pat) {
                existing.push(pat.clone());
            }
        }

        self.ensure_parent()?;
        let mut file = File::create(&self.path)?;
        for line in &existing {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    pub fn remove(&self, patterns: &[String]) -> Result<()> {
        let mut existing = self.read_lines()?;

        for pat in patterns {
            existing.retain(|l| l.trim() != pat);
        }

        if existing.is_empty() {
            if self.path.exists() {
                fs::remove_file(&self.path)?;
            }
        } else {
            self.ensure_parent()?;
            let mut file = File::create(&self.path)?;
            for line in &existing {
                writeln!(file, "{line}")?;
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<String>> {
        self.read_lines()
    }

    fn read_lines(&self) -> Result<Vec<String>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut lines = Vec::new();
        for line in reader.lines() {
            lines.push(line?);
        }
        Ok(lines)
    }
}
