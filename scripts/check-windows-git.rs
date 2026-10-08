//! Verify the app's Git module with an inert helper beside this probe.
#[allow(
    dead_code,
    reason = "the regression only needs the source module's public probe"
)]
#[path = "../crates/disktree-app/src/git.rs"]
mod git;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let fixture = std::path::PathBuf::from(args.next().expect("fixture path"));
    let marker = std::path::PathBuf::from(
        std::env::var_os("AUDIT_HELPER_MARKER").expect("marker path"),
    );
    if args.next().is_some_and(|arg| arg == "--control") {
        // Prove the original bare-name lookup reaches the adjacent helper.
        let status = std::process::Command::new("git")
            .arg("--version")
            .status()
            .expect("start the inert helper");
        assert!(status.success() && marker.is_file(), "inactive fixture");
        println!("CONTROL: bare-name lookup executed the adjacent git.exe.");
        return;
    }

    assert!(!marker.exists(), "the control marker was not cleared");
    let checkout = fixture.join("checkout");
    std::fs::create_dir_all(checkout.join(".git")).expect("checkout");
    let worktree = fixture.join("worktree");
    std::fs::create_dir_all(&worktree).expect("worktree");
    std::fs::write(worktree.join(".git"), "gitdir: elsewhere")
        .expect("worktree metadata");
    for path in [checkout, worktree] {
        assert!(git::is_checkout(&path), "fixture is a checkout");
        let state = git::state(&path);
        println!("Source-module Git state: {state:?}");
        assert!(!marker.exists(), "automatic inspection executed git.exe");
        assert!(state.is_none(), "Windows Git inspection must be unavailable");
    }
    println!("PASS: selecting both checkouts left the helper unexecuted.");
}
