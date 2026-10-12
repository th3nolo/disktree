#[cfg(windows)]
fn main() {
    use std::{env, fs, path::PathBuf, process::Command};

    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../assets/disktree.ico")
        .canonicalize()
        .expect("Windows icon asset is missing");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let rc = out.join("disktree.rc");
    let res = out.join("disktree.res");

    // rc.exe treats backslashes inside quoted paths as separators, not escapes.
    let icon_path = icon.to_string_lossy().replace('\\', "/");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let repository = env::var("CARGO_PKG_REPOSITORY").unwrap();
    let version_parts = [
        "CARGO_PKG_VERSION_MAJOR",
        "CARGO_PKG_VERSION_MINOR",
        "CARGO_PKG_VERSION_PATCH",
    ]
    .map(|part| env::var(part).unwrap())
    .join(",");
    // Explorer needs a VERSIONINFO resource; changing Cargo's version alone
    // leaves the downloaded executable's Details tab without an identity.
    let resources = format!(
        r#"1 ICON "{icon_path}"
1 VERSIONINFO
FILEVERSION {version_parts},0
PRODUCTVERSION {version_parts},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "FileDescription", "DiskTree personal fork\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "ProductName", "DiskTree\0"
            VALUE "ProductVersion", "{version}\0"
            VALUE "OriginalFilename", "disktree.exe\0"
            VALUE "Comments", "{repository}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#,
    );
    fs::write(&rc, resources)
        .expect("could not write Windows icon and version resource script");

    // Cargo can run outside a Visual Studio prompt, where the SDK bin is not
    // on PATH even though the MSVC linker and Windows SDK are installed.
    let rc_exe = env::var_os("DISKTREE_RC").map_or_else(
        || {
            let sdk_bin = env::var_os("WindowsSdkDir")
                .map(PathBuf::from)
                .or_else(|| {
                    env::var_os("ProgramFiles(x86)")
                        .map(|root| PathBuf::from(root).join("Windows Kits/10"))
                })
                .expect("Windows SDK is required to embed the icon")
                .join("bin");
            let host = match env::consts::ARCH {
                "x86_64" => "x64",
                "aarch64" => "arm64",
                "x86" => "x86",
                other => panic!("unsupported Windows build host: {other}"),
            };
            let mut versions: Vec<_> = fs::read_dir(&sdk_bin)
                .expect("Windows SDK bin directory is missing")
                .filter_map(Result::ok)
                .map(|entry| entry.path().join(host).join("rc.exe"))
                .filter(|path| path.is_file())
                .collect();
            versions.sort();
            versions
                .pop()
                .expect("rc.exe is missing from the Windows SDK")
        },
        PathBuf::from,
    );
    let status = Command::new(rc_exe)
        .arg("/nologo")
        .arg(format!("/fo{}", res.display()))
        .arg(&rc)
        .status()
        .expect("could not start rc.exe to embed the Windows icon");
    assert!(
        status.success(),
        "rc.exe failed to compile the Windows icon"
    );

    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_REPOSITORY");
    println!("cargo:rustc-link-arg-bin=disktree={}", res.display());
}

#[cfg(not(windows))]
fn main() {}
