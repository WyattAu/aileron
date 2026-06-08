use std::process::Command;

fn main() {
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let git_dirty = Command::new("git")
        .args(["diff", "--quiet", "HEAD"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);

    let hash = if git_dirty {
        format!("{git_hash}-dirty")
    } else {
        git_hash
    };

    println!("cargo:rustc-env=AILERON_GIT_HASH={hash}");
    println!("cargo:rerun-if-executed=build-script");
}
