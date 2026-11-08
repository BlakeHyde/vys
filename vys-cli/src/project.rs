use crate::forms::{Form, Module, ModulePath, PublicationKind, display};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
struct Publication {
    scope: ModulePath,
    audience: Option<ModulePath>,
    target: ModulePath,
    kind: PublicationKind,
}

#[derive(Debug, Default)]
pub struct Project {
    pub modules: BTreeMap<ModulePath, Module>,
    publications: Vec<Publication>,
    scopes: BTreeSet<ModulePath>,
}

impl Project {
    pub fn from_forms(forms: &[Form]) -> Result<Self, String> {
        let mut project = Self::default();
        project.add_forms(forms, &[])?;
        project.validate()?;
        Ok(project)
    }

    pub fn add_forms(&mut self, forms: &[Form], scope: &[String]) -> Result<(), String> {
        self.scopes.insert(scope.to_vec());
        for form in forms {
            match form {
                Form::Module(module) => {
                    let mut path = scope.to_vec();
                    path.push(module.name.clone());
                    if self.modules.contains_key(&path) {
                        return Err(format!("duplicate module '{}'", display(&path)));
                    }
                    self.modules.insert(path, module.clone());
                }
                Form::Within { scope, forms } => self.add_forms(forms, scope)?,
                Form::Publish(publication) => {
                    if publication
                        .audience
                        .as_ref()
                        .is_some_and(|a| !scope.starts_with(a))
                    {
                        return Err("publication audience must be an ancestor scope".into());
                    }
                    for target in &publication.targets {
                        self.publications.push(Publication {
                            scope: scope.to_vec(),
                            audience: publication.audience.clone(),
                            target: target.path.clone(),
                            kind: target.kind,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Named publications expose names; context forwarding does not.
    fn exposed(&self, owner: &[String], target: &[String], audience: &[String]) -> bool {
        if owner == target {
            return true;
        }
        self.publications.iter().any(|p| {
            p.scope == owner
                && p.kind == PublicationKind::Named
                && p.audience.as_ref().is_none_or(|a| audience.starts_with(a))
                && p.target.starts_with(owner)
                && p.target.len() > owner.len()
                && target.starts_with(&p.target)
                && self.exposed(&p.target, target, audience)
        })
    }

    fn visible(&self, scope: &[String], target: &[String]) -> bool {
        if target.starts_with(scope) && target.len() > scope.len() {
            return self.exposed(&target[..scope.len() + 1], target, scope);
        }
        if scope.is_empty() {
            return false;
        }
        self.modules.get(scope).is_some_and(|m| {
            m.dependencies
                .iter()
                .any(|dep| target.starts_with(dep) && self.exposed(dep, target, scope))
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        for scope in &self.scopes {
            if !scope.is_empty() && !self.modules.contains_key(scope) {
                return Err(format!(
                    "within references undeclared module '{}'",
                    display(scope)
                ));
            }
        }
        for (path, module) in &self.modules {
            let scope = &path[..path.len() - 1];
            if !scope.is_empty() && !self.modules.contains_key(scope) {
                return Err(format!(
                    "module '{}' has undeclared parent '{}'",
                    display(path),
                    display(scope)
                ));
            }
            for target in &module.dependencies {
                if !self.modules.contains_key(target) {
                    return Err(format!(
                        "module '{}' depends on unknown module '{}'",
                        display(path),
                        display(target)
                    ));
                }
                if target == path {
                    return Err(format!("module '{}' depends on itself", display(path)));
                }
                if !self.visible(scope, target) {
                    return Err(format!(
                        "module '{}' cannot access '{}': dependency is outside its scope or is not published",
                        display(path),
                        display(target)
                    ));
                }
            }
        }
        for p in &self.publications {
            if !self.modules.contains_key(&p.target) {
                return Err(format!(
                    "scope '{}' publishes unknown module '{}'",
                    display(&p.scope),
                    display(&p.target)
                ));
            }
            if !self.visible(&p.scope, &p.target) {
                return Err(format!(
                    "scope '{}' cannot publish inaccessible module '{}'",
                    display(&p.scope),
                    display(&p.target)
                ));
            }
            if p.kind == PublicationKind::Named && !p.target.starts_with(&p.scope) {
                return Err(format!(
                    "named publication '{}' must be a descendant of '{}' (use from to forward dependency context)",
                    display(&p.target),
                    display(&p.scope)
                ));
            }
        }
        self.check_cycles()
    }

    fn check_cycles(&self) -> Result<(), String> {
        fn visit(
            p: &Project,
            node: &ModulePath,
            active: &mut Vec<ModulePath>,
            done: &mut BTreeSet<ModulePath>,
        ) -> Result<(), String> {
            if done.contains(node) {
                return Ok(());
            }
            if let Some(start) = active.iter().position(|n| n == node) {
                let mut cycle: Vec<_> = active[start..].iter().map(|n| display(n)).collect();
                cycle.push(display(node));
                return Err(format!("dependency cycle: {}", cycle.join(" -> ")));
            }
            active.push(node.clone());
            for dep in &p.modules[node].dependencies {
                visit(p, dep, active, done)?;
            }
            active.pop();
            done.insert(node.clone());
            Ok(())
        }
        let mut done = BTreeSet::new();
        for node in self.modules.keys() {
            visit(self, node, &mut Vec::new(), &mut done)?;
        }
        Ok(())
    }

    pub fn dot(&self) -> String {
        fn quote(s: &str) -> String {
            format!(
                "\"{}\"",
                s.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
            )
        }
        fn nodes(p: &Project, scope: &[String], level: usize, out: &mut String) {
            let indent = "  ".repeat(level);
            for path in p
                .modules
                .keys()
                .filter(|k| k.len() == scope.len() + 1 && k.starts_with(scope))
            {
                let name = display(path);
                let has_children = p
                    .modules
                    .keys()
                    .any(|k| k.len() > path.len() && k.starts_with(path));
                if has_children {
                    out.push_str(&format!(
                        "{indent}subgraph {} {{\n{indent}  label={};\n",
                        quote(&format!("cluster_{name}")),
                        quote(path.last().unwrap())
                    ));
                }
                let inner = if has_children {
                    format!("{indent}  ")
                } else {
                    indent.clone()
                };
                out.push_str(&format!(
                    "{inner}{} [label={}];\n",
                    quote(&name),
                    quote(path.last().unwrap())
                ));
                if has_children {
                    nodes(p, path, level + 1, out);
                    out.push_str(&format!("{indent}}}\n"));
                }
            }
        }
        let mut out =
            String::from("digraph vys {\n  compound=true;\n  rankdir=LR;\n  node [shape=box];\n");
        nodes(self, &[], 1, &mut out);
        for (path, module) in &self.modules {
            let mut deps = module.dependencies.clone();
            deps.sort();
            for dep in deps {
                out.push_str(&format!(
                    "  {} -> {};\n",
                    quote(&display(path)),
                    quote(&display(&dep))
                ));
            }
        }
        out.push_str("}\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::{Publication, PublicationTarget};

    fn module(name: &str, dependencies: Vec<ModulePath>) -> Form {
        Form::Module(Module {
            name: name.into(),
            files: vec![format!("./{name}.rs")],
            dependencies,
        })
    }

    #[test]
    fn builds_and_validates_without_text_or_syntax_nodes() {
        let forms = vec![
            module("api", vec![]),
            module("web", vec![vec!["api".into(), "types".into()]]),
            Form::Within {
                scope: vec!["api".into()],
                forms: vec![
                    module("types", vec![]),
                    Form::Publish(Publication {
                        audience: None,
                        targets: vec![PublicationTarget {
                            path: vec!["api".into(), "types".into()],
                            kind: PublicationKind::Named,
                        }],
                    }),
                ],
            },
        ];
        let project = Project::from_forms(&forms).unwrap();
        assert_eq!(
            project.modules[&vec!["api".into(), "types".into()]].files,
            vec!["./types.rs"]
        );
        assert!(project.dot().contains("\"web\" -> \"api/types\""));
    }

    #[test]
    fn canonical_data_still_requires_semantic_validation() {
        let forms = vec![module("api", vec![vec!["missing".into()]])];
        assert!(
            Project::from_forms(&forms)
                .unwrap_err()
                .contains("unknown module")
        );
    }
}
