use std::env;
use code_search::check_if_commands_exist;
use code_search::searcher::search;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("fs [rg options] <search pattern> [search path]");
        return;
    }

    // check the commands
    let apps = ["rg", "fzf", "bat", "nvim"];
    if !check_if_commands_exist(&apps) {
        eprintln!("please install it!");
        return;
    }

    search(&args);
}
