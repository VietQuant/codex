use codex_protocol::custom_prompts::CustomPrompt;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;

/// Return the default prompts directory: `$CODEX_HOME/prompts`.
/// If `CODEX_HOME` cannot be resolved, returns `None`.
pub fn default_prompts_dir() -> Option<PathBuf> {
    crate::config::find_codex_home()
        .ok()
        .map(|home| home.join("prompts"))
}

/// Discover prompt files in the given directory, returning entries sorted by name.
/// Non-files are ignored. If the directory does not exist or cannot be read, returns empty.
pub async fn discover_prompts_in(dir: &Path) -> Vec<CustomPrompt> {
    discover_prompts_in_excluding(dir, &HashSet::new()).await
}

/// Discover prompt files in the given directory, excluding any with names in `exclude`.
/// Returns entries sorted by name. Non-files are ignored. Missing/unreadable dir yields empty.
pub async fn discover_prompts_in_excluding(
    dir: &Path,
    exclude: &HashSet<String>,
) -> Vec<CustomPrompt> {
    let mut out: Vec<CustomPrompt> = Vec::new();
    let mut entries = match fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return out,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let is_file = entry
            .file_type()
            .await
            .map(|ft| ft.is_file())
            .unwrap_or(false);
        if !is_file {
            continue;
        }
        // Only include Markdown files with a .md extension.
        let is_md = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("md"))
            .unwrap_or(false);
        if !is_md {
            continue;
        }
        let Some(name) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if exclude.contains(&name) {
            continue;
        }
        let content = match fs::read_to_string(&path).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        out.push(CustomPrompt {
            name,
            path,
            content,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Return the project prompts directory for a given project root: `<root>/.codex/prompts`.
pub fn project_prompts_dir(root: &Path) -> PathBuf {
    root.join(".codex").join("prompts")
}

/// Discover prompts from the project directory and the personal directory, merging them
/// with project prompts taking precedence over personal prompts on name collisions.
/// Results are sorted by name.
pub async fn discover_project_and_personal_prompts(
    project_root: &Path,
    exclude: &HashSet<String>,
    personal_dir: Option<PathBuf>,
) -> Vec<CustomPrompt> {
    let project_dir = project_prompts_dir(project_root);
    let mut by_name: HashMap<String, CustomPrompt> = HashMap::new();

    // Project prompts first (higher precedence)
    for p in discover_prompts_in_excluding(&project_dir, exclude).await {
        by_name.insert(p.name.clone(), p);
    }
    // Then personal prompts, only if not already present
    if let Some(dir) = personal_dir.as_deref() {
        for p in discover_prompts_in_excluding(dir, exclude).await {
            by_name.entry(p.name.clone()).or_insert(p);
        }
    }

    let mut out: Vec<CustomPrompt> = by_name.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn empty_when_dir_missing() {
        let tmp = tempdir().expect("create TempDir");
        let missing = tmp.path().join("nope");
        let found = discover_prompts_in(&missing).await;
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn discovers_and_sorts_files() {
        let tmp = tempdir().expect("create TempDir");
        let dir = tmp.path();
        fs::write(dir.join("b.md"), b"b").unwrap();
        fs::write(dir.join("a.md"), b"a").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();
        let found = discover_prompts_in(dir).await;
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn excludes_builtins() {
        let tmp = tempdir().expect("create TempDir");
        let dir = tmp.path();
        fs::write(dir.join("init.md"), b"ignored").unwrap();
        fs::write(dir.join("foo.md"), b"ok").unwrap();
        let mut exclude = HashSet::new();
        exclude.insert("init".to_string());
        let found = discover_prompts_in_excluding(dir, &exclude).await;
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["foo"]);
    }

    #[tokio::test]
    async fn skips_non_utf8_files() {
        let tmp = tempdir().expect("create TempDir");
        let dir = tmp.path();
        // Valid UTF-8 file
        fs::write(dir.join("good.md"), b"hello").unwrap();
        // Invalid UTF-8 content in .md file (e.g., lone 0xFF byte)
        fs::write(dir.join("bad.md"), vec![0xFF, 0xFE, b'\n']).unwrap();
        let found = discover_prompts_in(dir).await;
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["good"]);
    }

    #[tokio::test]
    async fn project_overrides_personal_and_merges() {
        let tmp = tempdir().expect("create TempDir");
        let root = tmp.path();
        let proj_dir = project_prompts_dir(root);
        std::fs::create_dir_all(&proj_dir).unwrap();
        // project: shared.md overrides, and x.md unique
        fs::write(proj_dir.join("shared.md"), b"project").unwrap();
        fs::write(proj_dir.join("x.md"), b"x").unwrap();

        // personal: shared.md (should be ignored), and y.md unique
        let personal = root.join("personal");
        std::fs::create_dir_all(&personal).unwrap();
        fs::write(personal.join("shared.md"), b"personal").unwrap();
        fs::write(personal.join("y.md"), b"y").unwrap();

        let exclude = HashSet::new();
        let found = discover_project_and_personal_prompts(root, &exclude, Some(personal)).await;
        let mut names: Vec<(String, String)> = found
            .into_iter()
            .map(|e| (e.name.clone(), e.content))
            .collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        pretty_assertions::assert_eq!(
            names,
            vec![
                ("shared".to_string(), "project".to_string()),
                ("x".to_string(), "x".to_string()),
                ("y".to_string(), "y".to_string()),
            ]
        );
    }
}
