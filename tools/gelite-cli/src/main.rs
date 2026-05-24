use std::{fs, path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use gelite_commands::{SchemaPlanStatement, plan_schema};

#[derive(Debug, Parser)]
#[command(name = "gelite")]
#[command(about = "Gelite command-line tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Schema {
        #[command(subcommand)]
        command: SchemaCommand,
    },
    Repl(ReplCommand),
}

#[derive(Debug, Subcommand)]
enum SchemaCommand {
    Plan { schema_file: PathBuf },
}

#[derive(Debug, Args)]
struct ReplCommand {
    #[arg(long)]
    debug: bool,
    #[arg(long)]
    schema: Option<PathBuf>,
    #[arg(long)]
    database: Option<PathBuf>,
    #[arg(trailing_var_arg = true)]
    query: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Schema { command } => run_schema_command(command),
        Command::Repl(command) => run_repl_command(command),
    }
}

fn run_schema_command(command: SchemaCommand) -> Result<(), String> {
    match command {
        SchemaCommand::Plan { schema_file } => {
            let source = fs::read_to_string(&schema_file)
                .map_err(|error| format!("failed to read {}: {error}", schema_file.display()))?;
            let output = plan_schema(&source).map_err(|error| error.message().to_string())?;

            for statement in output.statements() {
                println!("{}", statement.sql());
                if let SchemaPlanStatement::Insert { values, .. } = statement {
                    println!("  binds: {values:?}");
                }
            }

            Ok(())
        }
    }
}

fn run_repl_command(command: ReplCommand) -> Result<(), String> {
    let catalog = match (command.schema, command.database) {
        (Some(_), Some(_)) => {
            return Err("gelite repl accepts either --schema or --database, not both".to_string());
        }
        (Some(schema), None) => {
            let source = fs::read_to_string(&schema)
                .map_err(|error| format!("failed to read {}: {error}", schema.display()))?;
            schema_parser::parse_schema(&source).map_err(|error| format!("{error:#?}"))?
        }
        (None, Some(database)) => {
            return Err(format!(
                "gelite repl cannot load catalogs from databases yet: {}. Use --schema <schema.geli> for compile-only query inspection.",
                database.display()
            ));
        }
        (None, None) => {
            return Err(
                "gelite repl needs a catalog. Pass --schema <schema.geli> for compile-only query inspection. Database catalog loading through --database <app.db> is not implemented yet."
                    .to_string(),
            );
        }
    };

    let query = if command.query.is_empty() {
        None
    } else {
        Some(command.query.join(" "))
    };

    repl::run_with_catalog(
        &catalog,
        repl::ReplOptions {
            debug: command.debug,
            query,
        },
    )
    .map_err(|()| "gelite repl failed".to_string())
}
