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
    let restructures = cfg.attrs_may_merge() || cfg.attrs_may_flatten();
    let restyles = cfg.names_may_restyle();
    let restyled = if restyles {
        Cow::Owned(crate::names::restyle_source(&formatted, cfg)?)
    } else {
        Cow::Borrowed(formatted.as_str())
    };
    let merged = if restructures {
        Cow::Owned(crate::merge::merge_source(&restyled, cfg)?)
    } else {
        restyled
    };
    let sorted = crate::sort::sort_source(&merged, cfg)?;
    let changed = sorted != formatted;
    let formatted_after = if cfg.format_after_sort && (changed || !cfg.format_before_sort) {
        crate::base::run_base_formatter(cfg, &sorted).context("base formatter (after sort)")?
    } else {
        sorted
    };
    let is_flake = path
        .and_then(Path::file_name)
        .is_some_and(|name| name == "flake.nix");
    let output = crate::spacing::space_top_level(&formatted_after, cfg, is_flake)?;
    if output != input {
        crate::semantic::check_same_content(
            input,
            &output,
            cfg.lists_may_sort(),
            restructures,
            restyles,
        )?;
    }
    Ok(output)
}
