use std::io::prelude::*;
use std::process::{Command, Stdio};
use term_size;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let shell_proc = match Command::new("bash")
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .spawn() {
        Err(why) => panic!("couldn't spawn shell: {}", why),
        Ok(shell_proc) => shell_proc,
    };
    let args_str = &args[1..].join(" ");

    let rg_cmd = format!("rg --with-filename --color=always -n {}",
                         args_str);
    match shell_proc.stdin.unwrap().write_all(rg_cmd.as_bytes()) {
        Err(why) => panic!("couldn't write to rg stdin: {}", why),
        Ok(_) => {},
    }

    let mut rg_out = String::new();
    match shell_proc.stdout.unwrap().read_to_string(&mut rg_out) {
        Err(why) => panic!("couldn't read rg stdout: {}", why),
        Ok(_) => {},
    }

    let (_, term_hight) = term_size::dimensions().unwrap();
    let fzf_cmd = &format!(r#"fzf --ansi -e --tac -0 --cycle \
                           --min-height=20 -d ':' \
                           --preview="echo '\033[1;32m {{1}}\033[0m'; \
                           fspreview {{}} {}" \
                           --preview-window=right:60%"#, term_hight);
    loop {
        let fzf_proc = match Command::new("bash")
            .arg("-c")
            .arg(fzf_cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn() {
                Err(why) => panic!("couldn't spawn fzf: {}", why),
                Ok(fzf_proc) => fzf_proc,
            };
        match fzf_proc.stdin.as_ref().unwrap()
            .write_all(rg_out.as_bytes()) {
            Err(why) => panic!("couldn't write to fzf stdin: {}", why),
            Ok(_) => {},
        }
        let fzf_out = fzf_proc.wait_with_output().unwrap();

        if fzf_out.status.success() {
            let fzf_stdout = String::from_utf8(fzf_out.stdout).unwrap();

            let split_fzf_out = fzf_stdout.split(":")
                                .collect::<Vec<&str>>();
            let filename = split_fzf_out[0];
            let line = &format!("+{}", split_fzf_out[1]);

            let mut nvim_proc = match Command::new("nvim")
                .arg(filename)
                .arg(line)
                .spawn() {
                    Err(why) => panic!("couldn't spawn nvim: {}", why),
                    Ok(nvim_proc) => nvim_proc,
                };
            nvim_proc.wait().expect("failed to wait on nvim");
        } else {
            break;
        }
    }
}
