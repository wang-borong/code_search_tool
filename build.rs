fn main() {
    // Windows debug builds use a 1 MB default stack which overflows when
    // clap parses the large Commands enum.  Request an 8 MB stack to match
    // the default on Linux and macOS.
    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-arg=/STACK:8388608");
    }
}
