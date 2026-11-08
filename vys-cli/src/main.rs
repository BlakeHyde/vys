mod discovery;
mod forms;
mod project;
mod syntax;

use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};
use std::io::{self, Read, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "vys",
    version,
    about = "Collect, validate, and visualize vys module scopes"
)]
struct Cli {
    /// Project directory containing project.vys (otherwise search upward from cwd)
    #[arg(long, global = true, value_name = "DIR")]
    project_root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Collect reachable module definitions into a validated vys-project form
    Combine,
    /// Emit a Graphviz DOT dependency graph, clustered by module scope
    Graph {
        /// Read a combined vys-project form from standard input, without filesystem discovery
        #[arg(long, conflicts_with = "project_root")]
        stdin: bool,
    },
}

fn run(cli: Cli) -> Result<String, String> {
    if matches!(cli.command, Command::Graph { stdin: true }) {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| format!("stdin: {e}"))?;
        let forms = syntax::parse_combined(&input, "<stdin>")?;
        return Ok(project::Project::from_forms(&forms)?.dot());
    }
    let root = match cli.project_root {
        Some(path) => path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?,
        None => discovery::find_root(
            &std::env::current_dir().map_err(|e| format!("current directory: {e}"))?,
        )?,
    };
    let result = discovery::discover(&root, |path, scope| {
        let input =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let source = path.display().to_string();
        if scope.is_empty() {
            syntax::parse_project(&input, &source)
        } else {
            syntax::parse_module(&input, &source, scope)
        }
    })?;
    for orphan in &result.orphans {
        eprintln!("{}", discovery::orphan_warning(orphan, &root));
    }
    Ok(match cli.command {
        Command::Combine => syntax::combine(&result.forms),
        Command::Graph { .. } => result.project.dot(),
    })
}

fn main() {
    let cli = Cli::parse();
    // Clap does not check subcommand conflicts against a global option supplied
    // before the subcommand, so check that ordering explicitly as well.
    if cli.project_root.is_some() && matches!(cli.command, Command::Graph { stdin: true }) {
        Cli::command()
            .error(
                ErrorKind::ArgumentConflict,
                "--stdin cannot be used with --project-root",
            )
            .exit();
    }
    match run(cli) {
        Ok(output) => {
            if let Err(e) = io::stdout().lock().write_all(output.as_bytes())
                && e.kind() != io::ErrorKind::BrokenPipe
            {
                eprintln!("error: stdout: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
