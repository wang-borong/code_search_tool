
use clap::Parser;
use fcs::check_if_commands_exist;
// use fcs::searcher::search;
use fcs::previewer::preview;
use fcs::ignore::{create_ignore, add_ignore, remove_ignore, list_ignore};

/// A program that prints its own source code using the bat library
use bat::{PagingMode, PrettyPrinter, WrappingMode};
use skim::prelude::*;

use std::{env, error::Error, ffi::OsString, io::IsTerminal, process};

use {
    grep::{
        cli,
        printer::{ColorSpecs, StandardBuilder},
        regex::RegexMatcher,
        searcher::{BinaryDetection, SearcherBuilder},
    },
    termcolor::ColorChoice,
    walkdir::WalkDir,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    init: bool,
    #[arg(short, long, value_name = "RIPGREP OPTION")]
    option: Vec<String>,
    #[arg(short, long)]
    add_ignore: Vec<String>,
    #[arg(short, long)]
    list_ignore: bool,
    #[arg(short, long)]
    remove_ignore: Vec<String>,
    #[arg(short, long, value_name = "INTER PREVIEW")]
    preview: Vec<String>,
    search: Option<String>,
    directory: Option<String>,
}

fn get_ignore_file(args: &Args) -> String {
    let mut ignore_file = String::new();
    if let Some(dir) = args.directory.as_ref() {
        ignore_file.push_str(&format!("{}/.ignore", dir));
    } else {
        ignore_file.push_str(".ignore");
    }

    ignore_file
}

fn get_opts(args: &Args) -> Vec<String> {
    let mut opts = Vec::new();
    let opts_esc = &args.option;
    for opt in opts_esc {
        if opt.starts_with('\\') {
            opts.push(opt.strip_prefix('\\').unwrap().to_string());
        } else {
            opts.push(opt.to_string());
        }
    }
    opts
}

fn main1() {
    // check the dependent commands
    let apps = ["rg", "fzf", "bat", "nvim"];
    if !check_if_commands_exist(&apps) {
       eprintln!("please install it!");
       return;
    }

    let args = Args::parse();

    let ignore_file = get_ignore_file(&args);
    if args.init {
        create_ignore(&ignore_file, args.init);
        return;
    }

    if args.add_ignore.len() > 0 {
        let pats = args.add_ignore.as_ref();
        add_ignore(&ignore_file, &pats);
        return;
    }

    if args.remove_ignore.len() > 0 {
        let pats = args.remove_ignore.as_ref();
        remove_ignore(&ignore_file, &pats);
        return;
    }

    if args.list_ignore {
        list_ignore(&ignore_file);
        return;
    }

    if args.preview.len() > 0 {
        let pargs = args.preview.as_ref();
        preview(pargs);
        return;
    }

    let ss = match args.search.as_ref() {
        Some(ss) => ss,
        None => {
            eprintln!("No search string specified for searching!");
            return;
        }
    };
    let dir = args.directory.as_ref();
    let opts = get_opts(&args);

    // search(&opts, &ss, dir);
}

fn main_bat() {
    PrettyPrinter::new()
        .header(true)
        .grid(true)
        .line_numbers(true)
        .use_italics(true)
        // The following line will be highlighted in the output:
        .highlight(line!() as usize)
        .theme("1337")
        .wrapping_mode(WrappingMode::Character)
        .paging_mode(PagingMode::QuitIfOneScreen)
        .input_file(file!())
        .print()
        .unwrap();
}

pub fn main_skim() {
    let options = SkimOptions::default();

    let selected_items = Skim::run_with(&options, None)
        .map(|out| out.selected_items)
        .unwrap_or_else(Vec::new);

    for item in selected_items.iter() {
        println!("{}", item.output());
    }
}

fn main() {
    if let Err(err) = try_main() {
        eprintln!("{}", err);
        process::exit(1);
    }
}

fn try_main() -> Result<(), Box<dyn Error>> {
    let mut args: Vec<OsString> = env::args_os().collect();
    if args.len() < 2 {
        return Err("Usage: simplegrep <pattern> [<path> ...]".into());
    }
    if args.len() == 2 {
        args.push(OsString::from("./"));
    }
    search(cli::pattern_from_os(&args[1])?, &args[2..])
}

fn search(pattern: &str, paths: &[OsString]) -> Result<(), Box<dyn Error>> {
    let matcher = RegexMatcher::new_line_matcher(&pattern)?;
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(false)
        .build();
    let mut printer = StandardBuilder::new()
        .color_specs(ColorSpecs::default_with_color())
        .build(cli::stdout(if std::io::stdout().is_terminal() {
            ColorChoice::Auto
        } else {
            ColorChoice::Never
        }));

    for path in paths {
        for result in WalkDir::new(path) {
            let dent = match result {
                Ok(dent) => dent,
                Err(err) => {
                    eprintln!("{}", err);
                    continue;
                }
            };
            if !dent.file_type().is_file() {
                continue;
            }
            let result = searcher.search_path(
                &matcher,
                dent.path(),
                printer.sink_with_path(&matcher, dent.path()),
            );
            if let Err(err) = result {
                eprintln!("{}: {}", dent.path().display(), err);
            }
        }
    }
    Ok(())
}
