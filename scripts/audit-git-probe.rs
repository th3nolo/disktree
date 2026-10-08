//! Exercise the reviewed Git module on an isolated Windows runner.
#[allow(dead_code, reason = "the fixture only needs the source module's public probe")]
#[path = "../crates/disktree-app/src/git.rs"]
mod git;

fn main() {
    let checkout = std::path::PathBuf::from(
        std::env::args_os().nth(1).expect("fixture checkout path"),
    );
    std::fs::create_dir_all(checkout.join(".git")).expect("fixture checkout");
    let state = git::state(&checkout);
    println!("Source-module Git state: {state:?}");
    let marker = std::env::var_os("AUDIT_HELPER_MARKER").expect("marker path");
    assert!(
        std::path::Path::new(&marker).is_file(),
        "the adjacent helper was not executed"
    );
    println!("CONFIRMED: an adjacent git.exe executed during automatic Git inspection.");
}
