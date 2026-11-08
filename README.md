# vys

A language-neutral module and scope model for polyglot repositories, with explicit
publication and dependency boundaries for agent context, verification, and
sandboxing.

The Rust CLI currently collects and validates module definitions and generates
Graphviz dependency graphs. Materialization and sandboxing are planned.

- [Design and intent](design/000_intent.md)
- [CLI design](design/001_cli.md)
- [CLI usage and development](vys-cli/README.md)

## License

Copyright (C) 2026 Blake Hyde.

This project is licensed under the GNU General Public License, version 3 only
(SPDX: `GPL-3.0-only`). See [LICENSE](LICENSE) for the full terms. It is provided
without warranty.
