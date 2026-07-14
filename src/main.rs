use anyhow::{Context, Result, bail};
use clap::Parser;
use pedantix::config::{Config, FormatterChoice};
use pedantix::pipeline;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The pedantic Nix formatter: performs additional formatting in
/// compliance with your base formatter of choice.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Files to format in place. With no files (or `-`), reads stdin and
    /// writes the result to stdout.
    files: Vec<PathBuf>,

    /// Path to a config file (default: search for pedantix.toml or
    /// .pedantix.toml upwards from each file's directory, stopping at the
    /// git repository root — or immediately when there is no repository —
    /// then fall back to $XDG_CONFIG_HOME/pedantix/pedantix.toml).
    #[arg(long, short)]
    config: Option<PathBuf>,

    /// Complete configuration as an inline TOML document; disables config
    /// file loading and discovery. Example: --config-toml 'formatter = "off"'
    #[arg(long, value_name = "TOML", conflicts_with = "config")]
    config_toml: Option<String>,

    /// Set a single configuration value on top of the loaded config, using
    /// the same keys as pedantix.toml. Repeatable. Values are parsed as
    /// TOML. Examples: --set lets.sort=true --set 'args.first=["self"]'
    #[arg(long = "set", value_name = "KEY.PATH=VALUE")]
    set: Vec<String>,

    /// Execute a `formatter-command` found in an auto-discovered config
    /// file. Without this flag, only configs you name explicitly
    /// (--config, --config-toml, --set, the global XDG config) may
    /// specify a command to run; discovered pedantix.toml files are
    /// limited to the built-in `formatter` choices.
    #[arg(long)]
    allow_formatter_command: bool,

    /// Don't write anything; exit 1 if any file would be reformatted.
    #[arg(long)]
    check: bool,

    /// Override the configured base formatter.
    #[arg(long)]
    formatter: Option<FormatterChoice>,

    /// Path the stdin content notionally comes from; used for config
    /// discovery in stdin mode.
    #[arg(long)]
    stdin_filepath: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("pedantix: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn formatter_command_allowed(cli: &Cli) -> bool {
    cli.allow_formatter_command
        || cli.formatter.is_some()
        || cli.set.iter().any(|a| {
            a.split_once('=')
                .is_some_and(|(k, _)| k.trim() == "formatter-command")
        })
}

fn load_config(cli: &Cli, dir: &Path, warned: &mut HashSet<String>) -> Result<Config> {
    let (text, origin, discovered) = if let Some(inline) = &cli.config_toml {
        (inline.clone(), "--config-toml".to_string(), false)
    } else {
        let (path, discovered) = match &cli.config {
            Some(path) => (Some(path.clone()), false),
            None => match Config::discover_path(dir) {
                Some(path) => (Some(path), true),
                None => (Config::fallback_path(), false),
            },
        };
        match path {
            Some(path) => {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("cannot read config file {}", path.display()))?;
                (text, path.display().to_string(), discovered)
            }
            None => (String::new(), "default config".to_string(), false),
        }
    };
    let mut table: toml::Table = text
        .parse()
        .with_context(|| format!("invalid TOML in {origin}"))?;
    if discovered && table.contains_key("formatter-command") && !formatter_command_allowed(cli) {
        bail!(
            "{origin} sets `formatter-command`, which is not executed from auto-discovered \
             config files; rerun with --allow-formatter-command if you trust this repository, \
             pass the file via --config, or override with --formatter"
        );
    }
    for assignment in &cli.set {
        pedantix::config::apply_set(&mut table, assignment)?;
    }
    for problem in pedantix::config::ignored_keys(&table) {
        let warning = format!("pedantix: warning: {origin}: {problem}");
        if warned.insert(warning.clone()) {
            eprintln!("{warning}");
        }
    }
    let mut cfg = Config::from_table(table)
        .with_context(|| format!("invalid configuration ({origin} plus --set options)"))?;
    if let Some(formatter) = cli.formatter {
        cfg.formatter = formatter;
        cfg.formatter_command = None;
    }
    Ok(cfg)
}

fn run(cli: &Cli) -> Result<ExitCode> {
    let stdin_mode =
        cli.files.is_empty() || (cli.files.len() == 1 && cli.files[0].as_os_str() == "-");
    if stdin_mode {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .context("reading stdin")?;
        let dir = cli
            .stdin_filepath
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let cfg = load_config(cli, &dir, &mut HashSet::new())?;
        let output = pipeline::process_file(&input, &cfg, cli.stdin_filepath.as_deref())?;
        if cli.check {
            return Ok(if output == input {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            });
        }
        std::io::stdout()
            .write_all(output.as_bytes())
            .context("writing stdout")?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut would_change = false;
    let mut warned = HashSet::new();
    let mut configs: HashMap<PathBuf, Config> = HashMap::new();
    for file in &cli.files {
        let input = std::fs::read_to_string(file)
            .with_context(|| format!("cannot read {}", file.display()))?;
        let dir = file
            .canonicalize()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let cfg = match configs.entry(dir) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let cfg = load_config(cli, entry.key(), &mut warned)?;
                entry.insert(cfg)
            }
        };
        let output = pipeline::process_file(&input, cfg, Some(file))
            .with_context(|| format!("while formatting {}", file.display()))?;
        if output != input {
            if cli.check {
                eprintln!("would reformat: {}", file.display());
                would_change = true;
            } else {
                std::fs::write(file, &output)
                    .with_context(|| format!("cannot write {}", file.display()))?;
            }
        }
    }
    Ok(if would_change {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}
