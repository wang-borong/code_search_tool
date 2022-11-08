pub mod searcher;
pub mod previewer;
pub mod ignore;

use std::env;
use std::fs;

// only works on unix*
fn check_if_command_exists(cmd: &str) -> bool {
    if let Ok(path) = env::var("PATH") {
        for p in path.split(":") {
            let p_str = format!("{}/{}", p, cmd);
            if fs::metadata(p_str).is_ok() {
                return true;
            }
        }
    }
    false
}

pub fn check_if_commands_exist(cmds: &[&str]) -> bool {
    for cmd in cmds {
        if !check_if_command_exists(cmd) {
            eprint!("\"{}\" is not installed in your PATH, ", cmd);
            return false;
        }
    }

    true
}

