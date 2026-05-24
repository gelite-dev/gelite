fn main() {
    let args = ReplArgs::parse();
    let options = repl::ReplOptions {
        debug: args.debug,
        query: args.query,
    };

    if repl::run(options).is_err() {
        std::process::exit(1);
    }
}

struct ReplArgs {
    debug: bool,
    query: Option<String>,
}

impl ReplArgs {
    fn parse() -> Self {
        let mut debug = false;
        let mut query_parts = Vec::new();

        for arg in std::env::args().skip(1) {
            if arg == "--debug" {
                debug = true;
            } else {
                query_parts.push(arg);
            }
        }

        let query = if query_parts.is_empty() {
            None
        } else {
            Some(query_parts.join(" "))
        };

        Self { debug, query }
    }
}
