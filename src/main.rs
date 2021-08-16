use std::io::prelude::*;
use term_size;
use std::env;
use std::io::BufReader;
use std::process::{Command, Stdio};

fn previewer(args: &[String]) {
    if args.len() < 1 {
        eprintln!("Usage: fzf-previewer <rgout> <termnal hight>");
        return;
    }

    println!("args: {:?}", args);

    let rgout = &args[0];
    let termh = args[1].parse::<i32>().unwrap();
    let rgarr: Vec<&str> = rgout.splitn(3, ":").collect();
    let filname = rgarr[0];
    let linum = rgarr[1].parse::<i32>().unwrap();
    let rem_termh = termh * 3 / 4;
    let startline;
    let stopline;
    if linum > rem_termh {
        startline = linum - rem_termh;
    } else {
        startline = 0;
    }
    stopline = startline + termh * 3;

    let view_cmd = format!("bat -n --color=always -H {} -r {}:{} {}",
                       linum, startline, stopline, filname);

    Command::new("bash")
        .arg("-c")
        .arg(&view_cmd)
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success();
}

fn main() {
    let app_path = String::from(env::current_exe().unwrap()
                            .to_str().unwrap());
    let args: Vec<String> = env::args().collect();

    // We will implement the previewer command for fzf at here
    // we check the second argument, if it is "--PREVIEWER" then
    // it is used to preview the selected file in fzf.
    if args[1] == "--PREVIEWER".to_owned() {
        previewer(&args[2..]);
        return;
    }

    let rg_proc = match Command::new("rg")
                                // Set some default options for rg command
                                .args(&[
                                      "-n",
                                      "--with-filename",
                                      "--color=always",
                                ])
                                // Input args
                                .args(&args[1..])
                                .stdout(Stdio::piped())
                                .spawn() {
        Err(why) => panic!("couldn't spawn shell: {}", why),
        Ok(rg_proc) => rg_proc,
    };

    // Use a BufReader so that the fzf command will not blocked
    let mut rg_reader = BufReader::new(rg_proc.stdout.unwrap());
    // I can not figure out other methods to store rg output to
    // feed to fzf multi-times. It just works now.
    let mut rg_output_str = String::new();

    let (_, term_hight) = term_size::dimensions().unwrap();
    let mut fzf_query = String::new();

    loop {
        let mut fzf_query_opt = String::new();
        // feed the last query string in current fzf selecting.
        if !fzf_query.is_empty() {
            fzf_query_opt = format!(r#"-q "{}""#, fzf_query);
        }
        // fzf is start with bash now, oherwise it can not spawn
        // the preview command.
        // the options used here can be read from the fzf man.
        let fzf_cmd = &format!(r#"fzf --ansi -e --tac -0 --cycle -m \
                        --min-height=20 -d ':' --print-query \
                        --preview-window=right:59% \
                        --color fg:-1,bg:-1,hl:33,fg+:254,bg+:235,hl+:33 \
                        --color info:136,prompt:136,pointer:230 \
                        --color marker:230,spinner:136 \
                        --bind "ctrl-u:half-page-up" \
                        --bind "ctrl-d:half-page-down" \
                        --bind "alt-u:preview-page-up" \
                        --bind "alt-d:preview-page-down" \
                        --bind "alt-j:preview-down" \
                        --bind "alt-k:preview-up" \
                        --bind "ctrl-v:toggle-preview" \
                        --bind "ctrl-r:kill-line" \
                        --preview="echo '\033[1;32m {{1}}\033[0m'; \
                        {} --PREVIEWER "{{}}" "{}"" {} \
                        "#, app_path, term_hight, fzf_query_opt);
        let fzf_proc = match Command::new("bash")
            .arg("-c")
            .arg(fzf_cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn() {
                Err(why) => panic!("couldn't spawn fzf: {}", why),
                Ok(fzf_proc) => fzf_proc,
            };

        if rg_output_str.is_empty() {
            for line in rg_reader.by_ref().lines()
                            .by_ref().into_iter() {
                match line {
                    Ok(mut line) => {
                        // .lines() method of BufReader will strim the last '\n'.
                        // push it back, because fzf need it.
                        line.push('\n');
                        match fzf_proc.stdin.as_ref().unwrap()
                            .write_all(line.as_bytes()) {
                                // TODO:
                                // If someone broke the fzf process by selecting
                                // one pattern (or other operations) before rg
                                // searching is fininshed, the pipe will be
                                // broken. Perhaps, we can assume one user has
                                // got his/her result after stopping the process.

                                // Well, leave it panics now and figure out a
                                // better solution later.
                                Err(why) => panic!(
                                    "couldn't write to fzf stdin: {}", why),
                                Ok(_) => {},
                            }
                        rg_output_str.push_str(&line);
                    },
                    Err(why) => panic!("get wrong line: {}", why),
                }
            }
        } else {
            // After rg_output_str is filled at first time, we can use it now.
            // We dont need re-search the same pttern with rg. Thanks it, it
            // saves our time.
            match fzf_proc.stdin.as_ref().unwrap()
                .write_all(rg_output_str.as_bytes()) {
                    Err(why) => panic!("couldn't write to fzf stdin: {}", why),
                    Ok(_) => {},
                }

        }

        let fzf_out = fzf_proc.wait_with_output().unwrap();

        if fzf_out.status.success() {
            let fzf_stdout = String::from_utf8(fzf_out.stdout).unwrap();

            let split_fzf_out = fzf_stdout.split("\n")
                                          .collect::<Vec<&str>>();

            // Set the query string for next fzf process
            // query string is always the first string item
            // of split_fzf_out because we use fzf with
            // --print-query option.
            fzf_query = String::from(split_fzf_out[0]);

            // Users can use multi-select function of fzf, and all
            // selected patterns will be opened by nvim one by one.
            for pat in split_fzf_out[1..].into_iter() {
                if !pat.is_empty() {
                    let split_pat = pat.split(":")
                                       .collect::<Vec<&str>>();
                    let filename = split_pat[0];
                    let line = &format!("+{}", split_pat[1]);

                    let mut nvim_proc = match Command::new("nvim")
                        .arg(filename)
                        .arg(line)
                        .spawn() {
                            Err(why) => panic!("couldn't spawn nvim: {}", why),
                            Ok(nvim_proc) => nvim_proc,
                        };
                    nvim_proc.wait().expect("failed to wait on nvim");
                }
            }
        } else {
            // exit with error code?
            break;
        }
    }
}
