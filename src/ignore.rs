use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use crate::errors::Result;

pub struct IgnoreFile {
    path: String,
}

impl IgnoreFile {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
        }
    }

    fn ensure_parent(&self) -> Result<()> {
        if let Some(parent) = Path::new(&self.path).parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    pub fn init(&self, defaults: bool) -> Result<()> {
        if Path::new(&self.path).exists() {
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
            if Path::new(&self.path).exists() {
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
        let path = Path::new(&self.path);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut lines = Vec::new();
        for line in reader.lines() {
            lines.push(line?);
        }
        Ok(lines)
    }
}
