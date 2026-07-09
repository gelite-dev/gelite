use query_parser::parse_select;
use rustyline::{DefaultEditor, error::ReadlineError};
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};
use sqlite_query_sqlgen::SQLiteSelectStatement;
use sqlite_runner::{SQLiteCellValue, SQLiteQueryResult};

pub struct ReplOptions {
    pub debug: bool,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplError;

type QueryExecutor<'a> =
    dyn FnMut(&SQLiteSelectStatement) -> Result<SQLiteQueryResult, String> + 'a;

pub fn run(options: ReplOptions) -> Result<(), ReplError> {
    let catalog = build_development_schema();

    run_with_catalog(&catalog, options)
}

pub fn run_with_catalog(catalog: &SchemaCatalog, options: ReplOptions) -> Result<(), ReplError> {
    let mut runtime = ReplRuntime { executor: None };

    runtime.run(catalog, options)
}

pub fn run_with_executor(
    catalog: &SchemaCatalog,
    options: ReplOptions,
    executor: &mut QueryExecutor<'_>,
) -> Result<(), ReplError> {
    let mut runtime = ReplRuntime {
        executor: Some(executor),
    };

    runtime.run(catalog, options)
}

struct ReplRuntime<'a> {
    executor: Option<&'a mut QueryExecutor<'a>>,
}

enum ReplLoopAction {
    Continue,
    Break,
}

impl ReplRuntime<'_> {
    fn run(&mut self, catalog: &SchemaCatalog, options: ReplOptions) -> Result<(), ReplError> {
        match options.query {
            Some(query_text) => self.inspect_query(catalog, &query_text, options.debug),
            None => self.run_repl(catalog, options.debug),
        }
    }

    fn run_repl(&mut self, catalog: &SchemaCatalog, debug: bool) -> Result<(), ReplError> {
        run_repl(catalog, debug, self)
    }

    fn inspect_query(
        &mut self,
        catalog: &SchemaCatalog,
        query_text: &str,
        debug: bool,
    ) -> Result<(), ReplError> {
        let statement = compile_query(catalog, query_text, debug)?;

        match self.executor.as_deref_mut() {
            Some(executor) => {
                let result = executor(&statement).map_err(|error| {
                    eprintln!("failed to execute query: {error}");
                    ReplError
                })?;
                print_query_result(&result);
            }
            None => println!("{}", statement.sql()),
        }

        Ok(())
    }
}

fn run_repl(
    catalog: &SchemaCatalog,
    debug: bool,
    runtime: &mut ReplRuntime<'_>,
) -> Result<(), ReplError> {
    println!("gelite repl");
    println!("Type a select query, or :quit / :exit to leave.");
    println!("Use balanced braces for multiline input.");
    println!("Press Ctrl-C twice in a row to leave.");
    if debug {
        println!("Debug output is enabled.");
    }

    let mut editor = DefaultEditor::new().map_err(|error| {
        eprintln!("failed to initialize line editor: {error}");
        ReplError
    })?;
    let mut pending = String::new();
    let mut interrupt_count = 0;

    loop {
        let prompt = repl_prompt(&pending);

        match editor.readline(prompt) {
            Ok(line) => match handle_repl_line(
                catalog,
                debug,
                runtime,
                &mut editor,
                &mut pending,
                &mut interrupt_count,
                line,
            )? {
                ReplLoopAction::Continue => {}
                ReplLoopAction::Break => break,
            },
            Err(error) => {
                match handle_repl_read_error(error, &mut pending, &mut interrupt_count)? {
                    ReplLoopAction::Continue => {}
                    ReplLoopAction::Break => break,
                }
            }
        }
    }

    Ok(())
}

fn repl_prompt(pending: &str) -> &'static str {
    if pending.is_empty() {
        "gelite> "
    } else {
        "   ...> "
    }
}

fn handle_repl_line(
    catalog: &SchemaCatalog,
    debug: bool,
    runtime: &mut ReplRuntime<'_>,
    editor: &mut DefaultEditor,
    pending: &mut String,
    interrupt_count: &mut i32,
    line: String,
) -> Result<ReplLoopAction, ReplError> {
    *interrupt_count = 0;

    if pending.is_empty() && is_exit_command(line.trim()) {
        return Ok(ReplLoopAction::Break);
    }

    append_pending_line(pending, &line);

    if needs_more_input(pending) {
        return Ok(ReplLoopAction::Continue);
    }

    let query_text = pending.trim().to_string();
    pending.clear();

    if !query_text.is_empty() {
        let _ = editor.add_history_entry(query_text.as_str());
        let _ = runtime.inspect_query(catalog, &query_text, debug);
    }

    Ok(ReplLoopAction::Continue)
}

fn append_pending_line(pending: &mut String, line: &str) {
    if !pending.is_empty() {
        pending.push('\n');
    }
    pending.push_str(line);
}

fn handle_repl_read_error(
    error: ReadlineError,
    pending: &mut String,
    interrupt_count: &mut i32,
) -> Result<ReplLoopAction, ReplError> {
    match error {
        ReadlineError::Interrupted => handle_repl_interrupt(pending, interrupt_count),
        ReadlineError::Eof => Ok(ReplLoopAction::Break),
        error => {
            eprintln!("failed to read input: {error}");
            Err(ReplError)
        }
    }
}

fn handle_repl_interrupt(
    pending: &mut String,
    interrupt_count: &mut i32,
) -> Result<ReplLoopAction, ReplError> {
    pending.clear();
    *interrupt_count += 1;

    if *interrupt_count >= 2 {
        return Ok(ReplLoopAction::Break);
    }

    println!("input cancelled. Press Ctrl-C again to leave.");
    Ok(ReplLoopAction::Continue)
}

fn is_exit_command(input: &str) -> bool {
    matches!(input, ":quit" | ":q" | ":exit" | "quit" | "exit")
}

fn needs_more_input(input: &str) -> bool {
    brace_balance(input) > 0
}

fn brace_balance(input: &str) -> i32 {
    let mut balance = 0;
    let mut in_string = false;

    for ch in input.chars() {
        match ch {
            '"' => in_string = !in_string,
            '{' if !in_string => balance += 1,
            '}' if !in_string => balance -= 1,
            _ => {}
        }
    }

    balance
}

fn compile_query(
    catalog: &SchemaCatalog,
    query_text: &str,
    debug: bool,
) -> Result<SQLiteSelectStatement, ReplError> {
    let query = match parse_select(query_text) {
        Ok(query) => query,
        Err(error) => {
            eprintln!("failed to parse query: {error:#?}");
            return Err(ReplError);
        }
    };

    match query_resolver::resolve_select(catalog, &query) {
        Ok(resolved) => {
            let plan = sqlite_query_plan::plan_select(&resolved);
            let statement = sqlite_query_sqlgen::render_select(&plan);

            if debug {
                println!("SQL:\n{}", statement.sql());
                println!("Bind values: {:?}", statement.bind_values());
            }

            Ok(statement)
        }
        Err(error) => {
            eprintln!("failed to resolve query: {error:#?}");
            Err(ReplError)
        }
    }
}

fn print_query_result(result: &SQLiteQueryResult) {
    if !result.columns().is_empty() {
        println!("{}", result.columns().join("\t"));
    }

    for row in result.rows() {
        let values = row
            .iter()
            .map(format_cell_value)
            .collect::<Vec<_>>()
            .join("\t");
        println!("{values}");
    }

    if result.rows().is_empty() {
        println!("(0 rows)");
    }
}

fn format_cell_value(value: &SQLiteCellValue) -> String {
    match value {
        SQLiteCellValue::Integer(value) => value.to_string(),
        SQLiteCellValue::Real(value) => value.to_string(),
        SQLiteCellValue::Text(value) => value.clone(),
        SQLiteCellValue::Null => "NULL".to_string(),
    }
}

fn build_development_schema() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::new("author", "User", Cardinality::Required)),
            ],
        ),
    ])
    .expect("hardcoded development schema should be valid")
}

#[cfg(test)]
mod tests {
    use super::needs_more_input;

    #[test]
    fn multiline_input_continues_until_braces_are_balanced() {
        assert!(needs_more_input("select Post {"));
        assert!(needs_more_input("select Post {\n  author: { name }"));
        assert!(!needs_more_input("select Post {\n  author: { name }\n}"));
    }

    #[test]
    fn braces_inside_strings_do_not_start_multiline_input() {
        assert!(!needs_more_input(
            r#"select Post { title } filter .title = "{""#
        ));
    }
}
