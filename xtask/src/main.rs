//! Repository tasks. `cargo xtask ci` is the merge gate CI runs: format check, clippy with
//! warnings denied, the test suite, and the asset attribution check.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let task = env::args().nth(1).unwrap_or_default();
    let result = match task.as_str() {
        "ci" => ci(),
        "fmt" => run("cargo", &["fmt", "--all"]),
        "clippy" => clippy(),
        "test" => run("cargo", &["test", "--workspace"]),
        "assets" => assets(),
        _ => Err(format!("unknown task {task:?}; tasks are ci, fmt, clippy, test, assets")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn ci() -> Result<(), String> {
    run("cargo", &["fmt", "--all", "--check"])?;
    clippy()?;
    run("cargo", &["test", "--workspace"])?;
    assets()
}

fn clippy() -> Result<(), String> {
    run("cargo", &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program).args(args).status().map_err(|error| format!("cannot start {program}: {error}"))?;
    if status.success() { Ok(()) } else { Err(format!("{program} {} failed", args.join(" "))) }
}

/// Every file under `assets/` must have a row in ATTRIBUTION.md (AGENTS.md, Assets).
fn assets() -> Result<(), String> {
    let assets_dir = Path::new("assets");
    if !assets_dir.exists() {
        return Ok(());
    }
    let listing = std::fs::read_to_string("ATTRIBUTION.md").map_err(|error| format!("cannot read ATTRIBUTION.md: {error}"))?;
    let mut missing = Vec::new();
    walk(assets_dir, 0, &mut |file| {
        let name = file.to_string_lossy().replace('\\', "/");
        if !listing.contains(&name) {
            missing.push(name);
        }
    })?;
    if missing.is_empty() { Ok(()) } else { Err(format!("assets without an ATTRIBUTION.md row: {}", missing.join(", "))) }
}

fn walk(dir: &Path, depth: usize, found: &mut impl FnMut(PathBuf)) -> Result<(), String> {
    if depth > 32 {
        return Err(format!("the assets tree nests deeper than 32 at {}", dir.display()));
    }
    let entries = std::fs::read_dir(dir).map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, depth + 1, found)?;
        } else {
            found(path);
        }
    }
    Ok(())
}
