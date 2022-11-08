use std::process::Command;

pub fn preview(args: &[String]) {
    if args.len() < 1 {
        eprintln!("Usage: fzf-preview <rgout>");
        return;
    }

    let term_hight = args[1].parse::<usize>().unwrap();
    let rgout = &args[0];
    let rgarr: Vec<&str> = rgout.splitn(3, ":").collect();
    let filname = rgarr[0];
    let linum = rgarr[1].parse::<usize>().unwrap();
    let rem_termh = term_hight * 3 / 4;
    let startline;
    let stopline;
    if linum > rem_termh {
        startline = linum - rem_termh;
    } else {
        startline = 0;
    }
    stopline = startline + term_hight * 3;

    let view_cmd = format!(
        "bat -n --color=always -H {} -r {}:{} {}",
        linum, startline, stopline, filname
    );

    Command::new("bash")
        .arg("-c")
        .arg(&view_cmd)
        .spawn()
        .unwrap()
        .wait()
        .expect("run bash command failed!");
}

