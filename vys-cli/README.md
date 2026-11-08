# vys CLI

A standalone Rust CLI for collecting, validating, and visualizing vys modules,
following `../design/`. `materialize` and `sandbox` are not implemented yet.

## Build and run

```sh
cargo build --manifest-path vys-cli/Cargo.toml
cargo install --path vys-cli

vys combine
vys graph > modules.dot
dot -Tsvg modules.dot -o modules.svg
vys combine | vys graph --stdin
vys --project-root /path/to/repository combine
```

The default root is the nearest directory containing `project.vys`, searching
upward from the working directory. An explicit `--project-root` is used as-is
and must contain `project.vys`. `graph --stdin` requires exactly one combined
`(vys-project ...)` form, does not inspect the filesystem, and cannot be combined
with `--project-root`.

Successful output goes to stdout; warnings and errors go to stderr. Validation
errors exit with status 1, argument errors with status 2. Graphviz itself is not
needed to emit DOT, only to render it.

## Discovery and combination

`project.vys` accepts bare forms or a single `(vys-project ...)` wrapper.
A module declaration establishes a directory-like logical path: `(module api)`
at the root declares `api`; declarations in `api/module.vys` declare children
such as `api/services`. File-backed modules may exist without a directory.

Discovery scans for `module.vys` files and loads only those whose containing
module has been declared by the project or another reachable module file.
Undeclared files (including a root-level `module.vys`) are reported individually
as orphans and omitted, without parsing their contents. Discovery does not
follow symlinks, and skips `.git` and `.jj` metadata directories.

`combine` validates the project and emits a deterministic `(vys-project ...)`
form, wrapping each loaded module file in `(within (/ path ...) ...)`. References
are normalized to absolute `(/ ...)` paths and file strings are preserved;
comments and original formatting are discarded.

## Forms and validation

Supported forms:

```scheme
(module name
  (file "./source.rs")
  (depends (. sibling) (/ external)))

(within (. name)
  (module child))

(publish (. public-child))
(publish (within (..)) (. internally-public-child))
(publish (from (. implementation)))
```

`file` is optional and repeatable; paths are not checked for existence. References support `project`/`/`, `module`/`.`, and
`parent`/`..`, with multiple path components and nested parent references, e.g.
`(parent (parent db))`. References in a declaration are relative to its
**containing scope**, not the module being declared.

Visibility is checked as follows:

- A scope can refer to its immediate children (so sibling dependencies in their
  declarations work directly).
- Children may use dependencies explicitly granted to their containing module.
  Grants do not automatically propagate to grandchildren.
- Referring deeper into a module requires named publications along the path;
  a dependency does not grant access to private descendants.
- `(publish (. child))` exposes a descendant's name. `(within REFERENCE)` inside
  `publish` restricts the audience to an ancestor scope and its descendants.
- `(publish (from REFERENCE))` forwards public context, not the source module's
  name. The source must exist and be accessible; file/context materialization
  is deferred.

Unknown references, duplicate declarations, inaccessible dependencies or
publications, invalid scopes, self-dependencies, and dependency cycles are
errors. Module names must be single path components (no `/`, `\\`, `.` or `..`).
Quoted strings support `\"`, `\\`, `\n`, `\r`, and `\t`; semicolon comments run to
the end of the line. Syntax errors include source filename, line, and column;
semantic errors identify logical module paths.

## Graphs

DOT contains a node for every module, including modules with no dependencies.
A dependency edge points **from the dependent to its dependency**. Modules with
children form nested Graphviz clusters, with a node inside the cluster to keep
edges to the parent module explicit. Node identifiers are full logical paths;
labels use the last path component. Publications affect validation, but are not
drawn as dependency edges. Filesystem discovery and combined stdin input produce
equivalent graphs.

## Internal boundaries

| Module | Responsibility | Internal dependencies |
| --- | --- | --- |
| `forms` | Canonical Rust data: modules, files, absolute references, scoped blocks, and publications | None |
| `syntax` | Parse and format canonical forms; keep S-expressions private | `forms` |
| `project` | Build, validate, and graph the semantic model | `forms` |
| `discovery` | Scan paths and collect reachable forms through an injected loader | `forms`, `project` |
| `main` | Wire filesystem reading and syntax parsing into discovery; handle CLI I/O | All of the above |

The loader receives a file path and its logical scope and returns canonical forms.

## Tests

```sh
cargo test --manifest-path vys-cli/Cargo.toml
cargo clippy --manifest-path vys-cli/Cargo.toml --all-targets -- -D warnings
```
