use std::fs::{self, File};
use std::io::{self, BufRead, Read};
use std::path::Path;

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

pub fn create_ignore(f: &str, init: bool) {
    let fp = Path::new(f);
    let path = Path::new(f).parent().unwrap();
    if fp.exists() {
        return;
    }
    // create ignore file
    let mut _file = match File::create(&fp) {
        Err(why) => panic!("couldn't create {}: {}", f, why),
        Ok(file) => file,
    };
    if init {
        return;
    }
    // several conditions
}

pub fn add_ignore(f: &str, pats: &[String]) {
    if Path::new(f).exists() {
        if let Ok(mut lines) = read_lines(f) {
            let mut newpats: Vec<String> = Vec::from(pats);
            let mut newlines: Vec<String> = Vec::new();
            for line in lines.by_ref() {
                if let Ok(line) = line {
                    newlines.push(line);
                }
            }
            for i in 0..pats.len() {
                for line in &mut newlines {
                    if pats[i] == line.trim() {
                        newpats.remove(i);
                    }
                }
            }
            newlines.append(&mut newpats);
            fs::write(f, newlines.join("\n") + "\n").expect("write file failed");
        }
    } else {
        // create file and add default ignore patterns
        create_ignore(f, false);
    }
}

pub fn remove_ignore(f: &str, pats: &[String]) {
    if Path::new(f).exists() {
        let mut newlines:Vec<String> = Vec::new();
        if let Ok(mut lines) = read_lines(f) {
            for pat in pats {
                for line in lines.by_ref() {
                    if let Ok(line) = line {
                        if line.trim() != pat {
                            newlines.push(line);
                        }
                    }
                }
            }
        }
        fs::write(f, newlines.join("\n") + "\n").expect("write file failed");
    } else {
        println!("{} don't exist", f);
    }
}

pub fn list_ignore(f: &str) {
    if Path::new(f).exists() {
        let mut file = match File::open(&f) {
            Err(why) => panic!("couldn't open {}: {}", f, why),
            Ok(file) => file,
        };
        let mut s = String::new();
        match file.read_to_string(&mut s) {
            Err(why) => panic!("couldn't read {}: {}", f, why),
            Ok(_) => print!("{} contains:\n{}", f, s),
        }
    } else {
        println!("{} don't exist", f);
    }
}
