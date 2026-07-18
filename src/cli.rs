use crate::config::FormatterChoice;
use clap::Parser;
use std::path::PathBuf;

/// The pedantic Nix formatter: performs additional formatting in
/// compliance with your base formatter of choice.
#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    /// Files to format in place. With no files (or `-`), reads stdin and
    /// writes the result to stdout.
    pub files: Vec<PathBuf>,

    /// Path to a config file (default: search for pedantix.toml or
    /// .pedantix.toml upwards from each file's directory, stopping at the
    /// git repository root — or immediately when there is no repository —
    /// then fall back to $XDG_CONFIG_HOME/pedantix/pedantix.toml).
    #[arg(long, short)]
    pub config: Option<PathBuf>,

    /// Complete configuration as an inline TOML document; disables config
    /// file loading and discovery. Example: --config-toml 'formatter = "off"'
    #[arg(long, value_name = "TOML", conflicts_with = "config")]
    pub config_toml: Option<String>,

    /// Set a single configuration value on top of the loaded config, using
    /// the same keys as pedantix.toml. Repeatable. Values are parsed as
    /// TOML. Examples: --set lets.sort=true --set 'args.first=["self"]'
    #[arg(long = "set", value_name = "KEY.PATH=VALUE")]
    pub set: Vec<String>,

    /// Execute a `formatter-command` found in an auto-discovered config
    /// file. Without this flag, only configs you name explicitly
    /// (--config, --config-toml, --set, the global XDG config) may
    /// specify a command to run; discovered pedantix.toml files are
    /// limited to the built-in `formatter` choices.
    #[arg(long)]
    pub allow_formatter_command: bool,

    /// Don't write anything; exit 1 if any file would be reformatted.
    #[arg(long)]
    pub check: bool,

    /// Override the configured base formatter.
    #[arg(long)]
    pub formatter: Option<FormatterChoice>,

    /// Path the stdin content notionally comes from; used for config
    /// discovery in stdin mode.
    #[arg(long)]
    pub stdin_filepath: Option<PathBuf>,
}
