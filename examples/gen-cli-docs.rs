use clap::CommandFactory;
use pedantix::cli::Cli;

fn escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn main() {
    let cmd = Cli::command();

    let usage = cmd
        .clone()
        .render_usage()
        .to_string()
        .trim_start_matches("Usage: ")
        .trim()
        .to_string();

    println!("```console");
    println!("$ {usage}");
    println!("```");
    println!();

    let positionals: Vec<_> = cmd.get_positionals().collect();
    if !positionals.is_empty() {
        println!("### Arguments");
        println!();
        println!("| Argument | Description |");
        println!("|----------|-------------|");
        for arg in positionals {
            let name = arg
                .get_value_names()
                .and_then(|n| n.first())
                .map(|n| format!("`[{n}]...`"))
                .unwrap_or_else(|| format!("`{}`", arg.get_id()));
            println!("| {} | {} |", name, escape(&help_of(arg)));
        }
        println!();
    }

    println!("### Options");
    println!();
    println!("| Option | Description |");
    println!("|--------|-------------|");
    for arg in cmd.get_arguments().filter(|a| !a.is_positional()) {
        println!("| {} | {} |", flag_of(arg), escape(&help_of(arg)));
    }
}

fn takes_value(arg: &clap::Arg) -> bool {
    use clap::ArgAction;
    !matches!(
        arg.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count | ArgAction::Help
    )
}

fn help_of(arg: &clap::Arg) -> String {
    let mut help = arg
        .get_long_help()
        .or_else(|| arg.get_help())
        .map(|h| h.to_string())
        .unwrap_or_default();
    if takes_value(arg) {
        let values: Vec<_> = arg
            .get_possible_values()
            .iter()
            .map(|v| format!("`{}`", v.get_name()))
            .collect();
        if !values.is_empty() {
            help = format!("{help} Possible values: {}.", values.join(", "));
        }
    }
    help
}

fn flag_of(arg: &clap::Arg) -> String {
    let mut parts = Vec::new();
    if let Some(short) = arg.get_short() {
        parts.push(format!("-{short}"));
    }
    if let Some(long) = arg.get_long() {
        parts.push(format!("--{long}"));
    }
    let mut flag = format!("`{}`", parts.join(", "));
    if takes_value(arg)
        && let Some(name) = arg.get_value_names().and_then(|n| n.first())
    {
        flag = format!("`{} <{name}>`", parts.join(", "));
    }
    flag
}
