//! An inert executable used only for the Windows Git lookup regression.
fn main() {
    let marker =
        std::env::var_os("AUDIT_HELPER_MARKER").expect("fixture marker");
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(marker)
        .expect("write fixture evidence");
    let args: Vec<_> = std::env::args().skip(1).collect();
    writeln!(file, "{}", args.join(" ")).expect("record invocation");
    if args.iter().any(|arg| arg == "config") {
        std::process::exit(1);
    }
    if args.iter().any(|arg| arg == "rev-list") {
        println!("0");
    }
}
