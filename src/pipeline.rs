use crate::config::Config;
use anyhow::{Context, Result};
use std::borrow::Cow;
use std::path::Path;

pub fn process(input: &str, cfg: &Config) -> Result<String> {
    process_file(input, cfg, None)
}

pub fn process_file(input: &str, cfg: &Config, path: Option<&Path>) -> Result<String> {
    let formatted = if cfg.format_before_sort {
        crate::base::run_base_formatter(cfg, input).context("base formatter (before sort)")?
    } else {
        input.to_string()
    };
    let merged = if cfg.attrs_may_merge() {
        Cow::Owned(crate::merge::merge_source(&formatted, cfg)?)
    } else {
        Cow::Borrowed(formatted.as_str())
    };
    let sorted = crate::sort::sort_source(&merged, cfg)?;
    let changed = sorted != formatted;
    if changed {
        crate::semantic::check_same_content(
            &formatted,
            &sorted,
            cfg.lists_may_sort(),
            cfg.attrs_may_merge(),
        )?;
    }
    let output = if cfg.format_after_sort && (changed || !cfg.format_before_sort) {
        crate::base::run_base_formatter(cfg, &sorted).context("base formatter (after sort)")?
    } else {
        sorted
    };
    let is_flake = path
        .and_then(Path::file_name)
        .is_some_and(|name| name == "flake.nix");
    crate::spacing::space_top_level(&output, cfg, is_flake)
}
