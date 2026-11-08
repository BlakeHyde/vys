use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "vys-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn file(&self, name: &str, contents: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vys"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}
fn success(output: &Output) -> String {
    assert!(output.status.success(), "{}", text(&output.stderr));
    text(&output.stdout)
}
fn stdin_graph(input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vys"))
        .args(["graph", "--stdin"])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn invalid(forms: &str, expected: &str) {
    let output = stdin_graph(&format!("(vys-project {forms})"));
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        text(&output.stdout)
    );
    assert!(
        text(&output.stderr).contains(expected),
        "{}",
        text(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn discovery_round_trip_and_clusters() {
    let f = Fixture::new();
    f.file(
        "project.vys",
        "(module web (depends (. api))) (module api (depends (. db))) (module db)",
    );
    f.file(
        "api/module.vys",
        "(module types) (module services (depends (. types) (/ db))) (publish (from (. services)))",
    );
    f.file(
        "api/types/module.vys",
        "(module session (file \"./session.rs\")) (publish (within (..)) (. session))",
    );
    let combined = success(&f.run(&["combine"]));
    assert!(combined.contains("(within (/ api types)"));
    let dot = success(&f.run(&["graph"]));
    assert_eq!(success(&stdin_graph(&combined)), dot);
    assert!(dot.contains("subgraph \"cluster_api\""));
    assert!(dot.contains("\"api/services\" -> \"db\""));
    assert!(dot.contains("\"web\" -> \"api\""));
    assert_eq!(success(&f.run(&["combine"])), combined);
}

#[test]
fn orphans_are_warned_about_without_parsing_or_inclusion() {
    let f = Fixture::new();
    f.file("project.vys", "(module api)");
    f.file("api/module.vys", "(module inner)");
    f.file("orphan/module.vys", "invalid (");
    f.file("orphan/child/module.vys", "(module omitted)");
    f.file("module.vys", "(module wrong-root)");
    let output = f.run(&["combine"]);
    let combined = success(&output);
    assert!(!combined.contains("omitted"));
    assert!(!combined.contains("wrong-root"));
    assert_eq!(
        text(&output.stderr)
            .matches("warning: orphaned module file")
            .count(),
        3
    );
    assert!(text(&output.stderr).contains("orphan/child/module.vys"));
    assert!(text(&output.stderr).contains("module.vys"));
}

#[test]
fn upward_and_explicit_root_discovery() {
    let f = Fixture::new();
    f.file("project.vys", "(vys-project (module api))");
    f.file("api/nested/.keep", "");
    let output = Command::new(env!("CARGO_BIN_EXE_vys"))
        .current_dir(f.0.join("api/nested"))
        .arg("combine")
        .output()
        .unwrap();
    assert!(success(&output).contains("(module api)"));
    for args in [
        vec!["--project-root", f.0.to_str().unwrap(), "graph"],
        vec!["graph", "--project-root", f.0.to_str().unwrap()],
    ] {
        assert!(success(&f.run(&args)).contains("digraph vys"));
    }
}

#[test]
fn parent_dependencies_bound_child_access() {
    success(&stdin_graph(
        "(vys-project (module api (depends (. db))) (module db) (within (/ api) (module command (depends (/ db)))))",
    ));
    invalid(
        "(module api) (module db) (within (/ api) (module command (depends (/ db))))",
        "cannot access 'db'",
    );
    invalid(
        "(module api (depends (. db))) (module db) (within (/ api) (module serde) (within (. serde) (module child (depends (/ db)))))",
        "cannot access 'db'",
    );
}

#[test]
fn publication_is_explicit_and_audience_scoped() {
    let base = "(module api) (within (/ api) (module types) (module serde (depends (. types item))) (within (. types) (module item) (publish (within (..)) (. item))))";
    success(&stdin_graph(&format!("(vys-project {base})")));
    invalid(
        &format!("{base} (module web (depends (/ api types item)))"),
        "cannot access 'api/types/item'",
    );
    success(&stdin_graph(
        "(vys-project (module api) (module web (depends (/ api types item))) (within (/ api) (module types) (publish (. types)) (within (. types) (module item) (publish (. item)))))",
    ));
    invalid(
        "(module api) (module web (depends (/ api services))) (within (/ api) (module services) (publish (from (. services))))",
        "cannot access 'api/services'",
    );
    invalid(
        "(module api) (within (/ api) (module types) (module serde (depends (. types item))) (within (. types) (module item)))",
        "cannot access 'api/types/item'",
    );
}

#[test]
fn full_reference_names_and_nested_parents() {
    success(&stdin_graph(
        "(vys-project (module api (depends (module db))) (module db) (within (project api) (module types) (module serde (depends (module types) (project db))) (within (module serde) (module item (depends (parent (parent db)))))))",
    ));
}

#[test]
fn semantic_and_syntax_errors_are_reported() {
    invalid(
        "(module a (depends (. missing)))",
        "unknown module 'missing'",
    );
    invalid("(module a) (module a)", "duplicate module 'a'");
    invalid("(module a (depends (. a)))", "depends on itself");
    invalid(
        "(module a (depends (. b))) (module b (depends (. a)))",
        "dependency cycle:",
    );
    invalid(
        "(within (/ missing) (module a))",
        "undeclared module 'missing'",
    );
    invalid("(module ../escape)", "invalid module name");
    invalid("(module a (depends (.. x)))", "above the project root");
    invalid("(nonsense)", "unknown or empty form");
    invalid("(module a (file nope))", "malformed module option");
    invalid("(publish (. missing))", "publishes unknown module");
    invalid(
        "(module a) (within (. a) (publish (within (/ elsewhere)) (. a)))",
        "audience must be an ancestor",
    );
    let bad = stdin_graph("(vys-project\n (module a)");
    assert!(text(&bad.stderr).contains("<stdin>:2:"));
    assert!(bad.stdout.is_empty());
}

#[test]
fn invalid_reachable_file_fails_with_path() {
    let f = Fixture::new();
    f.file("project.vys", "(module api)");
    f.file("api/module.vys", "(module broken");
    let output = f.run(&["combine"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("api/module.vys:1:"));
}

#[test]
fn stdin_requires_wrapper_and_rejects_root_option() {
    assert!(!stdin_graph("(module a)").status.success());
    assert!(!stdin_graph("").status.success());
    let f = Fixture::new();
    let output = f.run(&["graph", "--stdin", "--project-root", "."]);
    assert_eq!(output.status.code(), Some(2));
    let output = f.run(&["--project-root", ".", "graph", "--stdin"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn missing_project_is_an_error() {
    let f = Fixture::new();
    let output = f.run(&["combine", "--project-root", "."]);
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("project.vys"));
}

#[cfg(unix)]
#[test]
fn symlink_directories_are_not_followed() {
    let f = Fixture::new();
    f.file("project.vys", "(module api)");
    std::os::unix::fs::symlink(&f.0, f.0.join("api")).unwrap();
    assert!(success(&f.run(&["combine"])).contains("(module api)"));
}
