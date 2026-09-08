//! Requires every note script to state who may consume it.
//!
//! An open note and a forgotten check look identical in a script's body, so a script either
//! enforces its rule itself, declares itself open to any consumer, or names the account procedure
//! that enforces the rule on its behalf. This fails for one that does none of those.

use std::path::{Path, PathBuf};

/// The procedure prefixes a note script uses to enforce who may consume it: the target account it
/// commits to, or the reclaimer it lets take the assets back.
const ENFORCEMENT_PREFIXES: [&str; 2] = ["exec.consumer::", "exec.reclaim::"];

/// What a note script open to any consumer declares instead.
const UNRESTRICTED_DECLARATION: &str = "#! Consumers: unrestricted";

/// What a note script whose rule is enforced by the account procedure it calls declares instead.
/// The declaration has to name where the rule is enforced, so the delegation is stated rather than
/// left to the reader of the called component.
const DELEGATED_DECLARATION: &str = "#! Consumers: target account (enforced by";

// TESTS
// ================================================================================================

#[test]
fn every_note_script_states_who_may_consume_it() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));

    for asm_dir in [manifest.join("../miden-standards/asm"), manifest.join("../miden-agglayer/asm")]
    {
        let scripts = find_note_scripts(&asm_dir);
        assert!(!scripts.is_empty(), "no note scripts found under {}", asm_dir.display());

        for path in scripts {
            let source = module_source(&path);
            let enforces = ENFORCEMENT_PREFIXES.iter().any(|prefix| source.contains(prefix));
            let declares =
                source.contains(UNRESTRICTED_DECLARATION) || source.contains(DELEGATED_DECLARATION);
            assert!(
                enforces || declares,
                "the note script at {} neither enforces who may consume it through one of \
                 {ENFORCEMENT_PREFIXES:?} nor declares `{UNRESTRICTED_DECLARATION}` or \
                 `{DELEGATED_DECLARATION} ...)`",
                path.display(),
            );
        }
    }
}

// HELPERS
// ================================================================================================

/// Returns the paths of every MASM file under `asm_dir` that defines a note script.
fn find_note_scripts(asm_dir: &Path) -> Vec<PathBuf> {
    let mut scripts = Vec::new();
    let mut dirs = vec![asm_dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "masm")
                && defines_note_script(&path)
            {
                scripts.push(path);
            }
        }
    }
    scripts
}

/// Returns whether the MASM file at `path` defines a note script, i.e. carries the `@note_script`
/// attribute rather than merely mentioning it in a doc comment.
fn defines_note_script(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .any(|line| line.trim() == "@note_script")
}

/// Returns the source of the note script's module: the file itself, plus its siblings when the note
/// script is a `mod.masm` whose submodules make up the script.
fn module_source(path: &Path) -> String {
    let source = std::fs::read_to_string(path).unwrap();
    if path.file_name().is_none_or(|name| name != "mod.masm") {
        return source;
    }

    let mut sources = vec![source];
    for entry in std::fs::read_dir(path.parent().unwrap()).unwrap() {
        let sibling = entry.unwrap().path();
        if sibling != path && sibling.extension().is_some_and(|ext| ext == "masm") {
            sources.push(std::fs::read_to_string(&sibling).unwrap());
        }
    }
    sources.join("\n")
}
