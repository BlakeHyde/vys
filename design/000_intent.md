# vys

vys is to be a _meta-modularity_ structure for assisting with the scoping,
sandboxing, and verification of software in the post-LLM world without being
dependent upon a given implementation language, agent, build tool, sandboxing
mechanism, agentic harness, and so on.

It's based on the realization that these concepts and others are shared across
languages, but that expressing them in a polyglot environment is unnecessarily
limited by the constraints of each language and ecosystem.

For the canonical, motivating example, consider a monorepo with this structure:

```
/web - An API written in TypeScript.
/ios - A native mobile app written in Swift.
/api - The backend API, RESTful.
/db  - Database schema and migrations.
```

We might define each top-level subdirectory as a module in `project.vys`:

```scheme
(module web (depends (module api)))

(module ios (depends (module api)))

(module api (depends (module db)))

(module db)
```

Within each module, more specific `module` forms can be specified. For example,
in `/api/module.vys`:

```scheme
(module types)

(module commands (depends (module types) (project db)))

(module queries (depends (module types) (project db)))

(module serde (depends (module types)))

(module services
  (depends (module types)
           (module commands)
           (module queries)
           (module serde)))

(publish (from (module services)))
```

These determine the scope visibility of dependencies. For example, if a
submodule of `api` declares that it depends on `(project ios)`, that's a vys
verification error. Similarly if a submodule of `(api serde)` declares that it
depends on `(project db)` when it's defined in `/api/serde/module.vys`, that's
a vys verification error. A module _can_ declare relative to its ancestors
using `(parent (parent (parent ...)))`, but this is a code smell.

Shorthand notation: in "reference positions," "project," "module," and "parent"
have abbreviations of `/`, `.`, and `..` as derived from folder structures and
relative to the file where the form is being read.

- `(/ api serde)` would serve as an absolute reference to the serde module.
- `(. serde)` would reference it from the  `(/ api)` scope, as in the example
  file `/api/module.vys`.
- `(.. serde)` would refer to it from within the `(/ api services)` scope, as
  in the file `/api/services/module.vys`.

For example, the definition for `services` above could be written as:

```scheme
(module services (depends (. types) (. commands) (. queries) (. serde)))
```
The `publish` form appends to the "public context" provided by the module. In
our example, the `(/ api)` module is appending the public context published
by `(/ api services)`. That means that, as defined here, the `(/ web)` and
`(/ ios)` modules would have access to the public context of that nested
module. _Publication is explicit, never implicit._

In our example, the `api` module is publishing a "flat" interface, with no
public knowledge other than that provided by the `services` module. However,
we can also declare a submodule as part of our public interface. Consider the
structure defined for `api` above. Obviously, there is a "public" case,
for `ios` and `web`, as well as more detailed submodule structure that would
be relevant only _within the module_.

For example, let's imagine that we want to restrict types, commands, queries,
and so on by the type of entities they deal with. If we're making an exercise
tracker, for example, we might divide `types` in `/api/types/module.vys`:

```scheme
(module exercise-type (file "./exercise.ml") (file "./exercise.mli"))

(module session (file "./session.ml") (file "./session.mli"))

(publish (within (..)) (. exercise-type) (. session))
```

Since these individual submodules are published `(within (..))`---that is,
within `/api`---other modules can depend on them specifically.

```scheme
; /api/serde/module.vys
(module exercise-type
        (file "./exercise.ml")
        (depends (.. types exercise-type)))

(module session
        (file "./session.ml")
        (depends (.. types session)))
```

Ultimately, the purpose of this is to build a tree of scopes that are
accessible for a given agent's context. For example, an agent directed to
implement the services for recording exercise sessions would have write
access to `/api/services/session.ml`, but also _read_ access to the modules
that those services depend on; likely:

- `/api/commands/session.ml`
- `/api/queries/exercise.ml`
- `/api/queries/session.ml`
- `/api/serde/exercise.ml`
- `/api/serde/session.ml`
- `/api/types/exercise.ml`
- `/api/types/session.ml`

## Sandboxing Strategies

Every harness, corporate or bespoke, has its own methods for providing context
to running agents. However, vys needn't be opinionated. We can implement the
logic to find, parse, and visualize `*.vys` files without any enforcement.

Afterward, materialization of scopes to lists of files and directories is a
natural step. The use of this to provide configuration for sandboxing utilities
like bubblewrap or Docker is obvious, although potentially tedious.
