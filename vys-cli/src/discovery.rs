use crate::forms::{Form, ModulePath, display};
use crate::project::Project;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Discovery {
    pub forms: Vec<Form>,
    pub project: Project,
    pub orphans: Vec<PathBuf>,
}

pub fn find_root(start: &Path) -> Result<PathBuf, String> {
    let start = start
        .canonicalize()
        .map_err(|e| format!("{}: {e}", start.display()))?;
    for dir in start.ancestors() {
        if dir.join("project.vys").is_file() {
            return Ok(dir.to_path_buf());
        }
    }
    Err(format!(
        "no project.vys found in '{}' or its ancestors",
        start.display()
    ))
}

fn scan(root: &Path, dir: &Path, files: &mut Vec<(ModulePath, PathBuf)>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", entry.path().display()))?;
        if kind.is_dir() {
            // Do not enter VCS metadata. Symlinks are never followed.
            if matches!(entry.file_name().to_str(), Some(".git" | ".jj")) {
                continue;
            }
            scan(root, &entry.path(), files)?;
        } else if kind.is_file() && entry.file_name() == "module.vys" {
            let relative = dir.strip_prefix(root).unwrap();
            let scope = relative
                .components()
                .map(|c| {
                    c.as_os_str()
                        .to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| format!("non-UTF-8 module directory: {}", dir.display()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            files.push((scope, entry.path()));
        }
    }
    Ok(())
}

/// Find reachable module files and collect their canonical forms.
///
/// The caller supplies loading/parsing. The loader receives an empty scope for
/// project.vys and the containing module's absolute scope for module.vys. It is
/// never called for orphaned files, and may retain state between calls.
pub fn discover(
    root: &Path,
    mut load: impl FnMut(&Path, &[String]) -> Result<Vec<Form>, String>,
) -> Result<Discovery, String> {
    let root_file = root.join("project.vys");
    let mut forms = load(&root_file, &[])?;
    let mut project = Project::default();
    project
        .add_forms(&forms, &[])
        .map_err(|e| format!("{}: {e}", root_file.display()))?;
    let mut pending = Vec::new();
    scan(root, root, &mut pending)?;
    pending.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(&b.0)));
    // Iterate to a fixed point: a reachable file can declare a scope containing
    // another file, independent of filesystem traversal order.
    loop {
        let mut progress = false;
        let mut remaining = Vec::new();
        for (scope, path) in pending {
            if !scope.is_empty() && project.modules.contains_key(&scope) {
                let local = load(&path, &scope)?;
                project
                    .add_forms(&local, &scope)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                forms.push(Form::Within {
                    scope,
                    forms: local,
                });
                progress = true;
            } else {
                remaining.push((scope, path));
            }
        }
        pending = remaining;
        if !progress {
            break;
        }
    }
    let orphans = pending.into_iter().map(|(_, path)| path).collect();
    project.validate()?;
    Ok(Discovery {
        forms,
        project,
        orphans,
    })
}

pub fn orphan_warning(path: &Path, root: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let scope: Vec<_> = relative
        .parent()
        .unwrap_or(Path::new(""))
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    format!(
        "warning: orphaned module file '{}' omitted (scope '{}' is not a declared module)",
        relative.display(),
        display(&scope)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::Module;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(files: &[&str]) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "vys-discovery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            for file in files {
                let path = root.join(file);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                // Deliberately not vys text: discovery must use the loader.
                fs::write(path, [0xff]).unwrap();
            }
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn module(name: &str) -> Form {
        Form::Module(Module {
            name: name.into(),
            files: Vec::new(),
            dependencies: Vec::new(),
        })
    }

    #[test]
    fn injected_loader_receives_scopes_and_never_loads_orphans() {
        let fixture = Fixture::new(&[
            "project.vys",
            "api/module.vys",
            "api/child/module.vys",
            "orphan/module.vys",
        ]);
        let mut calls = Vec::new();
        let result = discover(&fixture.0, |path, scope| {
            calls.push((
                path.strip_prefix(&fixture.0).unwrap().to_path_buf(),
                scope.to_vec(),
            ));
            match scope {
                [] => Ok(vec![module("api")]),
                [api] if api == "api" => Ok(vec![module("child")]),
                [api, child] if api == "api" && child == "child" => Ok(vec![module("leaf")]),
                _ => panic!("unexpected load: {}", path.display()),
            }
        })
        .unwrap();
        assert_eq!(
            calls,
            vec![
                (PathBuf::from("project.vys"), vec![]),
                (PathBuf::from("api/module.vys"), vec!["api".into()]),
                (
                    PathBuf::from("api/child/module.vys"),
                    vec!["api".into(), "child".into()]
                ),
            ]
        );
        assert_eq!(result.orphans, vec![fixture.0.join("orphan/module.vys")]);
        assert!(result.project.modules.contains_key(&vec![
            "api".into(),
            "child".into(),
            "leaf".into()
        ]));
    }

    #[test]
    fn loader_declarations_can_make_previously_unreachable_files_reachable() {
        let fixture = Fixture::new(&["project.vys", "a/module.vys", "z/module.vys"]);
        let mut calls = Vec::new();
        let result = discover(&fixture.0, |_, scope| {
            calls.push(scope.to_vec());
            match scope {
                [] => Ok(vec![module("z")]),
                [name] if name == "z" => Ok(vec![Form::Within {
                    scope: vec![],
                    forms: vec![module("a")],
                }]),
                [name] if name == "a" => Ok(vec![module("child")]),
                _ => panic!("unexpected scope"),
            }
        })
        .unwrap();
        assert_eq!(
            calls,
            vec![vec![], vec!["z".to_string()], vec!["a".to_string()]]
        );
        assert!(result.orphans.is_empty());
        assert!(
            result
                .project
                .modules
                .contains_key(&vec!["a".into(), "child".into()])
        );
    }

    #[test]
    fn loader_errors_propagate_without_loading_further_files() {
        let fixture = Fixture::new(&["project.vys", "api/module.vys"]);
        let mut calls = 0;
        let result = discover(&fixture.0, |_, scope| {
            calls += 1;
            if scope.is_empty() {
                Ok(vec![module("api")])
            } else {
                Err("loader failed".into())
            }
        });
        assert!(matches!(result, Err(e) if e == "loader failed"));
        assert_eq!(calls, 2);
    }
}
