use clap::Parser;
use code_search::check_if_commands_exist;
use code_search::searcher::search;
use code_search::previewer::preview;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    search: Vec<String>,
    #[arg(short, long, value_name = "RIPGREP OPTION WITHOUT '--' OR '-'")]
    option: Vec<String>,
    #[arg(short, long)]
    add_ignore: Option<String>,
    #[arg(short, long)]
    list_ignore: bool,
    #[arg(short, long)]
    remove_ignore: Option<String>,
    #[arg(short, long)]
    preview: Vec<String>,
    directory: Option<String>,
}

fn main() {
    // check the dependent commands
    let apps = ["rg", "fzf", "bat", "nvim"];
    if !check_if_commands_exist(&apps) {
       eprintln!("please install it!");
       return;
    }

    let args = Args::parse();

    if let Some(ai) = args.add_ignore.as_ref() {
        println!("ai: {}", ai);
        return;
    }

    if let Some(ri) = args.remove_ignore.as_ref() {
        println!("ri: {}", ri);
        return;
    }

    if args.list_ignore {
        println!("list ignores");
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
        return;
    }
}
