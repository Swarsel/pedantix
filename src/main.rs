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

struct LoadedConfig {
    table: toml::Table,
    origin: String,
    dir: Option<PathBuf>,
}

fn base_dir() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    Some(cwd.canonicalize().unwrap_or(cwd))
}

fn load_config(cli: &Cli, dir: &Path, warned: &mut HashSet<String>) -> Result<LoadedConfig> {
    let (text, origin, config_dir, discovered) = if let Some(inline) = &cli.config_toml {
        (
            inline.clone(),
            "--config-toml".to_string(),
            base_dir(),
            false,
        )
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
                let config_dir = path
                    .canonicalize()
                    .unwrap_or_else(|_| path.clone())
                    .parent()
                    .map(Path::to_path_buf);
                (text, path.display().to_string(), config_dir, discovered)
            }
            None => (
                String::new(),
                "default config".to_string(),
                base_dir(),
                false,
            ),
        }
    };
    let mut table: toml::Table = text
        .parse()
        .with_context(|| format!("invalid TOML in {origin}"))?;
    if discovered
        && pedantix::config::sets_formatter_command(&table)
        && !formatter_command_allowed(cli)
    {
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
    Ok(LoadedConfig {
        table,
        origin,
        dir: config_dir,
    })
}

fn match_path(file: &Path, config_dir: Option<&Path>) -> String {
    let canonical = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let relative = config_dir
        .and_then(|dir| canonical.strip_prefix(dir).ok())
        .unwrap_or(&canonical);
    relative.to_string_lossy().into_owned()
}

fn config_for_file(cli: &Cli, loaded: &LoadedConfig, file: Option<&Path>) -> Result<Config> {
    let path = file.map(|f| match_path(f, loaded.dir.as_deref()));
    let mut cfg = Config::from_table_for_file(loaded.table.clone(), path.as_deref(), &cli.set)
        .with_context(|| {
            format!(
                "invalid configuration ({} plus --set options)",
                loaded.origin
            )
        })?;
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
        let loaded = load_config(cli, &dir, &mut HashSet::new())?;
        let cfg = config_for_file(cli, &loaded, cli.stdin_filepath.as_deref())?;
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
    let mut configs: HashMap<PathBuf, LoadedConfig> = HashMap::new();
    for file in &cli.files {
        let input = std::fs::read_to_string(file)
            .with_context(|| format!("cannot read {}", file.display()))?;
        let dir = file
            .canonicalize()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let loaded = match configs.entry(dir) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let loaded = load_config(cli, entry.key(), &mut warned)?;
                entry.insert(loaded)
            }
        };
        let cfg = config_for_file(cli, loaded, Some(file))?;
        let output = pipeline::process_file(&input, &cfg, Some(file))
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
