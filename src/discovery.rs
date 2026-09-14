//! Database discovery for csusage.
//!
//! Claude Science stores conversation metadata — including aggregate token
//! usage per frame — in a local SQLite database. An explicit path (via
//! `--db` or `CLAUDE_SCIENCE_DB`) is exclusive; otherwise well-known roots
//! and the daemon's org layout are scanned.

use std::{env, fs, path::PathBuf};

use crate::loader::is_claude_science_database;

const CLAUDE_SCIENCE_DB_ENV: &str = "CLAUDE_SCIENCE_DB";

/// Known database filename used by the Claude Science daemon.
const DATABASE_FILE_NAME: &str = "operon-cli.db";

/// Roots (relative to the user's home directory) that may hold the database.
const DEFAULT_ROOTS: [&str; 6] = [
    ".claude-science",
    ".config/claude-science",
    ".config/Claude Science",
    ".local/share/claude-science",
    ".local/share/Claude Science",
    "Library/Application Support/Claude Science",
];

/// Directory names never entered during discovery scanning.
const SKIPPED_DIRECTORY_NAMES: [&str; 3] = ["conda", "pkgs", "node_modules"];

/// Returns every readable Claude Science metadata database.
pub fn database_paths(explicit: Option<&str>) -> Result<Vec<PathBuf>, String> {
    if let Some(explicit) = explicit.map(str::to_owned).or_else(|| env::var(CLAUDE_SCIENCE_DB_ENV).ok()) {
        if explicit.is_empty() {
            return Ok(Vec::new());
        }
        let paths = explicit
            .split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .filter(|path| is_claude_science_database(path))
            .collect();
        return Ok(paths);
    }

    let mut paths = Vec::new();
    for root in default_roots() {
        collect_database_files(&root, 0, &mut paths);
    }
    for path in org_database_paths() {
        push_unique(&mut paths, path);
    }
    paths.sort();
    Ok(paths
        .into_iter()
        .filter(|path| is_claude_science_database(path))
        .collect())
}

fn default_roots() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    DEFAULT_ROOTS.iter().map(|root| home.join(root)).collect()
}

/// The daemon's org layout: `<home>/.claude-science/cs-switch-proxy/orgs/<org>/`.
fn org_database_paths() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let orgs = home.join(".claude-science/cs-switch-proxy/orgs");
    let Ok(entries) = fs::read_dir(&orgs) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for entry in entries.filter_map(std::result::Result::ok) {
        if !entry.file_type().is_ok_and(|type_| type_.is_dir()) {
            continue;
        }
        let path = entry.path().join(DATABASE_FILE_NAME);
        if path.is_file() {
            paths.push(path);
        }
    }
    paths
}

/// Walks a root looking for SQLite files, bounded to two levels so a real
/// home directory's unrelated trees are never scanned.
fn collect_database_files(directory: &std::path::Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > 2 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(std::result::Result::ok) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && depth > 0 {
            continue;
        }
        if file_type.is_dir() {
            if SKIPPED_DIRECTORY_NAMES.contains(&name.as_ref()) {
                continue;
            }
            collect_database_files(&path, depth + 1, files);
        } else if file_type.is_file() {
            let looks_like_database = name.ends_with(".db")
                || name.ends_with(".sqlite")
                || name.ends_with(".sqlite3")
                || name == DATABASE_FILE_NAME;
            if looks_like_database {
                files.push(path);
            }
        }
    }
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

/// Minimal home-directory resolution without extra dependencies.
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
