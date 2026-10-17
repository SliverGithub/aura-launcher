//! `aura` — launch Aura Launcher.
//!
//! This tiny binary forwards to the sibling `aurelia` executable as
//! `aurelia aura`, so there is still only one implementation and one build.

use std::path::{Path, PathBuf};
use std::process::{Command, exit};

fn main() {
    let current = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("aura: could not resolve this executable: {error}");
            exit(1);
        }
    };
    let Some(aurelia) = find_aurelia(&current) else {
        eprintln!(
            "aura: no `aurelia` binary found next to {} or on PATH. Build/install both.",
            current.display()
        );
        exit(1);
    };

    let mut command = Command::new(&aurelia);
    command.arg("aura");
    command.args(std::env::args_os().skip(1));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        eprintln!("aura: failed to start {}: {error}", aurelia.display());
        exit(1);
    }

    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("aura: failed to start {}: {error}", aurelia.display());
            exit(1);
        }
    }
}

fn find_aurelia(current: &Path) -> Option<PathBuf> {
    if let Some(dir) = current.parent() {
        let sibling = dir.join("aurelia");
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("aurelia"))
        .find(|candidate| candidate.is_file())
}
