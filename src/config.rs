use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

fn default_true() -> bool {
    true
}

fn default_blank_lines_depth() -> usize {
    1
}

#[cfg(feature = "docs")]
fn default_formatter_doc() -> &'static str {
    "nixfmt"
}

#[cfg(feature = "docs")]
fn default_blank_lines_mode_doc() -> &'static str {
    "multiline"
}

#[cfg(feature = "docs")]
fn default_inherit_placement_doc() -> &'static str {
    "front"
}

#[cfg(feature = "docs")]
fn default_name_style_doc() -> &'static str {
    "preserve"
}

pub const DEFAULTED_TOKEN: &str = "<defaulted>";

/// Base formatter run before and after sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, clap::ValueEnum)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
#[clap(rename_all = "kebab-case")]
pub enum FormatterChoice {
    #[default]
    Nixfmt,
    Alejandra,
    NixpkgsFmt,
    /// Disable the base formatter; only reorder.
    Off,
}

impl FormatterChoice {
    pub fn argv(self) -> &'static [&'static str] {
        match self {
            FormatterChoice::Nixfmt => &["nixfmt", "-"],
            FormatterChoice::Alejandra => &["alejandra", "--quiet", "-"],
            FormatterChoice::NixpkgsFmt => &["nixpkgs-fmt"],
            FormatterChoice::Off => &[],
        }
    }
}

/// Which bindings receive blank-line spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum BlankLinesMode {
    /// Space bindings whose text spans several lines; keep consecutive
    /// single-line bindings together.
    #[default]
    Multiline,
    /// Apply spacing to all bindings.
    All,
    /// Suppress spacing entirely.
    Off,
}

/// Quoting style for attribute and `inherit` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum NameStyle {
    /// Keep names as written.
    #[default]
    Preserve,
    /// Unquote names that are valid identifiers: `"a" = 1;` becomes
    /// `a = 1;`.
    Identifier,
    /// Quote every name: `a = 1;` becomes `"a" = 1;`.
    String,
}

/// Where `inherit` statements land relative to ordinary bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum InheritPlacement {
    /// Pin `inherit lines to the top of the set.
    #[default]
    Front,
    /// Pin `inherit lines to the bottom of the set.
    Last,
    /// Sort `inherit lines alphabetically among the bindings.
    Sorted,
}

/// Rules for one sortable construct (`args`, `attrs`, `lets`, `inherits`,
/// `lists`). The `merge`, `flatten`, and `blank-lines*` keys only apply to
/// `attrs`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SortRules {
    /// Whether to reorder this construct at all.
    #[serde(default = "default_true")]
    pub sort: bool,
    /// Names pinned to the front, in the given order. The sentinel
    /// `"<defaulted>"` stands for all arguments with a default value
    /// (`name ? value`); `"..."` is the ellipsis.
    #[serde(default)]
    pub first: Vec<String>,
    /// Names pinned to the end, in the given order.
    #[serde(default)]
    pub last: Vec<String>,
    /// (attrs only) Merge bindings sharing their first attrpath component into
    /// one nested set: `a.b = 1; a.c = 2;` becomes `a = { b = 1; c = 2; };`.
    #[serde(default)]
    pub merge: bool,
    /// (attrs only) Flatten a binding whose value is an attrset holding a
    /// single binding into one attrpath: `a = { b = 1; };` becomes
    /// `a.b = 1;`. Overrides match the path of the set being flattened.
    #[serde(default)]
    pub flatten: bool,
    /// (attrs, lets, inherits) Quoting style for names: `identifier` unquotes
    /// names that are valid identifiers (`"a" = 1;` becomes `a = 1;`),
    /// `string` quotes every name, and `preserve` (the default) keeps names
    /// as written.
    #[serde(default)]
    #[cfg_attr(feature = "docs", schemars(default = "default_name_style_doc"))]
    pub name_style: NameStyle,
    /// (attrs only) Number of blank lines between the set's bindings. Unset
    /// keeps the existing spacing.
    #[serde(default)]
    pub blank_lines: Option<usize>,
    /// (attrs only) Which bindings receive the blank-line spacing.
    #[serde(default)]
    pub blank_lines_mode: Option<BlankLinesMode>,
    /// (attrs only) How deep the spacing reaches; wrappers such as functions
    /// and `let`s do not count as levels.
    #[serde(default = "default_blank_lines_depth")]
    pub blank_lines_depth: usize,
}

impl Default for SortRules {
    fn default() -> Self {
        toml::from_str("").expect("empty rules must deserialize")
    }
}

fn rules_off() -> SortRules {
    SortRules {
        sort: false,
        ..SortRules::default()
    }
}

/// Partial version of [`SortRules`] where every key is optional, used inside
/// `[[overrides]]` so an override only touches the keys it names.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PartialRules {
    pub sort: Option<bool>,
    pub first: Option<Vec<String>>,
    pub last: Option<Vec<String>>,
    pub merge: Option<bool>,
    pub flatten: Option<bool>,
    pub name_style: Option<NameStyle>,
    pub blank_lines: Option<usize>,
    pub blank_lines_mode: Option<BlankLinesMode>,
    pub blank_lines_depth: Option<usize>,
}

impl PartialRules {
    fn apply(&self, rules: &mut SortRules) {
        if let Some(sort) = self.sort {
            rules.sort = sort;
        }
        if let Some(first) = &self.first {
            rules.first = first.clone();
        }
        if let Some(last) = &self.last {
            rules.last = last.clone();
        }
        if let Some(merge) = self.merge {
            rules.merge = merge;
        }
        if let Some(flatten) = self.flatten {
            rules.flatten = flatten;
        }
        if let Some(name_style) = self.name_style {
            rules.name_style = name_style;
        }
        if let Some(blank_lines) = self.blank_lines {
            rules.blank_lines = Some(blank_lines);
        }
        if let Some(mode) = self.blank_lines_mode {
            rules.blank_lines_mode = Some(mode);
        }
        if let Some(depth) = self.blank_lines_depth {
            rules.blank_lines_depth = depth;
        }
    }
}

/// A per-path override. `path` is a glob over dot-separated attribute paths
/// (`*` matches one component, `**` any number); the remaining keys set
/// partial rules for the matched paths.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Override {
    /// Glob over dot-separated attribute paths. `*` matches exactly one
    /// component, `**` any number (including zero).
    pub path: String,
    /// Attribute-set rules to apply to matched paths.
    pub attrs: Option<PartialRules>,
    /// Function-argument rules to apply to matched paths.
    pub args: Option<PartialRules>,
    /// `let`-binding rules to apply to matched paths.
    pub lets: Option<PartialRules>,
    /// `inherit` rules to apply to matched paths.
    pub inherits: Option<PartialRules>,
    /// List-element rules to apply to matched paths.
    pub lists: Option<PartialRules>,
}

#[derive(Clone, Copy)]
pub enum RuleKind {
    Args,
    Attrs,
    Lets,
    Inherits,
    Lists,
}

/// A complete pedantix configuration, matching the structure of
/// `pedantix.toml`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    /// Base formatter run before and after sorting.
    #[serde(default)]
    #[cfg_attr(feature = "docs", schemars(default = "default_formatter_doc"))]
    pub formatter: FormatterChoice,
    /// Arbitrary `stdin -> stdout` command; overrides `formatter`. Because it
    /// runs an external program, it is only honored from configs named
    /// explicitly (`--config`, `--config-toml`, `--set`, the global XDG
    /// config); an auto-discovered config requires `--allow-formatter-command`.
    #[serde(default)]
    pub formatter_command: Option<Vec<String>>,
    /// Run the base formatter before sorting.
    #[serde(default = "default_true")]
    pub format_before_sort: bool,
    /// Run the base formatter again after sorting.
    #[serde(default = "default_true")]
    pub format_after_sort: bool,
    /// Exact number of blank lines between the bindings of the file's
    /// outermost attribute set. Unset keeps existing blank lines as-is.
    #[serde(default)]
    pub top_level_blank_lines: Option<usize>,
    /// Which top-level bindings receive the blank-line spacing.
    #[serde(default)]
    #[cfg_attr(feature = "docs", schemars(default = "default_blank_lines_mode_doc"))]
    pub top_level_blank_lines_mode: BlankLinesMode,
    /// How deep the top-level spacing reaches: 1 is only the outermost set, 2
    /// also covers the sets its bindings define, and so on. In `flake.nix`,
    /// `inputs` and `outputs` always count as top-level sets.
    #[serde(default = "default_blank_lines_depth")]
    pub top_level_blank_lines_depth: usize,
    /// Where `inherit` statements land relative to ordinary bindings.
    #[serde(default)]
    #[cfg_attr(feature = "docs", schemars(default = "default_inherit_placement_doc"))]
    pub inherit_placement: InheritPlacement,
    /// Rules for function arguments (`{ lib, config, pkgs, ... }`).
    #[serde(default)]
    pub args: SortRules,
    /// Rules for attribute-set bindings.
    #[serde(default)]
    pub attrs: SortRules,
    /// Rules for `let ... in` bindings (off by default).
    #[serde(default = "rules_off")]
    pub lets: SortRules,
    /// Rules for the names inside an `inherit` (off by default).
    #[serde(default = "rules_off")]
    pub inherits: SortRules,
    /// Rules for list elements (off by default, since list order is often
    /// significant).
    #[serde(default = "rules_off")]
    pub lists: SortRules,
    /// Per-path overrides that change the rules above for specific attribute
    /// paths.
    #[serde(default)]
    pub overrides: Vec<Override>,
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str("").expect("empty config must deserialize")
    }
}

const CONFIG_FILE_NAMES: &[&str] = &["pedantix.toml", ".pedantix.toml"];

const PRESETS: &[(&str, &str)] = &[
    ("nixos-module", include_str!("../presets/nixos-module.toml")),
    (
        "nixpkgs-package",
        include_str!("../presets/nixpkgs-package.toml"),
    ),
    ("alphabetical", include_str!("../presets/alphabetical.toml")),
];

fn expand_preset(mut table: toml::Table) -> Result<toml::Table> {
    let Some(preset_value) = table.remove("preset") else {
        return Ok(table);
    };
    let name = preset_value.as_str().context("`preset` must be a string")?;
    let preset_text = PRESETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, text)| *text)
        .with_context(|| {
            let available: Vec<&str> = PRESETS.iter().map(|(n, _)| *n).collect();
            format!(
                "unknown preset `{name}`; available presets: {}",
                available.join(", ")
            )
        })?;
    let preset: toml::Table = preset_text.parse().expect("embedded preset is valid TOML");
    let preset_overrides = preset.get("overrides").and_then(|v| v.as_array()).cloned();
    let user_overrides = table.get("overrides").and_then(|v| v.as_array()).cloned();
    let mut merged = merge_tables(preset, table);
    if let (Some(mut combined), Some(user)) = (preset_overrides, user_overrides) {
        combined.extend(user);
        merged.insert("overrides".into(), toml::Value::Array(combined));
    }
    Ok(merged)
}

fn expand_files(mut table: toml::Table, file: Option<&str>) -> Result<toml::Table> {
    let Some(files_value) = table.remove("files") else {
        return Ok(table);
    };
    let toml::Value::Array(entries) = files_value else {
        bail!("`files` must be an array of tables");
    };
    for entry in entries {
        let toml::Value::Table(mut entry) = entry else {
            bail!("`files` entries must be tables");
        };
        let pattern = entry
            .remove("pattern")
            .context("`files` entries require a `pattern` key")?;
        let pattern = pattern
            .as_str()
            .context("`files` entries require `pattern` to be a string")?
            .to_string();
        if pattern
            .strip_prefix("./")
            .unwrap_or(&pattern)
            .split('/')
            .any(|c| c == "." || c == "..")
        {
            bail!(
                "`files` pattern `{pattern}` contains a `.` or `..` component and can never \
                 match; patterns match paths anywhere beneath the config file's directory (a \
                 single leading `./` pins a pattern to that directory instead)"
            );
        }
        let has_preset = entry.contains_key("preset");
        let entry = expand_preset(entry)
            .with_context(|| format!("invalid `files` entry for pattern `{pattern}`"))?;
        Config::try_validated(entry.clone())
            .with_context(|| format!("invalid `files` entry for pattern `{pattern}`"))?;
        if !file.is_some_and(|f| glob_match_file(&pattern, f)) {
            continue;
        }
        if has_preset {
            table.remove("preset");
        }
        let base_overrides = table.get("overrides").and_then(|v| v.as_array()).cloned();
        let entry_overrides = entry.get("overrides").and_then(|v| v.as_array()).cloned();
        table = merge_tables(table, entry);
        if let (Some(mut combined), Some(from_entry)) = (base_overrides, entry_overrides) {
            combined.extend(from_entry);
            table.insert("overrides".into(), toml::Value::Array(combined));
        }
    }
    Ok(table)
}

pub fn sets_formatter_command(table: &toml::Table) -> bool {
    table.contains_key("formatter-command")
        || table
            .get("files")
            .and_then(|v| v.as_array())
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry
                        .as_table()
                        .is_some_and(|t| t.contains_key("formatter-command"))
                })
            })
}

fn merge_tables(base: toml::Table, over: toml::Table) -> toml::Table {
    let mut out = base;
    for (key, over_value) in over {
        let merged = match (out.remove(&key), over_value) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => {
                toml::Value::Table(merge_tables(b, o))
            }
            (_, o) => o,
        };
        out.insert(key, merged);
    }
    out
}

impl Config {
    pub fn from_table(table: toml::Table) -> Result<Config> {
        Config::from_table_for_file(table, None, &[])
    }

    pub fn from_table_for_file(
        table: toml::Table,
        file: Option<&str>,
        sets: &[String],
    ) -> Result<Config> {
        let mut table = expand_files(table, file)?;
        for assignment in sets {
            if assignment
                .split_once('=')
                .is_some_and(|(k, _)| k.trim().split('.').next() == Some("files"))
            {
                continue;
            }
            apply_set(&mut table, assignment)?;
        }
        Config::try_validated(expand_preset(table)?)
    }

    fn try_validated(table: toml::Table) -> Result<Config> {
        let cfg: Config = table.try_into()?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        let check = |first: &[String], last: &[String], what: &str| -> Result<()> {
            if first.iter().any(|n| n == "...") {
                bail!(
                    "{what} pins `...` via `first`, but `...` must stay the final formal; \
                     pin it as the last entry of `last` instead"
                );
            }
            if last
                .iter()
                .position(|n| n == "...")
                .is_some_and(|i| i + 1 != last.len())
            {
                bail!(
                    "{what} pins `...` before other names via `last`, but `...` must stay \
                     the final formal"
                );
            }
            Ok(())
        };
        check(&self.args.first, &self.args.last, "`args`")?;
        for o in &self.overrides {
            if let Some(args) = &o.args {
                check(
                    args.first.as_deref().unwrap_or_default(),
                    args.last.as_deref().unwrap_or_default(),
                    &format!("`args` in the override for `{}`", o.path),
                )?;
            }
        }
        Ok(())
    }

    pub fn from_toml_str(text: &str) -> Result<Config> {
        Config::from_table(text.parse().context("invalid TOML")?)
    }

    pub fn discover_path(dir: &Path) -> Option<PathBuf> {
        let repo_root = dir.ancestors().find(|a| a.join(".git").exists());
        for ancestor in dir.ancestors() {
            for name in CONFIG_FILE_NAMES {
                let candidate = ancestor.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            if repo_root.is_none_or(|root| ancestor == root) {
                return None;
            }
        }
        None
    }

    pub fn fallback_path() -> Option<PathBuf> {
        Self::fallback_path_from(
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        )
    }

    fn fallback_path_from(
        xdg_config_home: Option<std::ffi::OsString>,
        home: Option<std::ffi::OsString>,
    ) -> Option<PathBuf> {
        let base = xdg_config_home
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|home| PathBuf::from(home).join(".config")))?;
        let candidate = base.join("pedantix").join("pedantix.toml");
        candidate.is_file().then_some(candidate)
    }

    pub fn lists_may_sort(&self) -> bool {
        self.lists.sort
            || self
                .overrides
                .iter()
                .any(|o| o.lists.as_ref().is_some_and(|l| l.sort == Some(true)))
    }

    pub fn blank_lines_may_apply(&self) -> bool {
        self.top_level_blank_lines.is_some()
            || self.attrs.blank_lines.is_some()
            || self
                .overrides
                .iter()
                .any(|o| o.attrs.as_ref().is_some_and(|a| a.blank_lines.is_some()))
    }

    pub fn attrs_may_merge(&self) -> bool {
        self.attrs.merge
            || self
                .overrides
                .iter()
                .any(|o| o.attrs.as_ref().is_some_and(|a| a.merge == Some(true)))
    }

    pub fn names_may_restyle(&self) -> bool {
        let on = |rules: &SortRules| rules.name_style != NameStyle::Preserve;
        on(&self.attrs)
            || on(&self.lets)
            || on(&self.inherits)
            || self.overrides.iter().any(|o| {
                [&o.attrs, &o.lets, &o.inherits].into_iter().any(|p| {
                    p.as_ref().is_some_and(|p| {
                        p.name_style
                            .is_some_and(|style| style != NameStyle::Preserve)
                    })
                })
            })
    }

    pub fn attrs_may_flatten(&self) -> bool {
        self.attrs.flatten
            || self
                .overrides
                .iter()
                .any(|o| o.attrs.as_ref().is_some_and(|a| a.flatten == Some(true)))
    }

    pub fn formatter_argv(&self) -> Vec<String> {
        match &self.formatter_command {
            Some(argv) => argv.clone(),
            None => self
                .formatter
                .argv()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    pub fn rules_at(&self, kind: RuleKind, path: &[String]) -> Cow<'_, SortRules> {
        let mut rules = Cow::Borrowed(match kind {
            RuleKind::Args => &self.args,
            RuleKind::Attrs => &self.attrs,
            RuleKind::Lets => &self.lets,
            RuleKind::Inherits => &self.inherits,
            RuleKind::Lists => &self.lists,
        });
        for o in &self.overrides {
            if glob_match_path(&o.path, path) {
                let partial = match kind {
                    RuleKind::Args => &o.args,
                    RuleKind::Attrs => &o.attrs,
                    RuleKind::Lets => &o.lets,
                    RuleKind::Inherits => &o.inherits,
                    RuleKind::Lists => &o.lists,
                };
                if let Some(partial) = partial {
                    partial.apply(rules.to_mut());
                }
            }
        }
        rules
    }
}

pub fn apply_set(table: &mut toml::Table, assignment: &str) -> Result<()> {
    let (key, value) = assignment
        .split_once('=')
        .with_context(|| format!("--set expects KEY.PATH=VALUE, got `{assignment}`"))?;
    let value: toml::Value = match format!("v = {}", value.trim()).parse::<toml::Table>() {
        Ok(mut t) => t.remove("v").expect("just inserted"),
        Err(_) => toml::Value::String(value.trim().to_string()),
    };
    let keys: Vec<&str> = key.trim().split('.').collect();
    let (leaf, parents) = keys
        .split_last()
        .expect("split produces at least one element");
    let mut current = table;
    for k in parents {
        current = current
            .entry(k.to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .with_context(|| format!("--set: config key `{k}` is not a table"))?;
    }
    current.insert(leaf.to_string(), value);
    Ok(())
}

pub fn ignored_keys(table: &toml::Table) -> Vec<String> {
    let mut found = Vec::new();
    collect_ignored(table, "", &mut found);
    if let Some(toml::Value::Array(entries)) = table.get("files") {
        for entry in entries {
            let Some(entry) = entry.as_table() else {
                continue;
            };
            let location = entry
                .get("pattern")
                .and_then(toml::Value::as_str)
                .map(|p| format!(" in the `files` entry for `{p}`"))
                .unwrap_or_default();
            collect_ignored(entry, &location, &mut found);
        }
    }
    found
}

fn collect_ignored(table: &toml::Table, location: &str, found: &mut Vec<String>) {
    const SECTIONS: &[&str] = &["args", "lets", "inherits", "lists"];
    const ATTRS_ONLY: &[&str] = &[
        "merge",
        "flatten",
        "blank-lines",
        "blank-lines-mode",
        "blank-lines-depth",
    ];
    let mut check = |section: &str, rules: &toml::Table, context: &str| {
        for key in ATTRS_ONLY {
            if rules.contains_key(*key) {
                found.push(format!(
                    "`{section}.{key}`{context} has no effect; only `attrs` supports `{key}`"
                ));
            }
        }
        if matches!(section, "args" | "lists") && rules.contains_key("name-style") {
            found.push(format!(
                "`{section}.name-style`{context} has no effect; only `attrs`, `lets`, and \
                 `inherits` support `name-style`"
            ));
        }
    };
    for section in SECTIONS {
        if let Some(toml::Value::Table(rules)) = table.get(*section) {
            check(section, rules, location);
        }
    }
    if let Some(toml::Value::Array(overrides)) = table.get("overrides") {
        for entry in overrides {
            let Some(entry) = entry.as_table() else {
                continue;
            };
            let context = entry
                .get("path")
                .and_then(toml::Value::as_str)
                .map(|p| format!(" in the override for `{p}`{location}"))
                .unwrap_or_else(|| location.to_string());
            for section in SECTIONS {
                if let Some(toml::Value::Table(rules)) = entry.get(*section) {
                    check(section, rules, &context);
                }
            }
        }
    }
}

fn glob_match_file(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = match pattern.strip_prefix("./") {
        Some(anchored) => anchored.split('/').collect(),
        None => ["**"].into_iter().chain(pattern.split('/')).collect(),
    };
    let path: Vec<&str> = path.split('/').collect();
    file_glob(&pat, &path)
}

fn file_glob(pat: &[&str], path: &[&str]) -> bool {
    match pat.first() {
        None => path.is_empty(),
        Some(&"**") => {
            file_glob(&pat[1..], path) || (!path.is_empty() && file_glob(pat, &path[1..]))
        }
        Some(&p) => match path.first() {
            Some(&c) if component_match(p.as_bytes(), c.as_bytes()) => {
                file_glob(&pat[1..], &path[1..])
            }
            _ => false,
        },
    }
}

fn component_match(pat: &[u8], text: &[u8]) -> bool {
    match pat.first() {
        None => text.is_empty(),
        Some(b'*') => {
            component_match(&pat[1..], text)
                || (!text.is_empty() && component_match(pat, &text[1..]))
        }
        Some(c) => text.first() == Some(c) && component_match(&pat[1..], &text[1..]),
    }
}

fn glob_match_path(pattern: &str, path: &[String]) -> bool {
    let pat: Vec<&str> = pattern.split('.').collect();
    let path: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
    glob_match(&pat, &path)
}

fn glob_match(pat: &[&str], path: &[&str]) -> bool {
    match pat.first() {
        None => path.is_empty(),
        Some(&"**") => {
            glob_match(&pat[1..], path) || (!path.is_empty() && glob_match(pat, &path[1..]))
        }
        Some(&p) => match path.first() {
            Some(&c) if p == "*" || p == c => glob_match(&pat[1..], &path[1..]),
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matching() {
        let path = |s: &str| -> Vec<String> { s.split('.').map(String::from).collect() };
        assert!(glob_match_path("a.b.c", &path("a.b.c")));
        assert!(!glob_match_path("a.b", &path("a.b.c")));
        assert!(glob_match_path("a.*.c", &path("a.b.c")));
        assert!(glob_match_path("**.c", &path("a.b.c")));
        assert!(glob_match_path("**", &path("a.b.c")));
        assert!(glob_match_path("**", &[]));
        assert!(glob_match_path("a.**.c", &path("a.c")));
        assert!(glob_match_path("a.**.c", &path("a.x.y.c")));
        assert!(!glob_match_path("**.d", &path("a.b.c")));
    }

    #[test]
    fn file_glob_matching() {
        assert!(glob_match_file("*.pkg.nix", "hello.pkg.nix"));
        assert!(glob_match_file("*.pkg.nix", "pkgs/deep/hello.pkg.nix"));
        assert!(!glob_match_file("*.pkg.nix", "hello.nix"));
        assert!(!glob_match_file("*.pkg.nix", "pkg.nix"));
        assert!(glob_match_file("hello.nix", "a/b/hello.nix"));
        assert!(glob_match_file("pkgs/*.nix", "pkgs/hello.nix"));
        assert!(glob_match_file("pkgs/*.nix", "deep/er/pkgs/hello.nix"));
        assert!(!glob_match_file("pkgs/*.nix", "other/hello.nix"));
        assert!(!glob_match_file("pkgs/*.nix", "pkgs/deep/hello.nix"));
        assert!(glob_match_file("pkgs/**/*.nix", "pkgs/deep/er/hello.nix"));
        assert!(glob_match_file("pkgs/**/*.nix", "pkgs/hello.nix"));
        assert!(glob_match_file("by-name/**", "pkgs/by-name/he/hello.nix"));
        assert!(!glob_match_file("pkgs/*.nix", "üñï/hello.nix"));
        assert!(glob_match_file("*.nix", "üñï.nix"));
        assert!(glob_match_file("./test.nix", "test.nix"));
        assert!(!glob_match_file("./test.nix", "a/test.nix"));
        assert!(glob_match_file("./*.nix", "test.nix"));
        assert!(!glob_match_file("./*.nix", "a/test.nix"));
        assert!(glob_match_file("./nested/*.nix", "nested/deep.nix"));
        assert!(!glob_match_file("./nested/*.nix", "a/nested/deep.nix"));
    }

    #[test]
    fn files_entries_layer_onto_matching_files() {
        let table: toml::Table = r#"
            preset = "nixos-module"
            formatter = "alejandra"

            [[overrides]]
            path = "**.alias"
            attrs.sort = false

            [[files]]
            pattern = "*.pkg.nix"
            preset = "nixpkgs-package"

            [[files]]
            pattern = "*.pkg.nix"
            attrs.first = ["version"]

            [[files]]
            pattern = "unrelated.nix"
            formatter = "off"

            [[files.overrides]]
            path = "**.src"
            attrs.sort = false
        "#
        .parse()
        .unwrap();

        let cfg = Config::from_table_for_file(table.clone(), Some("mod.nix"), &[]).unwrap();
        assert_eq!(cfg.args.first[..3], ["config", "lib", "pkgs"]);
        assert_eq!(cfg.formatter, FormatterChoice::Alejandra);

        let cfg =
            Config::from_table_for_file(table.clone(), Some("pkgs/hello.pkg.nix"), &[]).unwrap();
        assert_eq!(cfg.args.first[0], "lib");
        assert_eq!(cfg.attrs.first, vec!["version"]);
        assert_eq!(cfg.formatter, FormatterChoice::Alejandra);
        let p = |s: &str| -> Vec<String> { s.split('.').map(String::from).collect() };
        assert!(!cfg.rules_at(RuleKind::Attrs, &p("x.alias")).sort);
        assert_eq!(cfg.rules_at(RuleKind::Attrs, &p("pkg.src")).first[0], "url");

        let cfg = Config::from_table_for_file(table.clone(), Some("unrelated.nix"), &[]).unwrap();
        assert_eq!(cfg.formatter, FormatterChoice::Off);
        assert!(!cfg.rules_at(RuleKind::Attrs, &p("x.alias")).sort);
        assert!(!cfg.rules_at(RuleKind::Attrs, &p("pkg.src")).sort);

        let cfg = Config::from_table_for_file(table, None, &[]).unwrap();
        assert_eq!(cfg.args.first[..3], ["config", "lib", "pkgs"]);
    }

    #[test]
    fn files_presets_override_root_config() {
        let table: toml::Table = r#"
            [args]
            first = ["lib", "config", "pkgs", "inputs", "inputs'", "self", "self'"]
            last = ["<defaulted>", "..."]

            [lets]
            sort = true

            [attrs]
            first = ["flake-file", "imports", "perSystem"]
            flatten = true
            merge = true

            [[files]]
            pattern = "**/*.pkg.nix"
            preset = "nixpkgs-package"
        "#
        .parse()
        .unwrap();

        let cfg =
            Config::from_table_for_file(table.clone(), Some("pkgs/foo.pkg.nix"), &[]).unwrap();
        assert_eq!(cfg.attrs.first[..2], ["pname", "version"]);
        assert_eq!(cfg.args.first[..2], ["lib", "stdenv"]);
        assert!(cfg.lets.sort);
        assert!(cfg.attrs.flatten);
        let p = |s: &str| -> Vec<String> { s.split('.').map(String::from).collect() };
        assert_eq!(cfg.rules_at(RuleKind::Attrs, &p("pkg.src")).first[0], "url");

        let cfg = Config::from_table_for_file(table, Some("flake.nix"), &[]).unwrap();
        assert_eq!(cfg.attrs.first[0], "flake-file");
    }

    #[test]
    fn files_presets_replace_the_root_preset() {
        let table: toml::Table = r#"
            preset = "nixos-module"

            [[files]]
            pattern = "*.pkg.nix"
            preset = "alphabetical"
        "#
        .parse()
        .unwrap();
        let cfg = Config::from_table_for_file(table, Some("foo.pkg.nix"), &[]).unwrap();
        assert!(cfg.attrs.sort);
        assert!(cfg.attrs.first.is_empty(), "root preset must be ignored");
    }

    #[test]
    fn set_assignments_win_over_files_entries() {
        let table: toml::Table = r#"
            formatter-command = ["repo"]

            [[files]]
            pattern = "*.nix"
            attrs.sort = false
            formatter = "alejandra"
            formatter-command = ["entry"]
        "#
        .parse()
        .unwrap();
        let sets = [
            "attrs.sort=true".to_string(),
            r#"formatter-command=["mine"]"#.to_string(),
        ];
        let cfg = Config::from_table_for_file(table.clone(), Some("a.nix"), &sets).unwrap();
        assert!(cfg.attrs.sort);
        assert_eq!(cfg.formatter_command, Some(vec!["mine".to_string()]));
        assert_eq!(cfg.formatter, FormatterChoice::Alejandra);
        let cfg = Config::from_table_for_file(table, Some("a.nix"), &[]).unwrap();
        assert!(!cfg.attrs.sort);
        assert_eq!(cfg.formatter_command, Some(vec!["entry".to_string()]));
    }

    #[test]
    fn files_entries_are_validated_even_without_a_match() {
        let err = |toml: &str| Config::from_toml_str(toml).expect_err(toml).to_string();
        assert!(err("files = 5").contains("array of tables"));
        assert!(err("files = [5]").contains("must be tables"));
        assert!(err("[[files]]\npreset = \"nixos-module\"").contains("`pattern`"));
        assert!(err("[[files]]\npattern = 5").contains("`pattern`"));
        assert!(err("[[files]]\npattern = \"*.nix\"\npreset = \"nope\"")
            .contains("invalid `files` entry for pattern `*.nix`"));
        assert!(err("[[files]]\npattern = \"*.nix\"\nnot-a-key = true")
            .contains("invalid `files` entry"));
        assert!(
            err("[[files]]\npattern = \"a.nix\"\n[[files.files]]\npattern = \"b.nix\"")
                .contains("invalid `files` entry")
        );
        assert!(err("[[files]]\npattern = \"../test.nix\"").contains("can never match"));
        assert!(err("[[files]]\npattern = \"a/./test.nix\"").contains("can never match"));
        assert!(err("[[files]]\npattern = \"././test.nix\"").contains("can never match"));
    }

    #[test]
    fn formatter_command_is_detected_inside_files_entries() {
        let top: toml::Table = "formatter-command = [\"cat\"]".parse().unwrap();
        assert!(sets_formatter_command(&top));
        let nested: toml::Table = "[[files]]\npattern = \"*.nix\"\nformatter-command = [\"cat\"]\n"
            .parse()
            .unwrap();
        assert!(sets_formatter_command(&nested));
        assert!(!sets_formatter_command(
            &"formatter = \"off\"".parse().unwrap()
        ));
    }

    #[test]
    fn ignored_keys_are_reported_inside_files_entries() {
        let table: toml::Table = r#"
            [[files]]
            pattern = "*.pkg.nix"
            args.blank-lines = 1

            [[files.overrides]]
            path = "**.xs"
            lists.merge = true
        "#
        .parse()
        .unwrap();
        let found = ignored_keys(&table);
        assert_eq!(found.len(), 2);
        assert!(found[0].contains("`args.blank-lines`"));
        assert!(found[0].contains("in the `files` entry for `*.pkg.nix`"));
        assert!(found[1].contains("`lists.merge`"));
        assert!(
            found[1].contains("in the override for `**.xs` in the `files` entry for `*.pkg.nix`")
        );
    }

    #[test]
    fn ellipsis_pins_must_stay_final() {
        let err = |toml: &str| format!("{:#}", Config::from_toml_str(toml).expect_err(toml));
        assert!(err("[args]\nfirst = [\"...\"]").contains("final formal"));
        assert!(err("[args]\nlast = [\"...\", \"x\"]").contains("final formal"));
        assert!(Config::from_toml_str("[args]\nlast = [\"x\", \"...\"]").is_ok());
        assert!(
            err("[[overrides]]\npath = \"**.x\"\nargs.first = [\"...\"]")
                .contains("override for `**.x`")
        );
        assert!(
            err("[[files]]\npattern = \"*.nix\"\nargs.last = [\"...\", \"x\"]")
                .contains("invalid `files` entry for pattern `*.nix`")
        );
    }

    #[test]
    fn default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.formatter, FormatterChoice::Nixfmt);
        assert!(cfg.format_before_sort && cfg.format_after_sort);
    }

    #[test]
    fn discovery_stops_at_the_git_repo_root() {
        let dir =
            std::env::temp_dir().join(format!("pedantix-discover-test-{}", std::process::id()));
        let repo = dir.join("repo");
        let sub = repo.join("sub");
        let inner = repo.join("inner");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(inner.join("deep")).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(dir.join("pedantix.toml"), "").unwrap();

        assert_eq!(Config::discover_path(&sub), None);
        assert_eq!(Config::discover_path(&dir), Some(dir.join("pedantix.toml")));

        std::fs::write(repo.join("pedantix.toml"), "").unwrap();
        assert_eq!(
            Config::discover_path(&sub),
            Some(repo.join("pedantix.toml"))
        );

        std::fs::write(inner.join(".git"), "gitdir: elsewhere").unwrap();
        assert_eq!(Config::discover_path(&inner.join("deep")), None);

        let dotted = repo.join("sub");
        std::fs::write(dotted.join(".pedantix.toml"), "").unwrap();
        assert_eq!(
            Config::discover_path(&dotted),
            Some(dotted.join(".pedantix.toml"))
        );
        std::fs::write(dotted.join("pedantix.toml"), "").unwrap();
        assert_eq!(
            Config::discover_path(&dotted),
            Some(dotted.join("pedantix.toml")),
            "the undotted name wins when both are present"
        );

        let plain = dir.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(Config::discover_path(&plain), None);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fallback_path_uses_xdg_config_home() {
        let dir = std::env::temp_dir().join(format!("pedantix-xdg-test-{}", std::process::id()));
        let cfg_dir = dir.join("pedantix");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join("pedantix.toml"), "formatter = \"off\"\n").unwrap();
        let xdg = Some(dir.clone().into_os_string());
        let home = Some(dir.clone().into_os_string());

        assert_eq!(
            Config::fallback_path_from(xdg, None),
            Some(cfg_dir.join("pedantix.toml"))
        );
        assert_eq!(Config::fallback_path_from(None, home.clone()), None);

        let home_cfg_dir = dir.join(".config").join("pedantix");
        std::fs::create_dir_all(&home_cfg_dir).unwrap();
        std::fs::write(home_cfg_dir.join("pedantix.toml"), "formatter = \"off\"\n").unwrap();
        assert_eq!(
            Config::fallback_path_from(None, home.clone()),
            Some(home_cfg_dir.join("pedantix.toml"))
        );
        assert_eq!(
            Config::fallback_path_from(Some("relative/xdg".into()), home),
            Some(home_cfg_dir.join("pedantix.toml"))
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_set_merges_onto_table() {
        let mut table: toml::Table = "[args]\nfirst = [\"lib\"]\n".parse().unwrap();
        apply_set(&mut table, "lets.sort=true").unwrap();
        apply_set(&mut table, "inherits.sort=true").unwrap();
        apply_set(&mut table, "formatter=alejandra").unwrap();
        apply_set(&mut table, r#"args.last=["<defaulted>", "..."]"#).unwrap();
        let cfg: Config = table.try_into().unwrap();
        assert!(cfg.lets.sort);
        assert!(cfg.inherits.sort);
        assert_eq!(cfg.formatter, FormatterChoice::Alejandra);
        assert_eq!(cfg.args.first, vec!["lib"]);
        assert_eq!(cfg.args.last, vec![DEFAULTED_TOKEN, "..."]);
        assert!(apply_set(&mut toml::Table::new(), "no-equals-sign").is_err());
    }

    #[test]
    fn apply_set_accepts_inline_override_tables() {
        let mut table = toml::Table::new();
        apply_set(
            &mut table,
            r#"overrides=[{ path = "**.alias", attrs = { sort = false } }, { path = "**.src", attrs = { first = ["owner"] } }]"#,
        )
        .unwrap();
        let cfg: Config = Config::from_table(table).unwrap();
        assert_eq!(cfg.overrides.len(), 2);
        assert_eq!(cfg.overrides[0].path, "**.alias");
        assert_eq!(cfg.overrides[0].attrs.as_ref().unwrap().sort, Some(false));
        assert_eq!(
            cfg.overrides[1].attrs.as_ref().unwrap().first,
            Some(vec!["owner".to_string()])
        );
    }

    #[test]
    fn attrs_only_keys_are_reported_as_ignored() {
        let table: toml::Table = r#"
            [attrs]
            merge = true
            blank-lines = 1

            [args]
            blank-lines = 1

            [lets]
            sort = true
            merge = true
            flatten = true

            [[overrides]]
            path = "**.xs"
            attrs.blank-lines-depth = 2
            lists.blank-lines-mode = "all"
        "#
        .parse()
        .unwrap();
        let found = ignored_keys(&table);
        assert_eq!(found.len(), 4);
        assert!(found[0].contains("`args.blank-lines`"));
        assert!(found[1].contains("`lets.merge`"));
        assert!(found[2].contains("`lets.flatten`"));
        assert!(found[3].contains("`lists.blank-lines-mode`"));
        assert!(found[3].contains("override for `**.xs`"));
        assert!(ignored_keys(&"[attrs]\nmerge = true".parse().unwrap()).is_empty());
    }

    #[test]
    fn presets_are_valid_and_expand() {
        for (name, _) in PRESETS {
            let cfg = Config::from_toml_str(&format!("preset = \"{name}\"")).unwrap();
            assert!(cfg.args.sort, "preset {name} should keep args sorting on");
        }
        assert!(Config::from_toml_str("preset = \"nope\"")
            .unwrap_err()
            .to_string()
            .contains("unknown preset"));
        assert!(Config::from_toml_str("preset = \"nixpkgs-package\"\noverrides = 5").is_err());
    }

    #[test]
    fn explicit_config_wins_over_preset_and_overrides_concatenate() {
        let cfg = Config::from_toml_str(
            r#"
            preset = "nixpkgs-package"
            formatter = "alejandra"

            [args]
            first = ["mine"]

            [[overrides]]
            path = "**.src"
            attrs.first = ["custom"]
            "#,
        )
        .unwrap();
        assert_eq!(cfg.formatter, FormatterChoice::Alejandra);
        assert_eq!(cfg.args.first, vec!["mine"]);
        assert_eq!(cfg.attrs.first[0], "name");
        let src_overrides: Vec<_> = cfg
            .overrides
            .iter()
            .filter(|o| o.path == "**.src")
            .collect();
        assert_eq!(src_overrides.len(), 2);
        let p = |s: &str| -> Vec<String> { s.split('.').map(String::from).collect() };
        let rules = cfg.rules_at(RuleKind::Attrs, &p("pkg.src"));
        assert_eq!(rules.first, vec!["custom"]);
    }

    #[test]
    fn lists_may_sort_detection() {
        assert!(!Config::default().lists_may_sort());
        let cfg: Config = toml::from_str("[lists]\nsort = true").unwrap();
        assert!(cfg.lists_may_sort());
        let cfg: Config =
            toml::from_str("[[overrides]]\npath = \"**.xs\"\nlists.sort = true").unwrap();
        assert!(cfg.lists_may_sort());
        let cfg: Config =
            toml::from_str("[[overrides]]\npath = \"**.xs\"\nlists.first = [\"z\"]").unwrap();
        assert!(!cfg.lists_may_sort());
        let cfg: Config =
            toml::from_str("[[overrides]]\npath = \"**.xs\"\nlists.sort = false").unwrap();
        assert!(!cfg.lists_may_sort());
    }

    #[test]
    fn names_may_restyle_detection() {
        assert!(!Config::default().names_may_restyle());
        for section in ["attrs", "lets", "inherits"] {
            let cfg: Config =
                toml::from_str(&format!("[{section}]\nname-style = \"identifier\"")).unwrap();
            assert!(cfg.names_may_restyle(), "{section}");
        }
        let cfg: Config =
            toml::from_str("[[overrides]]\npath = \"**.xs\"\nattrs.name-style = \"string\"")
                .unwrap();
        assert!(cfg.names_may_restyle());
        let cfg: Config =
            toml::from_str("[[overrides]]\npath = \"**.xs\"\nlets.name-style = \"preserve\"")
                .unwrap();
        assert!(!cfg.names_may_restyle());
    }

    #[test]
    fn name_style_is_reported_as_ignored_outside_named_constructs() {
        let table: toml::Table = r#"
            [args]
            name-style = "identifier"

            [lets]
            name-style = "identifier"

            [[overrides]]
            path = "**.xs"
            lists.name-style = "string"
        "#
        .parse()
        .unwrap();
        let found = ignored_keys(&table);
        assert_eq!(found.len(), 2);
        assert!(found[0].contains("`args.name-style`"));
        assert!(found[1].contains("`lists.name-style`"));
        assert!(found[1].contains("override for `**.xs`"));
    }

    #[test]
    fn overrides_apply_in_order() {
        let cfg: Config = toml::from_str(
            r#"
            [attrs]
            first = ["enable"]

            [[overrides]]
            path = "**.alias"
            attrs.sort = false

            [[overrides]]
            path = "a.**"
            attrs.first = ["z"]
            "#,
        )
        .unwrap();
        let p = |s: &str| -> Vec<String> { s.split('.').map(String::from).collect() };
        let r = cfg.rules_at(RuleKind::Attrs, &p("x.alias"));
        assert!(!r.sort);
        assert_eq!(r.first, vec!["enable"]);
        let r = cfg.rules_at(RuleKind::Attrs, &p("a.b"));
        assert!(r.sort);
        assert_eq!(r.first, vec!["z"]);
        let r = cfg.rules_at(RuleKind::Attrs, &p("a.alias"));
        assert!(!r.sort);
        assert_eq!(r.first, vec!["z"]);
    }
}
