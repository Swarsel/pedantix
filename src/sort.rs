use crate::config::{Config, DEFAULTED_TOKEN, InheritPlacement, RuleKind, SortRules};
use anyhow::{Result, anyhow, bail};
use tree_sitter::{Node, Parser};

pub fn parse(src: &str) -> Result<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_nix::LANGUAGE.into())
        .expect("tree-sitter-nix language is compatible");
    parser
        .parse(src, None)
        .ok_or_else(|| anyhow!("tree-sitter failed to parse input"))
}

pub fn sort_source(src: &str, cfg: &Config) -> Result<String> {
    let tree = parse(src)?;
    let root = tree.root_node();
    if root.has_error() {
        bail!("input is not valid Nix (parse error); refusing to reorder");
    }
    let mut path: Vec<String> = Vec::new();
    Ok(rewrite(root, src, &mut path, cfg).unwrap_or_else(|| src.to_string()))
}

pub(crate) fn text<'a>(node: Node, src: &'a str) -> &'a str {
    &src[node.start_byte()..node.end_byte()]
}

fn rewrite(node: Node, src: &str, path: &mut Vec<String>, cfg: &Config) -> Option<String> {
    match node.kind() {
        // Binding sets are sorted from their container, because comments
        // before the first / after the last binding are siblings of the
        // binding_set node, not children.
        "attrset_expression" | "rec_attrset_expression" | "let_attrset_expression" => {
            sort_container(node, src, path, cfg, RuleKind::Attrs)
        }
        "let_expression" => sort_container(node, src, path, cfg, RuleKind::Lets),
        "formals" => sort_formals(node, src, path, cfg),
        "list_expression" => sort_list(node, src, path, cfg),
        "inherited_attrs" => sort_inherited_attrs(node, src, path, cfg),
        "binding" => descend_binding(node, src, path, |child, path| {
            rewrite(child, src, path, cfg)
        }),
        _ => splice_children(node, src, path, cfg),
    }
}

fn splice_children(node: Node, src: &str, path: &mut Vec<String>, cfg: &Config) -> Option<String> {
    splice_children_with(node, src, |child| rewrite(child, src, path, cfg))
}

pub(crate) fn splice_children_with<F>(node: Node, src: &str, mut rewrite_child: F) -> Option<String>
where
    F: FnMut(Node) -> Option<String>,
{
    let mut cursor = node.walk();
    let mut out = String::new();
    let mut pos = node.start_byte();
    let mut changed = false;
    for child in node.children(&mut cursor) {
        if let Some(new_text) = rewrite_child(child) {
            out.push_str(&src[pos..child.start_byte()]);
            out.push_str(&new_text);
            pos = child.end_byte();
            changed = true;
        }
    }
    if !changed {
        return None;
    }
    out.push_str(&src[pos..node.end_byte()]);
    Some(out)
}

pub(crate) fn descend_binding<F>(
    node: Node,
    src: &str,
    path: &mut Vec<String>,
    mut rewrite_child: F,
) -> Option<String>
where
    F: FnMut(Node, &mut Vec<String>) -> Option<String>,
{
    let expr_id = node.child_by_field_name("expression").map(|n| n.id());
    let comps = binding_components(node, src);
    splice_children_with(node, src, |child| {
        if Some(child.id()) == expr_id {
            let depth = path.len();
            path.extend(comps.iter().cloned());
            let result = rewrite_child(child, path);
            path.truncate(depth);
            result
        } else {
            rewrite_child(child, path)
        }
    })
}

pub(crate) fn binding_components(binding: Node, src: &str) -> Vec<String> {
    binding
        .child_by_field_name("attrpath")
        .map(|ap| attrpath_components(ap, src))
        .unwrap_or_default()
}

fn attrpath_components(attrpath: Node, src: &str) -> Vec<String> {
    let mut cursor = attrpath.walk();
    attrpath
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .map(|c| normalize_key(c, src))
        .collect()
}

pub(crate) fn normalize_key(node: Node, src: &str) -> String {
    let raw = text(node, src);
    if node.kind() == "string_expression" {
        let inner = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"'));
        if let Some(inner) = inner {
            return inner.to_string();
        }
    }
    raw.to_string()
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SortKey {
    class: u8,
    list_idx: usize,
    key: Vec<String>,
}

const CLASS_INHERIT_FRONT: u8 = 0;
const CLASS_FIRST: u8 = 1;
const CLASS_MIDDLE: u8 = 2;
const CLASS_LAST: u8 = 3;
const CLASS_ELLIPSES: u8 = 4;
const CLASS_INHERIT_LAST: u8 = 5;

fn ranked_key(name: &str, full_key: Vec<String>, rules: &SortRules) -> SortKey {
    if let Some(idx) = rules.first.iter().position(|f| f == name) {
        SortKey {
            class: CLASS_FIRST,
            list_idx: idx,
            key: full_key,
        }
    } else if let Some(idx) = rules.last.iter().position(|l| l == name) {
        SortKey {
            class: CLASS_LAST,
            list_idx: idx,
            key: full_key,
        }
    } else {
        SortKey {
            class: CLASS_MIDDLE,
            list_idx: 0,
            key: full_key,
        }
    }
}

pub(crate) struct Item<'a, K = SortKey> {
    pub(crate) leading: Vec<Node<'a>>,
    pub(crate) node: Node<'a>,
    pub(crate) trailing: Option<Node<'a>>,
    pub(crate) key: K,
}

pub(crate) fn collect_items<'a, K, F>(
    children: Vec<Node<'a>>,
    mut make_key: F,
) -> (Vec<Item<'a, K>>, Vec<Node<'a>>)
where
    F: FnMut(Node<'a>) -> K,
{
    let mut items: Vec<Item<'a, K>> = Vec::new();
    let mut pending: Vec<Node<'a>> = Vec::new();
    for child in children {
        if child.kind() == "comment" {
            match items.last_mut() {
                Some(last)
                    if last.trailing.is_none()
                        && pending.is_empty()
                        && child.start_position().row == last.node.end_position().row =>
                {
                    last.trailing = Some(child);
                }
                _ => pending.push(child),
            }
        } else {
            items.push(Item {
                leading: std::mem::take(&mut pending),
                node: child,
                key: make_key(child),
                trailing: None,
            });
        }
    }
    (items, pending)
}

fn line_start_of(node: Node, src: &str) -> usize {
    src[..node.start_byte()]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0)
}

pub(crate) fn indent_of(node: Node, src: &str) -> String {
    let prefix = &src[line_start_of(node, src)..node.start_byte()];
    if prefix.chars().all(|c| c == ' ' || c == '\t') {
        prefix.to_string()
    } else {
        " ".repeat(node.start_position().column)
    }
}

fn line_indent_of(node: Node, src: &str) -> String {
    src[line_start_of(node, src)..]
        .chars()
        .take_while(|&c| c == ' ' || c == '\t')
        .collect()
}

pub(crate) fn is_multiline(node: Node) -> bool {
    node.start_position().row != node.end_position().row
}

pub(crate) fn item_sep<K>(items: &[Item<K>], src: &str, single_line: bool) -> String {
    if single_line {
        " ".to_string()
    } else {
        format!("\n{}", indent_of(items[0].node, src))
    }
}

pub(crate) fn render_item_with<K, F>(
    item: &Item<K>,
    src: &str,
    sep: &str,
    suffix: &str,
    mut rewrite_node: F,
) -> String
where
    F: FnMut(Node) -> Option<String>,
{
    let mut out = String::new();
    for c in &item.leading {
        out.push_str(text(*c, src));
        out.push_str(sep);
    }
    match rewrite_node(item.node) {
        Some(new_text) => out.push_str(&new_text),
        None => out.push_str(text(item.node, src)),
    }
    out.push_str(suffix);
    if let Some(tr) = item.trailing {
        out.push(' ');
        out.push_str(text(tr, src));
    }
    out
}

fn render_item<K>(
    item: &Item<K>,
    src: &str,
    path: &mut Vec<String>,
    cfg: &Config,
    sep: &str,
    suffix: &str,
) -> String {
    render_item_with(item, src, sep, suffix, |node| rewrite(node, src, path, cfg))
}

fn changed_order(items: &[Item]) -> Option<Vec<usize>> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| items[a].key.cmp(&items[b].key));
    order
        .iter()
        .enumerate()
        .any(|(i, &o)| i != o)
        .then_some(order)
}

/// A `#` comment swallows the rest of its line, forcing multi-line output.
fn has_comments<K>(items: &[Item<K>], dangling: &[Node]) -> bool {
    !dangling.is_empty()
        || items
            .iter()
            .any(|i| !i.leading.is_empty() || i.trailing.is_some())
}

pub(crate) fn container_region(node: Node) -> Option<(Vec<Node>, usize, usize)> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    let bs_idx = children.iter().position(|c| c.kind() == "binding_set")?;
    let mut start_idx = bs_idx;
    while start_idx > 0 && children[start_idx - 1].kind() == "comment" {
        start_idx -= 1;
    }
    let mut end_idx = bs_idx;
    while end_idx + 1 < children.len() && children[end_idx + 1].kind() == "comment" {
        end_idx += 1;
    }
    let mut flat: Vec<Node> = Vec::new();
    for (i, child) in children.iter().enumerate() {
        if i < start_idx || i > end_idx {
            continue;
        }
        if i == bs_idx {
            let mut bs_cursor = child.walk();
            flat.extend(child.children(&mut bs_cursor));
        } else {
            flat.push(*child);
        }
    }
    Some((
        flat,
        children[start_idx].start_byte(),
        children[end_idx].end_byte(),
    ))
}

pub(crate) fn splice_region<F>(
    node: Node,
    src: &str,
    region_start: usize,
    region_end: usize,
    region_text: &str,
    mut rewrite_child: F,
) -> String
where
    F: FnMut(Node) -> Option<String>,
{
    let mut cursor = node.walk();
    let mut out = String::new();
    let mut pos = node.start_byte();
    for child in node.children(&mut cursor) {
        if child.start_byte() >= region_start && child.end_byte() <= region_end {
            if child.start_byte() == region_start {
                out.push_str(&src[pos..region_start]);
                out.push_str(region_text);
                pos = region_end;
            }
            continue;
        }
        if let Some(new_text) = rewrite_child(child) {
            out.push_str(&src[pos..child.start_byte()]);
            out.push_str(&new_text);
            pos = child.end_byte();
        }
    }
    out.push_str(&src[pos..node.end_byte()]);
    out
}

fn sort_container(
    node: Node,
    src: &str,
    path: &mut Vec<String>,
    cfg: &Config,
    kind: RuleKind,
) -> Option<String> {
    let rules = cfg.rules_at(kind, path);
    if !rules.sort {
        return splice_children(node, src, path, cfg);
    }
    let Some((flat, region_start, region_end)) = container_region(node) else {
        return splice_children(node, src, path, cfg);
    };
    let Some(region_text) = sort_bindings(flat, src, path, cfg, &rules) else {
        return splice_children(node, src, path, cfg);
    };
    Some(splice_region(
        node,
        src,
        region_start,
        region_end,
        &region_text,
        |child| rewrite(child, src, path, cfg),
    ))
}

fn sort_bindings(
    children: Vec<Node>,
    src: &str,
    path: &mut Vec<String>,
    cfg: &Config,
    rules: &SortRules,
) -> Option<String> {
    let inherit_sort = cfg.inherit_placement == InheritPlacement::Sorted
        && cfg.rules_at(RuleKind::Inherits, path).sort;
    let (items, dangling) = collect_items(children, |child| match child.kind() {
        "binding" => {
            let comps = binding_components(child, src);
            let name = comps.first().cloned().unwrap_or_default();
            ranked_key(&name, comps, rules)
        }
        "inherit" | "inherit_from" => match cfg.inherit_placement {
            InheritPlacement::Front => SortKey {
                class: CLASS_INHERIT_FRONT,
                list_idx: 0,
                key: Vec::new(),
            },
            InheritPlacement::Last => SortKey {
                class: CLASS_INHERIT_LAST,
                list_idx: 0,
                key: Vec::new(),
            },
            InheritPlacement::Sorted => {
                let names = inherit_names(child, src);
                let name = if inherit_sort {
                    names.iter().min().cloned()
                } else {
                    names.first().cloned()
                }
                .unwrap_or_default();
                ranked_key(&name, vec![name.clone()], rules)
            }
        },
        _ => SortKey {
            class: CLASS_MIDDLE,
            list_idx: 0,
            key: Vec::new(),
        },
    });

    let order = changed_order(&items)?;

    let single_line = !has_comments(&items, &dangling)
        && items[0].node.start_position().row == items[items.len() - 1].node.end_position().row;
    let sep = item_sep(&items, src, single_line);
    let mut blocks: Vec<String> = order
        .iter()
        .map(|&i| render_item(&items[i], src, path, cfg, &sep, ""))
        .collect();
    blocks.extend(dangling.iter().map(|c| text(*c, src).to_string()));
    Some(blocks.join(&sep))
}

pub(crate) fn inherit_names(node: Node, src: &str) -> Vec<String> {
    node.child_by_field_name("attrs")
        .map(|attrs| {
            let mut cursor = attrs.walk();
            attrs
                .named_children(&mut cursor)
                .filter(|c| c.kind() != "comment")
                .map(|c| normalize_key(c, src))
                .collect()
        })
        .unwrap_or_default()
}

fn render_sorted(
    node: Node,
    src: &str,
    path: &mut Vec<String>,
    cfg: &Config,
    (items, dangling): (Vec<Item>, Vec<Node>),
    (open, close): (char, char),
    comma: bool,
) -> Option<String> {
    let order = changed_order(&items)?;
    let single_line = !is_multiline(node) && !has_comments(&items, &dangling);
    let sep = item_sep(&items, src, single_line);
    let mut out = String::from(open);
    out.push_str(&sep);
    let blocks: Vec<String> = order
        .iter()
        .enumerate()
        .map(|(pos, &i)| {
            let suffix = if comma && pos + 1 < order.len() {
                ","
            } else {
                ""
            };
            render_item(&items[i], src, path, cfg, &sep, suffix)
        })
        .collect();
    out.push_str(&blocks.join(&sep));
    for c in &dangling {
        out.push_str(&sep);
        out.push_str(text(*c, src));
    }
    if single_line {
        out.push(' ');
        out.push(close);
    } else {
        out.push('\n');
        out.push_str(&line_indent_of(node, src));
        out.push(close);
    }
    Some(out)
}

fn sort_formals(node: Node, src: &str, path: &mut Vec<String>, cfg: &Config) -> Option<String> {
    let rules = cfg.rules_at(RuleKind::Args, path);
    if !rules.sort {
        return splice_children(node, src, path, cfg);
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node
        .children(&mut cursor)
        .filter(|c| matches!(c.kind(), "formal" | "ellipses" | "comment"))
        .collect();
    render_sorted(
        node,
        src,
        path,
        cfg,
        collect_items(children, |child| match child.kind() {
            "formal" => {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| text(n, src).to_string())
                    .unwrap_or_default();
                let mut key = ranked_key(&name, vec![name.clone()], &rules);
                if key.class == CLASS_MIDDLE && child.child_by_field_name("default").is_some() {
                    if let Some(idx) = rules.first.iter().position(|f| f == DEFAULTED_TOKEN) {
                        key.class = CLASS_FIRST;
                        key.list_idx = idx;
                    } else if let Some(idx) = rules.last.iter().position(|l| l == DEFAULTED_TOKEN) {
                        key.class = CLASS_LAST;
                        key.list_idx = idx;
                    }
                }
                key
            }
            _ => {
                let key = ranked_key("...", vec![String::from("...")], &rules);
                if key.class == CLASS_MIDDLE {
                    SortKey {
                        class: CLASS_ELLIPSES,
                        ..key
                    }
                } else {
                    key
                }
            }
        }),
        ('{', '}'),
        true,
    )
    .or_else(|| splice_children(node, src, path, cfg))
}

fn sort_list(node: Node, src: &str, path: &mut Vec<String>, cfg: &Config) -> Option<String> {
    let rules = cfg.rules_at(RuleKind::Lists, path);
    if !rules.sort {
        return splice_children(node, src, path, cfg);
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node
        .children(&mut cursor)
        .filter(|c| c.is_named())
        .collect();
    render_sorted(
        node,
        src,
        path,
        cfg,
        collect_items(children, |child| {
            let key = normalize_key(child, src);
            ranked_key(&key, vec![key.clone()], &rules)
        }),
        ('[', ']'),
        false,
    )
    .or_else(|| splice_children(node, src, path, cfg))
}

fn sort_inherited_attrs(
    node: Node,
    src: &str,
    path: &mut Vec<String>,
    cfg: &Config,
) -> Option<String> {
    let rules = cfg.rules_at(RuleKind::Inherits, path);
    if !rules.sort {
        return splice_children(node, src, path, cfg);
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    // Comments inside an inherit name list are rare and position-sensitive;
    // leave such lists untouched rather than risk mangling them.
    if children.iter().any(|c| c.kind() == "comment") {
        return splice_children(node, src, path, cfg);
    }
    let (items, _) = collect_items(children, |child| {
        let name = normalize_key(child, src);
        ranked_key(&name, vec![name.clone()], &rules)
    });
    let Some(order) = changed_order(&items) else {
        return splice_children(node, src, path, cfg);
    };
    let blocks: Vec<String> = order
        .iter()
        .map(|&i| render_item(&items[i], src, path, cfg, " ", ""))
        .collect();
    Some(blocks.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg(toml_str: &str) -> Config {
        toml::from_str(toml_str).unwrap()
    }

    fn sorted(src: &str, config: &Config) -> String {
        sort_source(src, config).unwrap()
    }

    #[test]
    fn sorts_attrs_alphabetically() {
        let out = sorted("{\n  b = 1;\n  a = 2;\n}\n", &Config::default());
        assert_eq!(out, "{\n  a = 2;\n  b = 1;\n}\n");
    }

    #[test]
    fn unchanged_input_is_untouched_bytewise() {
        let src = "{\n  a = 2;\n\n  # keeps blank lines and comments\n  b = 1;\n}\n";
        assert_eq!(sorted(src, &Config::default()), src);
    }

    #[test]
    fn single_line_stays_single_line() {
        let out = sorted("{ b = 1; a = 2; }\n", &Config::default());
        assert_eq!(out, "{ a = 2; b = 1; }\n");
    }

    #[test]
    fn first_and_last_lists() {
        let config = cfg(r#"
            [attrs]
            first = ["enable", "package"]
            last = ["extraConfig"]
        "#);
        let out = sorted(
            "{\n  extraConfig = 1;\n  alpha = 2;\n  package = 3;\n  enable = 4;\n  beta = 5;\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  enable = 4;\n  package = 3;\n  alpha = 2;\n  beta = 5;\n  extraConfig = 1;\n}\n"
        );
    }

    #[test]
    fn sorts_by_full_attrpath() {
        let out = sorted(
            "{\n  programs.git = 1;\n  programs.bash = 2;\n  lfs.enable = 3;\n}\n",
            &Config::default(),
        );
        assert_eq!(
            out,
            "{\n  lfs.enable = 3;\n  programs.bash = 2;\n  programs.git = 1;\n}\n"
        );
    }

    #[test]
    fn nested_attrsets_are_sorted() {
        let out = sorted(
            "{\n  a = {\n    z = 1;\n    y = [\n      {\n        d = 1;\n        c = 2;\n      }\n    ];\n  };\n}\n",
            &Config::default(),
        );
        assert_eq!(
            out,
            "{\n  a = {\n    y = [\n      {\n        c = 2;\n        d = 1;\n      }\n    ];\n    z = 1;\n  };\n}\n"
        );
    }

    #[test]
    fn comments_move_with_their_binding() {
        let out = sorted(
            "{\n  # about b\n  b = 1; # trailing b\n  a = 2;\n}\n",
            &Config::default(),
        );
        assert_eq!(out, "{\n  a = 2;\n  # about b\n  b = 1; # trailing b\n}\n");
    }

    #[test]
    fn inherits_go_first_keeping_relative_order() {
        let out = sorted(
            "{\n  a = 1;\n  inherit (x) z;\n  inherit y;\n}\n",
            &Config::default(),
        );
        assert_eq!(out, "{\n  inherit (x) z;\n  inherit y;\n  a = 1;\n}\n");
    }

    #[test]
    fn inherit_names_sorted_when_enabled() {
        let config = cfg("[inherits]\nsort = true\n");
        let out = sorted("{\n  inherit (x) c a b;\n}\n", &config);
        assert_eq!(out, "{\n  inherit (x) a b c;\n}\n");
    }

    #[test]
    fn formals_sorted_with_first_last_and_ellipsis() {
        let config = cfg(r#"
            [args]
            first = ["self", "lib", "config", "pkgs"]
            last = ["..."]
        "#);
        let out = sorted("{\n  pkgs,\n  minimal,\n  lib,\n  ...\n}:\n{\n}\n", &config);
        assert_eq!(out, "{\n  lib,\n  pkgs,\n  minimal,\n  ...\n}:\n{\n}\n");
    }

    #[test]
    fn formals_defaults_and_trailing_comments() {
        let out = sorted(
            "{\n  b ? null, # about b\n  a,\n  ...\n}:\na\n",
            &Config::default(),
        );
        assert_eq!(out, "{\n  a,\n  b ? null, # about b\n  ...\n}:\na\n");
    }

    #[test]
    fn single_line_formals() {
        let out = sorted("{ b, a, ... }: a\n", &Config::default());
        assert_eq!(out, "{ a, b, ... }: a\n");
    }

    #[test]
    fn lets_untouched_by_default_but_sortable() {
        let src = "let\n  b = 1;\n  a = 2;\nin\nb\n";
        assert_eq!(sorted(src, &Config::default()), src);
        let config = cfg("[lets]\nsort = true\n");
        assert_eq!(sorted(src, &config), "let\n  a = 2;\n  b = 1;\nin\nb\n");
    }

    #[test]
    fn inherit_placement_last_keeps_relative_order() {
        let config = cfg("inherit-placement = \"last\"\n");
        let out = sorted(
            "{\n  inherit y;\n  b = 1;\n  inherit (x) z;\n  a = 2;\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  a = 2;\n  b = 1;\n  inherit y;\n  inherit (x) z;\n}\n"
        );
        let config = cfg("inherit-placement = \"last\"\n[attrs]\nlast = [\"z\"]\n");
        let out = sorted("{\n  inherit y;\n  z = 1;\n  a = 2;\n}\n", &config);
        assert_eq!(out, "{\n  a = 2;\n  z = 1;\n  inherit y;\n}\n");
    }

    #[test]
    fn inherit_placement_sorted_keys_by_name() {
        let src = "{\n  d = 1;\n  inherit (x) c a;\n  b = 2;\n}\n";
        let config = cfg("inherit-placement = \"sorted\"\n");
        assert_eq!(
            sorted(src, &config),
            "{\n  b = 2;\n  inherit (x) c a;\n  d = 1;\n}\n"
        );
        let config = cfg("inherit-placement = \"sorted\"\n[inherits]\nsort = true\n");
        assert_eq!(
            sorted(src, &config),
            "{\n  inherit (x) a c;\n  b = 2;\n  d = 1;\n}\n"
        );
    }

    #[test]
    fn per_path_overrides_reach_all_rule_kinds() {
        let config = cfg("[[overrides]]\npath = \"f\"\nargs.sort = false\n");
        assert_eq!(
            sorted("{ f = { b, a }: a; f2 = { b, a }: a; }\n", &config),
            "{ f = { b, a }: a; f2 = { a, b }: a; }\n"
        );
        let config = cfg("[lets]\nsort = true\n[[overrides]]\npath = \"g\"\nlets.sort = false\n");
        assert_eq!(
            sorted(
                "{ g = let b = 1; a = 2; in a; g2 = let b = 1; a = 2; in a; }\n",
                &config
            ),
            "{ g = let b = 1; a = 2; in a; g2 = let a = 2; b = 1; in a; }\n"
        );
        let config =
            cfg("[inherits]\nsort = true\n[[overrides]]\npath = \"h\"\ninherits.sort = false\n");
        assert_eq!(
            sorted(
                "{ h = { inherit (x) c a; }; h2 = { inherit (x) c a; }; }\n",
                &config
            ),
            "{ h = { inherit (x) c a; }; h2 = { inherit (x) a c; }; }\n"
        );
    }

    #[test]
    fn defaulted_token_in_first_and_last() {
        let config = cfg("[args]\nfirst = [\"lib\", \"<defaulted>\"]\n");
        assert_eq!(
            sorted("{\n  b,\n  x ? 1,\n  lib,\n  a\n}:\na\n", &config),
            "{\n  lib,\n  x ? 1,\n  a,\n  b\n}:\na\n"
        );
        // In `last`, defaulted args group together (sorted by name), an
        // explicit `first` entry beats the token, and "..." stays behind.
        let config =
            cfg("[args]\nfirst = [\"lib\", \"nixosConfig\"]\nlast = [\"<defaulted>\", \"...\"]\n");
        assert_eq!(
            sorted(
                "{\n  nixosConfig ? null,\n  minimal,\n  globals ? duck,\n  x ? 1,\n  lib,\n  a,\n  ...\n}:\nlib\n",
                &config
            ),
            "{\n  lib,\n  nixosConfig ? null,\n  a,\n  minimal,\n  globals ? duck,\n  x ? 1,\n  ...\n}:\nlib\n"
        );
    }

    #[test]
    fn first_last_apply_to_lets_and_inherits() {
        let config = cfg("[lets]\nsort = true\nfirst = [\"z\"]\nlast = [\"a\"]\n");
        assert_eq!(
            sorted("let\n  b = 1;\n  a = 2;\n  z = 3;\nin\nb\n", &config),
            "let\n  z = 3;\n  b = 1;\n  a = 2;\nin\nb\n"
        );
        let config = cfg("[inherits]\nsort = true\nfirst = [\"z\"]\nlast = [\"a\"]\n");
        assert_eq!(
            sorted("{\n  inherit (x) a b z;\n}\n", &config),
            "{\n  inherit (x) z b a;\n}\n"
        );
    }

    #[test]
    fn per_path_override_disables_sorting() {
        let config = cfg(r#"
            [[overrides]]
            path = "**.alias"
            attrs.sort = false
        "#);
        let src = "{\n  prog.alias = {\n    s = 1;\n    a = 2;\n  };\n  z = 1;\n  b = 2;\n}\n";
        let out = sorted(src, &config);
        assert_eq!(
            out,
            "{\n  b = 2;\n  prog.alias = {\n    s = 1;\n    a = 2;\n  };\n  z = 1;\n}\n"
        );
    }

    #[test]
    fn per_path_override_changes_first() {
        let config = cfg(r#"
            [[overrides]]
            path = "services.*"
            attrs.first = ["description"]
        "#);
        let src = "{\n  services.foo = {\n    after = 1;\n    description = 2;\n  };\n}\n";
        let out = sorted(src, &config);
        assert_eq!(
            out,
            "{\n  services.foo = {\n    description = 2;\n    after = 1;\n  };\n}\n"
        );
    }

    #[test]
    fn quoted_keys_sort_by_inner_text() {
        let out = sorted("{\n  \"b\" = 1;\n  a = 2;\n}\n", &Config::default());
        assert_eq!(out, "{\n  a = 2;\n  \"b\" = 1;\n}\n");
    }

    #[test]
    fn rec_attrsets_are_sorted() {
        let out = sorted("rec {\n  b = a;\n  a = 1;\n}\n", &Config::default());
        assert_eq!(out, "rec {\n  a = 1;\n  b = a;\n}\n");
    }

    #[test]
    fn lists_sorted_only_when_enabled() {
        let src = "{\n  xs = [\n    \"c\"\n    \"a\"\n    \"b\"\n  ];\n}\n";
        assert_eq!(sorted(src, &Config::default()), src);
        let config = cfg("[lists]\nsort = true\n");
        assert_eq!(
            sorted(src, &config),
            "{\n  xs = [\n    \"a\"\n    \"b\"\n    \"c\"\n  ];\n}\n"
        );
    }

    #[test]
    fn single_line_list_with_first_and_last() {
        let config = cfg(r#"
            [lists]
            sort = true
            first = ["primary"]
            last = ["fallback"]
        "#);
        let out = sorted("{ xs = [ b fallback a primary ]; }\n", &config);
        assert_eq!(out, "{ xs = [ primary a b fallback ]; }\n");
    }

    #[test]
    fn list_sorting_per_path_override() {
        let config = cfg(r#"
            [[overrides]]
            path = "**.sortedList"
            lists.sort = true
        "#);
        let src = "{\n  sortedList = [ b a ];\n  keepList = [ b a ];\n}\n";
        let out = sorted(src, &config);
        assert_eq!(
            out,
            "{\n  keepList = [ b a ];\n  sortedList = [ a b ];\n}\n"
        );
    }

    #[test]
    fn list_comments_move_with_elements() {
        let config = cfg("[lists]\nsort = true\n");
        let out = sorted(
            "{\n  xs = [\n    # about b\n    b # trailing b\n    a\n  ];\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  xs = [\n    a\n    # about b\n    b # trailing b\n  ];\n}\n"
        );
    }

    #[test]
    fn nixpkgs_package_preset_orders_a_recipe() {
        let config = Config::from_toml_str("preset = \"nixpkgs-package\"").unwrap();
        let out = sorted(
            "{\n  stdenv,\n  fetchFromGitHub,\n  lib,\n  zlib,\n  enableFoo ? true,\n}:\nstdenv.mkDerivation {\n  meta = { license = 1; description = 2; };\n  src = fetchFromGitHub {\n    hash = \"x\";\n    repo = \"r\";\n    owner = \"o\";\n    rev = \"v\";\n  };\n  version = \"1.0\";\n  pname = \"foo\";\n  passthru = { };\n  buildInputs = [ zlib ];\n}\n",
            &config,
        );
        let pos = |needle: &str| {
            out.find(needle)
                .unwrap_or_else(|| panic!("missing {needle}"))
        };
        assert!(pos("lib,") < pos("stdenv,"));
        assert!(pos("stdenv,") < pos("fetchFromGitHub,"));
        assert!(pos("zlib,") < pos("enableFoo ? true"));
        assert!(pos("pname") < pos("version = \"1.0\""));
        assert!(pos("version = \"1.0\"") < pos("src = fetchFromGitHub"));
        assert!(pos("buildInputs") < pos("passthru"));
        assert!(pos("passthru") < pos("meta"));
        assert!(
            pos("owner") < pos("repo = ")
                && pos("repo = ") < pos("rev")
                && pos("rev") < pos("hash")
        );
        assert!(pos("description") < pos("license"));
    }

    #[test]
    fn nixos_module_preset_orders_module_args() {
        let config = Config::from_toml_str("preset = \"nixos-module\"").unwrap();
        let out = sorted(
            "{\n  pkgs,\n  lib,\n  myArg,\n  config,\n  ...\n}:\n{\n  config = { };\n  options = { };\n  imports = [ ];\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  config,\n  lib,\n  pkgs,\n  myArg,\n  ...\n}:\n{\n  imports = [ ];\n  options = { };\n  config = { };\n}\n"
        );
    }

    #[test]
    fn invalid_nix_is_rejected() {
        assert!(sort_source("{ a = ; }", &Config::default()).is_err());
    }
}
