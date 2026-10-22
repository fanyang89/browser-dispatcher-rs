//! Standalone fake browser compiled by integration tests and platform smoke tests.
use std::{env, fs, path::PathBuf};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let directory = PathBuf::from(env::var_os("BROWSER_DISPATCHER_TEST_OUTPUT").expect("output directory"));
    fs::create_dir_all(&directory).unwrap();
    // Each process owns a file, so concurrent URL deliveries cannot interleave.
    let output = directory.join(format!("{}.args", std::process::id()));
    let temporary = output.with_extension("tmp");
    fs::write(&temporary, args.join("\0")).unwrap();
    fs::rename(temporary, output).unwrap();
}
