//! Where a project may live: inside the operator's `workspaces_root`, never
//! outside it, and never through a symlink or a junction.

use std::path::{Component, Path, PathBuf};
/// Resolve a project directory under the operator's root. Rejects absolute
/// paths, `..`, existing symlinks and canonicalized escapes (junctions on
/// Windows are followed by `canonicalize`, so the final `starts_with` catches
/// them even when `symlink_metadata` calls them ordinary directories).
pub(crate) fn resolve_project_path(
    root: &Path,
    name: &str,
    requested: Option<&str>,
) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("workspaces_root is not accessible: {error}"))?;
    let relative = match requested {
        Some(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
        _ => slug(name)?,
    };
    let relative = Path::new(&relative);
    if relative.is_absolute() {
        return Err("path must be relative to workspaces_root".to_string());
    }
    for component in relative.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err("path cannot contain '.', '..' or a prefix".to_string()),
        }
    }
    let mut prefix = root.clone();
    for component in relative.components() {
        prefix.push(component);
        if let Ok(metadata) = std::fs::symlink_metadata(&prefix) {
            if metadata.file_type().is_symlink() {
                return Err(format!("path component {} is a symlink", prefix.display()));
            }
        }
    }
    let candidate = root.join(relative);
    std::fs::create_dir_all(&candidate)
        .map_err(|error| format!("could not create {}: {error}", candidate.display()))?;
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("could not canonicalize {}: {error}", candidate.display()))?;
    if !canonical.starts_with(&root) {
        return Err("path escapes workspaces_root".to_string());
    }
    Ok(canonical)
}

pub(super) fn slug(name: &str) -> Result<String, String> {
    let mut slug = String::new();
    let mut dash = false;
    for character in name.chars() {
        if character.is_alphanumeric() {
            for lower in character.to_lowercase() {
                slug.push(lower);
            }
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        return Err("name has no usable characters for a directory".to_string());
    }
    Ok(slug)
}

/// The role prompt of a resident seat: what it may do and how it ends.
pub(super) fn role_text(role_id: &str, name: &str) -> String {
    match role_id {
        "lead" => format!(
            "Eres {name}, lead del proyecto. Recibes una tarea de Alfred, puedes crear \
             especialistas con agent.spawn y repartirles trabajo con delegation.assign. \
             Cuando termines, llama a delegation.complete con el resultado completo."
        ),
        _ => format!(
            "Eres {name}, especialista del proyecto. Ejecuta la tarea que recibes y \
             termínala con delegation.complete; no delegues ni crees otros asientos."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

    #[test]
    fn paths_stay_inside_the_workspace_root() {
        let root = TestDir::new("team-root");
        let inside = resolve_project_path(&root, "Demo", None).expect("inside");
        assert!(inside.ends_with("demo"));
        assert!(resolve_project_path(&root, "Demo", Some("../escape")).is_err());
        assert!(resolve_project_path(&root, "Demo", Some("C:\\escape")).is_err());
        assert!(resolve_project_path(&root, "", Some("/abs")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_escapes_is_rejected() {
        let root = TestDir::new("team-symlink");
        let outside = TestDir::new("team-outside");
        std::os::unix::fs::symlink(&outside, root.join("demo")).expect("symlink");
        assert!(resolve_project_path(&root, "Demo", None).is_err());
    }

    /// On Windows a junction is not a symlink for `symlink_metadata`: only the
    /// canonicalized path tells that it leaves the root.
    #[cfg(windows)]
    #[test]
    fn a_junction_that_escapes_is_rejected() {
        let root = TestDir::new("team-junction");
        let outside = TestDir::new("team-junction-out");
        let link = root.join("demo");
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path())
            .output()
            .expect("mklink");
        if !output.status.success() {
            // No junction support in this environment: nothing to prove.
            return;
        }
        assert!(resolve_project_path(&root, "Demo", None).is_err());
    }

    #[test]
    fn slugs_are_stable() {
        assert_eq!(slug("Demo").expect("slug"), "demo");
        assert_eq!(slug("Plan 2026!").expect("slug"), "plan-2026");
        assert!(slug("!!").is_err());
    }
}
