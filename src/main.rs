use clap::{Parser, Subcommand};
use skim::prelude::*;

use fcs::errors::AppError;
use fcs::ignore::IgnoreFile;
use fcs::search::{self, SearchResult};

#[derive(Parser, Debug)]
#[command(name = "fcs", author, version, about = "Fuzzy code search tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a .ignore file with default patterns
    Init {
        /// Target directory (default: current directory)
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Manage ignore patterns
    Ignore {
        #[command(subcommand)]
        action: IgnoreAction,

        /// Target directory (default: current directory)
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Preview a file at a specific line
    Preview {
        /// Format: "path:line" or "path:line:height"
        target: String,
    },

    /// Search patterns in files
    Search {
        /// Search pattern (regex)
        pattern: String,

        /// Target directory to search in
        directory: Option<String>,

        /// Ripgrep-compatible search options (e.g. -i/--ignore-case or --no-ignore)
        #[arg(short, long)]
        option: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum IgnoreAction {
    /// Add patterns to .ignore
    Add {
        /// Patterns to add
        patterns: Vec<String>,
    },
    /// Remove patterns from .ignore
    Remove {
        /// Patterns to remove
        patterns: Vec<String>,
    },
    /// List patterns in .ignore
    List,
}

fn get_ignore_file(directory: Option<&String>) -> String {
    directory
        .map(|d| format!("{d}/.ignore"))
        .unwrap_or_else(|| ".ignore".to_string())
}

fn parse_preview_arg(s: &str) -> Result<(String, usize, usize), AppError> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() < 2 {
        return Err(AppError::InvalidPreview(
            "Usage: fcs preview <path>:<line>[:height]".to_string(),
        ));
    }
    let path = parts[0].to_string();
    let line: usize = parts[1]
        .parse()
        .map_err(|e| AppError::InvalidPreview(format!("Invalid line number: {e}")))?;
    let height: usize = parts
        .get(2)
        .and_then(|h| h.parse().ok())
        .unwrap_or(24);
    Ok((path, line, height))
}

fn make_result(path: &str, line: usize, text: &str) -> SearchResult {
    SearchResult {
        path: path.to_string(),
        line_num: line,
        line_text: text.to_string(),
        display: format!("{path}:{line}:{text}"),
        match_ranges: Vec::new(),
    }
}

fn handle_search(
    pattern: &str,
    directory: Option<&String>,
    options: &[String],
) -> Result<(), AppError> {
    // Step 1: Search using regex + ignore crates
    let results = search::search(pattern, directory, options)?;
    let flat = results.flat();

    if flat.is_empty() {
        println!("No matches found");
        return Ok(());
    }

    loop {
        // Step 2: Interactive select using Skim
        let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
        let items: Vec<Arc<dyn SkimItem>> = flat
            .iter()
            .map(|result| std::sync::Arc::new(result.clone()) as std::sync::Arc<dyn SkimItem>)
            .collect();
        let _ = tx.send(items);
        drop(tx);

        let bind_opts = vec![
            "ctrl-u:half-page-up".to_string(),
            "ctrl-d:half-page-down".to_string(),
            "ctrl-r:kill-line".to_string(),
            "ctrl-v:toggle-preview".to_string(),
            "alt-u:preview-page-up".to_string(),
            "alt-d:preview-page-down".to_string(),
            "alt-j:preview-down".to_string(),
            "alt-k:preview-up".to_string(),
        ];

        let skim_options = SkimOptionsBuilder::default()
            .height("100%")
            .min_height("20")
            .multi(true)
            .delimiter(regex::Regex::new(":").unwrap())
            .color(
                "fg:-1,bg:-1,hl:33,fg+:254,bg+:235,hl+:33,info:136,prompt:136,pointer:230,marker:230,spinner:136",
            )
            .exact(true)
            .tac(true)
            .cycle(true)
            .bind(bind_opts)
            .preview("")
            .preview_window("right:59%")
            .build()
            .map_err(|e| AppError::Skim(e.to_string()))?;

        let output = Skim::run_with(skim_options, Some(rx)).ok();
        if output.is_none() {
            break;
        }
        let output = output.unwrap();
        if output.is_abort {
            break;
        }

        // Step 3: Open selected results in editor
        for item in output.selected_items.iter() {
            let display = item.output().to_string();
            if let Some(result) = flat.iter().find(|r| r.display == display) {
                search::open_file(&result.path, Some(result.line_num))?;
            }
        }
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), AppError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { directory } => {
            let ignore_file = IgnoreFile::new(&get_ignore_file(directory.as_ref()));
            ignore_file.init(true)?;
            println!("Initialized .ignore file");
        }
        Commands::Ignore { action, directory } => {
            let ignore_file = IgnoreFile::new(&get_ignore_file(directory.as_ref()));
            match action {
                IgnoreAction::Add { patterns } => {
                    if patterns.is_empty() {
                        return Err(AppError::General("No patterns specified to add".to_string()));
                    }
                    ignore_file.add(&patterns)?;
                    println!("Added patterns to .ignore");
                }
                IgnoreAction::Remove { patterns } => {
                    if patterns.is_empty() {
                        return Err(AppError::General("No patterns specified to remove".to_string()));
                    }
                    ignore_file.remove(&patterns)?;
                    println!("Removed patterns from .ignore");
                }
                IgnoreAction::List => {
                    let patterns = ignore_file.list()?;
                    if patterns.is_empty() {
                        println!("No ignore patterns");
                    } else {
                        for p in &patterns {
                            println!("{p}");
                        }
                    }
                }
            }
        }
        Commands::Preview { target } => {
            let (path, line, height) = parse_preview_arg(&target)?;
            let result = make_result(&path, line, "");
            fcs::preview::preview(&result, height)?;
        }
        Commands::Search {
            pattern,
            directory,
            option,
        } => {
            handle_search(&pattern, directory.as_ref(), &option)?;
        }
    }

    Ok(())
}
