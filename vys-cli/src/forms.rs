//! Canonical vys data, independent of its textual representation.
//!
//! Reference aliases and relative paths are resolved by the syntax boundary.
//! Every reference below is an absolute logical path; file strings remain
//! relative to the file's scope, as supplied by the author.

pub type ModulePath = Vec<String>;

pub fn display(path: &[String]) -> String {
    if path.is_empty() {
        "/".into()
    } else {
        path.join("/")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Form {
    Module(Module),
    Within { scope: ModulePath, forms: Vec<Form> },
    Publish(Publication),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub files: Vec<String>,
    pub dependencies: Vec<ModulePath>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationKind {
    /// Expose a descendant module's name.
    Named,
    /// Forward public context without exposing the source module's name.
    Context,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationTarget {
    pub path: ModulePath,
    pub kind: PublicationKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    pub audience: Option<ModulePath>,
    pub targets: Vec<PublicationTarget>,
}
