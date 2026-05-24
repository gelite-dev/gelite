use query_parser::parse_select;
use rustyline::{DefaultEditor, error::ReadlineError};
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};

const DEFAULT_QUERY: &str = r#"select Post { title, author: { name } } filter .title = "Hello" order by .title desc limit 10 offset 0"#;

pub struct ReplOptions {
    pub debug: bool,
    pub query: Option<String>,
}

pub fn run(options: ReplOptions) -> Result<(), ()> {
    let catalog = build_development_schema();

    run_with_catalog(&catalog, options)
}

pub fn run_with_catalog(catalog: &SchemaCatalog, options: ReplOptions) -> Result<(), ()> {
    match options.query {
        Some(query_text) => inspect_query(catalog, &query_text, options.debug),
        None => run_repl(catalog, options.debug),
    }
}

fn run_repl(catalog: &SchemaCatalog, debug: bool) -> Result<(), ()> {
    println!("gelite repl");
    println!("Type a select query to render SQL, or :quit / :exit to leave.");
    println!("Press Enter on an empty line to run the default query.");
    println!("Use balanced braces for multiline input.");
    println!("Press Ctrl-C twice in a row to leave.");
    if debug {
        println!("Debug output is enabled.");
    }

    let mut editor = DefaultEditor::new().map_err(|error| {
        eprintln!("failed to initialize line editor: {error}");
    })?;
    let mut pending = String::new();
    let mut interrupt_count = 0;

    loop {
        let prompt = if pending.is_empty() {
            "gelite> "
        } else {
            "   ...> "
        };

        match editor.readline(prompt) {
            Ok(line) => {
                interrupt_count = 0;
                let trimmed = line.trim();

                if pending.is_empty() && is_exit_command(trimmed) {
                    break;
                }

                if pending.is_empty() && trimmed.is_empty() {
                    let _ = inspect_query(catalog, DEFAULT_QUERY, debug);
                    continue;
                }

                if !pending.is_empty() {
                    pending.push('\n');
                }
                pending.push_str(&line);

                if needs_more_input(&pending) {
                    continue;
                }

                let query_text = pending.trim().to_string();
                pending.clear();

                if !query_text.is_empty() {
                    let _ = editor.add_history_entry(query_text.as_str());
                    let _ = inspect_query(catalog, &query_text, debug);
                }
            }
            Err(ReadlineError::Interrupted) => {
                pending.clear();
                interrupt_count += 1;

                if interrupt_count >= 2 {
                    break;
                }

                println!("input cancelled. Press Ctrl-C again to leave.");
            }
            Err(ReadlineError::Eof) => break,
            Err(error) => {
                eprintln!("failed to read input: {error}");
                return Err(());
            }
        }
    }

    Ok(())
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

fn inspect_query(catalog: &SchemaCatalog, query_text: &str, debug: bool) -> Result<(), ()> {
    let query = match parse_select(query_text) {
        Ok(query) => query,
        Err(error) => {
            eprintln!("failed to parse query: {error:#?}");
            return Err(());
        }
    };

    if debug {
        println!("Query:\n{query_text}");
        println!("Query AST:\n{query:#?}");
    }

    match query_resolver::resolve_select(catalog, &query) {
        Ok(resolved) => {
            let plan = sqlite_query_plan::plan_select(&resolved);
            let statement = sqlite_query_sqlgen::render_select(&plan);

            if debug {
                println!("Resolved IR:\n{resolved:#?}");
                println!(
                    "SQLite Plan:\n  root: {} as {}\n  selected values:",
                    plan.root_source().table_name(),
                    plan.root_source().alias()
                );
                for value in plan.selected_values() {
                    println!(
                        "    {}.{} -> {}",
                        value.source_alias(),
                        value.column_name(),
                        value.output_name()
                    );
                }
                println!("  joins:");
                for join in plan.joins() {
                    let on = join.on();
                    let join_kind = match join.kind() {
                        sqlite_query_plan::SQLiteJoinKind::Inner => "inner join",
                        sqlite_query_plan::SQLiteJoinKind::Left => "left join",
                    };
                    println!(
                        "    {} {} as {} on {}.{} = {}.{}",
                        join_kind,
                        join.target_table(),
                        join.target_alias(),
                        on.left_alias(),
                        on.left_column(),
                        on.right_alias(),
                        on.right_column()
                    );
                }
                println!("SQL:\n{}", statement.sql());
                println!("Bind values:\n{:#?}", statement.bind_values());
            } else {
                println!("{}", statement.sql());
            }
            Ok(())
        }
        Err(error) => {
            eprintln!("failed to resolve query: {error:#?}");
            Err(())
        }
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
