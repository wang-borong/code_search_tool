use clap::Parser;
use code_search::check_if_commands_exist;
use code_search::searcher::search;
use code_search::previewer::preview;
use code_search::ignore::{add_ignore, remove_ignore, list_ignore};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    search: Vec<String>,
    #[arg(short, long, value_name = "RIPGREP OPTION WITHOUT '--' OR '-'")]
    option: Vec<String>,
    #[arg(short, long)]
    add_ignore: Vec<String>,
    #[arg(short, long)]
    list_ignore: bool,
    #[arg(short, long)]
    remove_ignore: Vec<String>,
    #[arg(short, long)]
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

fn main() {
    // check the dependent commands
    let apps = ["rg", "fzf", "bat", "nvim"];
    if !check_if_commands_exist(&apps) {
       eprintln!("please install it!");
       return;
    }

    let args = Args::parse();

    if args.add_ignore.len() > 0 {
        let ignore_file = get_ignore_file(&args);
        let pats = args.add_ignore.as_ref();
        add_ignore(&ignore_file, &pats);
        return;
    }

    if args.remove_ignore.len() > 0 {
        let ignore_file = get_ignore_file(&args);
        let pats = args.remove_ignore.as_ref();
        remove_ignore(&ignore_file, pats);
        return;
    }

    if args.list_ignore {
        let ignore_file = get_ignore_file(&args);
        list_ignore(&ignore_file);
        return;
    }

    if args.preview.len() > 0 {
        let pargs = args.preview.as_ref();
        preview(pargs);
        return;
    }

    if args.search.len() > 0 {
        let ss = args.search.as_ref();
        search(ss);
        // search(opts, ss, dir)
        return;
    }
}
