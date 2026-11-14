//! Turns a checked-out KONTRA tree into CONTRA's scope, in place.
//!
//! ```text
//! cargo run -p upstream-scope -- <tree>
//! ```
//!
//! The same transform applied to KONTRA's old base and its new head gives a
//! diff in CONTRA's terms (see `README.md` here).
//!
//! - Deletes what CONTRA does not carry: other formats (UVI, SFZ), UVI Lua
//!   scripting, the website, release/CI/publishing, the old author's
//!   workflow, non-Rust tooling, and docs other than the Kontakt
//!   reverse-engineering evidence.
//! - Leaves the vendored plugin/UI frameworks and `Cargo.lock` out entirely:
//!   CONTRA vendors and patches its own copies, by hand.
//! - Renames kontra to contra (`KONTRA`, `Kontra`, `kontra`) in file
//!   contents and paths, except evidence, license texts and `NOTICE`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Deleted with everything under them.
const DELETE: &[&str] = &[
    ".claude/agents",
    ".claude/skills/v3-review",
    ".claude/skills/v3-task",
    ".coderabbit.yaml",
    ".impeccable",
    ".ignore",
    ".github",
    ".cargo/config.toml",
    "website",
    "handoff",
    "audits",
    "release-fixes.json",
    "SOURCE_COMMIT",
    "CHANGELOG.md",
    "ROADMAP.md",
    "PRODUCT.md",
    "DESIGN.md",
    "CONTRIBUTING.md",
    "README.md",
    "CLAUDE.md",
    "AGENTS.md",
    "crates/format/uvi",
    "crates/format/sfz",
    "crates/format/native",
    "vendor/uvi-file",
    "vendor/derpcat-flight-recorder",
    "vendor/buffr-durable-file",
    "licenses/BUFFR",
    "tools/xtask",
    "crates/app/host/tests/sfz_loading.rs",
    // The frameworks and the lockfile: CONTRA's own vendored copies, by hand.
    "vendor/MOOSE-PATCHES.md",
    "vendor/moose-state-queue.rs",
    "vendor/moose-state-queue-tests.rs",
    "vendor/vello",
    "vendor/xim-rs",
    "Cargo.lock",
];
const DELETE_PREFIX: &[&str] = &["vendor/moose", "vendor/mui"];
/// Under docs/, only these survive.
const KEEP_DOCS: &[&str] = &[
    "docs/re/",
    "docs/ALTERNATING_LOOPS.md",
    "docs/EDITOR_PREFERENCES.md",
    "docs/LEGAL.md",
    "docs/UI_NATIVE_PRESERVATION.md",
    "docs/shared-note-expression.md",
    "docs/KSP_",
];
/// Under tools/, only these survive (and none of their non-Rust scripts).
const KEEP_TOOLS: &[&str] = &[
    "tools/inspect/",
    "tools/ui-shots/",
    "tools/kontra-window/",
    "tools/contra-window/",
    "tools/clap-probe/",
    "tools/vst3-probe/",
    "tools/upstream/",
];
const DROP_EXT: &[&str] = &[
    ".py", ".sh", ".lua", ".java", ".mjs", ".js", ".cpp", ".html",
];
const NO_RENAME: &[&str] = &["docs/re/", "licenses/", "LICENSE", "NOTICE"];

fn doomed(rel: &str) -> bool {
    let under = |d: &str| rel == d || rel.strip_prefix(d).is_some_and(|r| r.starts_with('/'));
    if DELETE.iter().any(|d| under(d)) || DELETE_PREFIX.iter().any(|p| rel.starts_with(p)) {
        return true;
    }
    // UVI's Lua scripting; the Luau for Kontakt 8 native_ui pages stays.
    if rel.starts_with("crates/script/lua/")
        && ![
            "Cargo.toml",
            "src/alloc.rs",
            "src/interrupt.rs",
            "src/probe.rs",
            "src/nui",
        ]
        .iter()
        .any(|k| rel["crates/script/lua/".len()..].starts_with(k))
    {
        return true;
    }
    if rel.starts_with("docs/") {
        let name = rel.rsplit('/').next().unwrap_or(rel).to_ascii_lowercase();
        if !KEEP_DOCS.iter().any(|k| rel.starts_with(k))
            || name.contains("uvi")
            || name.contains("falcon")
            || name.ends_with(".py")
        {
            return true;
        }
    }
    if rel.starts_with("tools/") {
        // Test data: two Kontakt instruments the format tests load, and the
        // KSP scripts the compiler's tests run.
        if (rel.starts_with("tools/kontakt-reference/scenarios/") && rel.ends_with(".nki"))
            || (rel.starts_with("tools/ksp-conformance/cases/") && rel.ends_with(".ksp"))
        {
            return false;
        }
        return !KEEP_TOOLS.iter().any(|k| rel.starts_with(k))
            || DROP_EXT.iter().any(|e| rel.ends_with(e));
    }
    false
}

fn renamed(s: &str) -> String {
    s.replace("KONTRA", "CONTRA")
        .replace("Kontra", "Contra")
        .replace("kontra", "contra")
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if entry.file_name() != ".git" {
                files(&path, out)?;
            }
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Removes directories under `dir` that end up empty.
fn prune(dir: &Path, top: bool) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.file_name() != ".git" {
            prune(&entry.path(), false)?;
        }
    }
    if !top && fs::read_dir(dir)?.next().is_none() {
        fs::remove_dir(dir)?;
    }
    Ok(())
}

fn scope(tree: &Path) -> io::Result<()> {
    let mut all = Vec::new();
    files(tree, &mut all)?;
    for path in all {
        let rel = path
            .strip_prefix(tree)
            .expect("under the tree")
            .to_string_lossy()
            .into_owned();
        if doomed(&rel) {
            fs::remove_file(&path)?;
            continue;
        }
        if NO_RENAME.iter().any(|k| rel.starts_with(k)) {
            continue;
        }
        let data = fs::read(&path)?;
        if !data.contains(&0) {
            if let Ok(text) = std::str::from_utf8(&data) {
                let new = renamed(text);
                if new != text {
                    fs::write(&path, new)?;
                }
            }
        }
        let new_rel = renamed(&rel);
        if new_rel != rel {
            let dest = tree.join(&new_rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&path, &dest)?;
        }
    }
    prune(tree, true)
}

fn main() {
    let Some(tree) = std::env::args_os().nth(1) else {
        eprintln!("usage: upstream-scope <KONTRA tree>");
        std::process::exit(2);
    };
    if let Err(e) = scope(Path::new(&tree)) {
        eprintln!("upstream-scope: {e}");
        std::process::exit(1);
    }
}
