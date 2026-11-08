//! Text boundary: S-expressions in, canonical forms out (and back again).
//! No generic syntax nodes escape this module.
mod sexpr;

use crate::forms::{
    Form, Module, ModulePath, Publication, PublicationKind, PublicationTarget, display,
};
use sexpr::Expr;

pub fn parse_project(input: &str, source: &str) -> Result<Vec<Form>, String> {
    let forms = sexpr::parse(input, source)?;
    unwrap_project(forms)
        .and_then(|forms| decode(&forms, &[]))
        .map_err(|e| format!("{source}: {e}"))
}

pub fn parse_combined(input: &str, source: &str) -> Result<Vec<Form>, String> {
    let forms = sexpr::parse(input, source)?;
    if forms.len() != 1 || forms[0].head() != Some("vys-project") {
        return Err(format!(
            "{source}: stdin must contain exactly one (vys-project ...) form, as emitted by vys combine"
        ));
    }
    unwrap_project(forms)
        .and_then(|forms| decode(&forms, &[]))
        .map_err(|e| format!("{source}: {e}"))
}

pub fn parse_module(input: &str, source: &str, scope: &[String]) -> Result<Vec<Form>, String> {
    decode(&sexpr::parse(input, source)?, scope).map_err(|e| format!("{source}: {e}"))
}

fn unwrap_project(forms: Vec<Expr>) -> Result<Vec<Expr>, String> {
    if forms.len() == 1 && forms[0].head() == Some("vys-project") {
        let Expr::List(xs) = forms.into_iter().next().unwrap() else {
            unreachable!()
        };
        Ok(xs.into_iter().skip(1).collect())
    } else if forms.iter().any(|x| x.head() == Some("vys-project")) {
        Err("vys-project must be the only top-level form".into())
    } else {
        Ok(forms)
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !matches!(name, "." | "..")
        && !name.chars().any(|c| matches!(c, '/' | '\\' | '\0'))
}

fn reference(expr: &Expr, scope: &[String]) -> Result<ModulePath, String> {
    let xs = expr
        .list()
        .ok_or_else(|| format!("expected module reference, got {expr}"))?;
    let head = xs
        .first()
        .and_then(Expr::atom)
        .ok_or("empty module reference")?;
    let mut path = match head {
        "/" | "project" => Vec::new(),
        "." | "module" => scope.to_vec(),
        ".." | "parent" => {
            let mut p = scope.to_vec();
            p.pop()
                .ok_or_else(|| format!("reference {expr} goes above the project root"))?;
            p
        }
        _ => return Err(format!("unknown reference kind '{head}' in {expr}")),
    };
    for x in &xs[1..] {
        if let Some(name) = x.atom() {
            if !valid_name(name) {
                return Err(format!("invalid module name '{name}' in {expr}"));
            }
            path.push(name.to_string());
        } else if xs.len() == 2 && matches!(head, "parent" | "..") {
            path = reference(x, &path)?;
        } else {
            return Err(format!("expected a module name in {expr}"));
        }
    }
    Ok(path)
}

fn decode(forms: &[Expr], scope: &[String]) -> Result<Vec<Form>, String> {
    forms
        .iter()
        .map(|form| {
            decode_form(form, scope).map_err(|e| format!("in scope '{}': {e}", display(scope)))
        })
        .collect()
}

fn decode_form(form: &Expr, scope: &[String]) -> Result<Form, String> {
    let xs = form
        .list()
        .ok_or_else(|| format!("expected a form, got {form}"))?;
    match form.head() {
        Some("module") => {
            let name = xs
                .get(1)
                .and_then(Expr::atom)
                .ok_or("module requires a name")?;
            if !valid_name(name) {
                return Err(format!("invalid module name '{name}'"));
            }
            let mut module = Module {
                name: name.into(),
                files: Vec::new(),
                dependencies: Vec::new(),
            };
            for option in &xs[2..] {
                let args = option
                    .list()
                    .ok_or_else(|| format!("invalid module option {option}"))?;
                match option.head() {
                    Some("depends") => {
                        for r in &args[1..] {
                            let target = reference(r, scope)?;
                            if !module.dependencies.contains(&target) {
                                module.dependencies.push(target);
                            }
                        }
                    }
                    Some("file") if args.len() == 2 => {
                        if let Expr::String(file) = &args[1] {
                            module.files.push(file.clone());
                        } else {
                            return Err(format!("unsupported or malformed module option {option}"));
                        }
                    }
                    _ => return Err(format!("unsupported or malformed module option {option}")),
                }
            }
            Ok(Form::Module(module))
        }
        Some("within") => {
            let target = reference(xs.get(1).ok_or("within requires a reference")?, scope)?;
            Ok(Form::Within {
                forms: decode(&xs[2..], &target)?,
                scope: target,
            })
        }
        Some("publish") => {
            let mut publication = Publication {
                audience: None,
                targets: Vec::new(),
            };
            for item in &xs[1..] {
                match item.head() {
                    Some("within") => {
                        let args = item.list().unwrap();
                        if args.len() != 2 || publication.audience.is_some() {
                            return Err(format!("malformed publication audience {item}"));
                        }
                        publication.audience = Some(reference(&args[1], scope)?);
                    }
                    Some("from") => {
                        let args = item.list().unwrap();
                        if args.len() < 2 {
                            return Err("from requires a module reference".into());
                        }
                        for r in &args[1..] {
                            publication.targets.push(PublicationTarget {
                                path: reference(r, scope)?,
                                kind: PublicationKind::Context,
                            });
                        }
                    }
                    _ => publication.targets.push(PublicationTarget {
                        path: reference(item, scope)?,
                        kind: PublicationKind::Named,
                    }),
                }
            }
            if publication.targets.is_empty() {
                return Err("publish requires at least one module reference".into());
            }
            Ok(Form::Publish(publication))
        }
        _ => Err(format!("unknown or empty form {form}")),
    }
}

fn absolute(path: &[String]) -> Expr {
    Expr::List(
        std::iter::once(Expr::Atom("/".into()))
            .chain(path.iter().cloned().map(Expr::Atom))
            .collect(),
    )
}

/// Emit canonical syntax: absolute references, normalized options, no comments.
pub fn combine(forms: &[Form]) -> String {
    let mut output = String::from("(vys-project\n");
    for form in forms {
        format_form(form, 1, &mut output);
    }
    output.push_str(")\n");
    output
}

fn format_form(form: &Form, indent: usize, output: &mut String) {
    let pad = "  ".repeat(indent);
    match form {
        Form::Module(module) => {
            let mut xs = vec![Expr::Atom("module".into()), Expr::Atom(module.name.clone())];
            for file in &module.files {
                xs.push(Expr::List(vec![
                    Expr::Atom("file".into()),
                    Expr::String(file.clone()),
                ]));
            }
            if !module.dependencies.is_empty() {
                xs.push(Expr::List(
                    std::iter::once(Expr::Atom("depends".into()))
                        .chain(module.dependencies.iter().map(|p| absolute(p)))
                        .collect(),
                ));
            }
            output.push_str(&format!("{pad}{}\n", Expr::List(xs)));
        }
        Form::Within { scope, forms } => {
            output.push_str(&format!("{pad}(within {}\n", absolute(scope)));
            for form in forms {
                format_form(form, indent + 1, output);
            }
            output.push_str(&format!("{pad})\n"));
        }
        Form::Publish(publication) => {
            let mut xs = vec![Expr::Atom("publish".into())];
            if let Some(audience) = &publication.audience {
                xs.push(Expr::List(vec![
                    Expr::Atom("within".into()),
                    absolute(audience),
                ]));
            }
            for target in &publication.targets {
                let reference = absolute(&target.path);
                xs.push(match target.kind {
                    PublicationKind::Named => reference,
                    PublicationKind::Context => {
                        Expr::List(vec![Expr::Atom("from".into()), reference])
                    }
                });
            }
            output.push_str(&format!("{pad}{}\n", Expr::List(xs)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_and_duplicate_dependencies_lower_to_the_same_data() {
        let short = parse_project("(module api (depends (. db) (/ db)))", "test").unwrap();
        let long = parse_project("(module api (depends (project db)))", "test").unwrap();
        assert_eq!(short, long);
        let Form::Module(module) = &short[0] else {
            panic!("expected module")
        };
        assert_eq!(module.dependencies, vec![vec!["db".to_string()]]);
    }

    #[test]
    fn canonical_round_trip_preserves_files_audiences_and_publication_kinds() {
        let forms = parse_project(
            r#"
            (module api)
            (within (. api)
                (module types (file "./types.rs") (file "./types\n\"\\.rs"))
                (module services (depends (. types)))
                (publish (within (..)) (. types) (from (. services))))
        "#,
            "test",
        )
        .unwrap();
        let combined = combine(&forms);
        assert_eq!(parse_combined(&combined, "test").unwrap(), forms);
        assert!(combined.contains("(depends (/ api types))"));
        assert!(combined.contains("(within (/))"));
    }

    #[test]
    fn module_lowering_resolves_against_its_source_scope() {
        let scope = vec!["api".into(), "serde".into()];
        let forms = parse_module(
            "(module item (file \"./item.rs\") (depends (parent (parent db)) (. local)))",
            "module.vys",
            &scope,
        )
        .unwrap();
        let Form::Module(module) = &forms[0] else {
            panic!("expected module")
        };
        assert_eq!(module.files, vec!["./item.rs"]);
        assert_eq!(
            module.dependencies,
            vec![vec!["db"], vec!["api", "serde", "local"]]
        );
    }
}
