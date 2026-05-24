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
    if let Some(schema) = command.schema {
        return Err(format!(
            "gelite repl does not support schema files yet: {}",
            schema.display()
        ));
    }

    if let Some(database) = command.database {
        return Err(format!(
            "gelite repl does not support database files yet: {}",
            database.display()
        ));
    }

    let query = if command.query.is_empty() {
        None
    } else {
        Some(command.query.join(" "))
    };

    repl::run(repl::ReplOptions {
        debug: command.debug,
        query,
    })
    .map_err(|()| "gelite repl failed".to_string())
}
