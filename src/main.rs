use clap::Parser;
use code_search::check_if_commands_exist;
use code_search::searcher::search;
use code_search::previewer::preview;
use code_search::ignore::{create_ignore, add_ignore, remove_ignore, list_ignore};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    init: bool,
    #[arg(short, long, value_name = "SEARCH STRING")]
    search: Vec<String>,
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

fn main() {
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

    let ss = args.search.as_ref();
    let dir = args.directory.as_ref();
    let opts = get_opts(&args);

    search(&opts, ss, dir);
}
