use anyhow::{Context, Result, bail};
use clap::Parser;
use pedantix::cli::Cli;
use pedantix::config::Config;
use pedantix::pipeline;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
