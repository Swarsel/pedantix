use anyhow::{Context, Result};
use serde::Deserialize;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

fn default_true() -> bool {
    true
}

fn default_blank_lines_depth() -> usize {
    1
}

pub const DEFAULTED_TOKEN: &str = "<defaulted>";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, clap::ValueEnum)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlankLinesMode {
    #[default]
    Multiline,
    All,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InheritPlacement {
    #[default]
    Front,
    Last,
    Sorted,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SortRules {
    #[serde(default = "default_true")]
    pub sort: bool,
    #[serde(default)]
    pub first: Vec<String>,
    #[serde(default)]
    pub last: Vec<String>,
    #[serde(default)]
    pub merge: bool,
    #[serde(default)]
    pub blank_lines: Option<usize>,
    #[serde(default)]
    pub blank_lines_mode: Option<BlankLinesMode>,
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PartialRules {
    pub sort: Option<bool>,
    pub first: Option<Vec<String>>,
    pub last: Option<Vec<String>>,
    pub merge: Option<bool>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Override {
    pub path: String,
    pub attrs: Option<PartialRules>,
    pub args: Option<PartialRules>,
    pub lets: Option<PartialRules>,
    pub inherits: Option<PartialRules>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub formatter: FormatterChoice,
    #[serde(default)]
    pub formatter_command: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub format_before_sort: bool,
    #[serde(default = "default_true")]
    pub format_after_sort: bool,
    #[serde(default)]
    pub top_level_blank_lines: Option<usize>,
    #[serde(default)]
    pub top_level_blank_lines_mode: BlankLinesMode,
    #[serde(default = "default_blank_lines_depth")]
    pub top_level_blank_lines_depth: usize,
    #[serde(default)]
    pub inherit_placement: InheritPlacement,
    #[serde(default)]
    pub args: SortRules,
    #[serde(default)]
    pub attrs: SortRules,
    #[serde(default = "rules_off")]
    pub lets: SortRules,
    #[serde(default = "rules_off")]
    pub inherits: SortRules,
    #[serde(default = "rules_off")]
    pub lists: SortRules,
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
        expand_preset(table)?
            .try_into()
            .context("invalid configuration")
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
        self.lists.sort || self.overrides.iter().any(|o| o.lists.is_some())
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
    const SECTIONS: &[&str] = &["args", "lets", "inherits", "lists"];
    const ATTRS_ONLY: &[&str] = &[
        "merge",
        "blank-lines",
        "blank-lines-mode",
        "blank-lines-depth",
    ];
    let mut found = Vec::new();
    let mut check = |section: &str, rules: &toml::Table, context: &str| {
        for key in ATTRS_ONLY {
            if rules.contains_key(*key) {
                found.push(format!(
                    "`{section}.{key}`{context} has no effect; only `attrs` supports `{key}`"
                ));
            }
        }
    };
    for section in SECTIONS {
        if let Some(toml::Value::Table(rules)) = table.get(*section) {
            check(section, rules, "");
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
                .map(|p| format!(" in the override for `{p}`"))
                .unwrap_or_default();
            for section in SECTIONS {
                if let Some(toml::Value::Table(rules)) = entry.get(*section) {
                    check(section, rules, &context);
                }
            }
        }
    }
    found
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
    fn default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.formatter, FormatterChoice::Nixfmt);
        assert!(cfg.args.sort);
        assert!(cfg.attrs.sort);
        assert!(!cfg.lets.sort);
        assert!(!cfg.inherits.sort);
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

            [[overrides]]
            path = "**.xs"
            attrs.blank-lines-depth = 2
            lists.blank-lines-mode = "all"
        "#
        .parse()
        .unwrap();
        let found = ignored_keys(&table);
        assert_eq!(found.len(), 3);
        assert!(found[0].contains("`args.blank-lines`"));
        assert!(found[1].contains("`lets.merge`"));
        assert!(found[2].contains("`lists.blank-lines-mode`"));
        assert!(found[2].contains("override for `**.xs`"));
        assert!(ignored_keys(&"[attrs]\nmerge = true".parse().unwrap()).is_empty());
    }

    #[test]
    fn presets_are_valid_and_expand() {
        for (name, _) in PRESETS {
            let cfg = Config::from_toml_str(&format!("preset = \"{name}\"")).unwrap();
            assert!(cfg.args.sort, "preset {name} should keep args sorting on");
        }
        let cfg = Config::from_toml_str("preset = \"nixpkgs-package\"").unwrap();
        assert_eq!(cfg.attrs.first[0], "pname");
        assert_eq!(cfg.attrs.last, vec!["passthru", "meta"]);
        assert_eq!(cfg.args.first[0], "lib");
        assert!(cfg.overrides.iter().any(|o| o.path == "**.src"));

        let cfg = Config::from_toml_str("preset = \"nixos-module\"").unwrap();
        assert_eq!(cfg.args.first[..3], ["config", "lib", "pkgs"]);
        assert_eq!(cfg.attrs.last, vec!["meta"]);

        assert!(
            Config::from_toml_str("preset = \"nope\"")
                .unwrap_err()
                .to_string()
                .contains("unknown preset")
        );
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
        assert_eq!(cfg.attrs.first[0], "pname");
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
