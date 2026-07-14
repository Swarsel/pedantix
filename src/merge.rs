use crate::config::{Config, RuleKind};
use crate::sort::{
    Item, collect_items, container_region, descend_binding, inherit_names, item_sep, normalize_key,
    parse, render_item_with, splice_children_with, splice_region, text,
};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, HashSet};
use tree_sitter::Node;

pub fn merge_source(src: &str, cfg: &Config) -> Result<String> {
    let mut current = src.to_string();
    loop {
        let tree = parse(&current)?;
        let root = tree.root_node();
        if root.has_error() {
            bail!("input is not valid Nix (parse error); refusing to merge");
        }
        let mut path: Vec<String> = Vec::new();
        match rewrite(root, &current, &mut path, cfg) {
            Some(next) => current = next,
            None => return Ok(current),
        }
    }
}

fn rewrite(node: Node, src: &str, path: &mut Vec<String>, cfg: &Config) -> Option<String> {
    match node.kind() {
        "attrset_expression" | "rec_attrset_expression" | "let_attrset_expression" => {
            merge_container(node, src, path, cfg)
        }
        "binding" => descend_binding(node, src, path, |child, path| {
            rewrite(child, src, path, cfg)
        }),
        _ => splice_children(node, src, path, cfg),
    }
}

fn splice_children(node: Node, src: &str, path: &mut Vec<String>, cfg: &Config) -> Option<String> {
    splice_children_with(node, src, |child| rewrite(child, src, path, cfg))
}

fn attrpath_comp_nodes<'a>(binding: Node<'a>) -> Option<Vec<Node<'a>>> {
    let ap = binding.child_by_field_name("attrpath")?;
    let mut cursor = ap.walk();
    let comps: Vec<Node> = ap.named_children(&mut cursor).collect();
    if comps.iter().any(|c| c.kind() == "comment") {
        return None;
    }
    Some(comps)
}

// Heads with interpolations are dynamic attributes; two bindings sharing
// one are an eval error, so merging them would change the file's meaning.
fn is_static_head(node: Node) -> bool {
    match node.kind() {
        "identifier" => true,
        "string_expression" => {
            let mut cursor = node.walk();
            let dynamic = node
                .children(&mut cursor)
                .any(|c| c.kind() == "interpolation");
            !dynamic
        }
        _ => false,
    }
}

fn merge_container(node: Node, src: &str, path: &mut Vec<String>, cfg: &Config) -> Option<String> {
    let rules = cfg.rules_at(RuleKind::Attrs, path);
    if !rules.merge {
        return splice_children(node, src, path, cfg);
    }
    let Some((flat, region_start, region_end)) = container_region(node) else {
        return splice_children(node, src, path, cfg);
    };
    let (items, dangling) = collect_items(flat, |_| ());

    let mut heads: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut blocked: HashSet<String> = HashSet::new();
    for (idx, item) in items.iter().enumerate() {
        match item.node.kind() {
            "binding" => match attrpath_comp_nodes(item.node) {
                Some(comps) if comps.len() >= 2 => {
                    if is_static_head(comps[0]) {
                        heads
                            .entry(normalize_key(comps[0], src))
                            .or_default()
                            .push(idx);
                    }
                }
                Some(comps) => {
                    if let Some(head) = comps.first() {
                        blocked.insert(normalize_key(*head, src));
                    }
                }
                None => {}
            },
            "inherit" | "inherit_from" => {
                for name in inherit_names(item.node, src) {
                    blocked.insert(name);
                }
            }
            _ => {}
        }
    }
    heads.retain(|head, idxs| idxs.len() >= 2 && !blocked.contains(head));
    if heads.is_empty() {
        return splice_children(node, src, path, cfg);
    }

    let mut merged_at: BTreeMap<usize, &Vec<usize>> = BTreeMap::new();
    let mut absorbed: HashSet<usize> = HashSet::new();
    for idxs in heads.values() {
        merged_at.insert(idxs[0], idxs);
        absorbed.extend(&idxs[1..]);
    }

    let sep = item_sep(&items, src, false);
    let inner_sep = format!("{sep}  ");
    let mut blocks: Vec<String> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if absorbed.contains(&idx) {
            continue;
        }
        if let Some(group) = merged_at.get(&idx) {
            blocks.push(render_merged(
                &items,
                group.as_slice(),
                src,
                &sep,
                &inner_sep,
            ));
        } else {
            blocks.push(render_item_with(item, src, &sep, "", |child| {
                rewrite(child, src, path, cfg)
            }));
        }
    }
    blocks.extend(dangling.iter().map(|c| text(*c, src).to_string()));
    let region_text = blocks.join(&sep);
    Some(splice_region(
        node,
        src,
        region_start,
        region_end,
        &region_text,
        |child| rewrite(child, src, path, cfg),
    ))
}

fn render_merged(
    items: &[Item<()>],
    group: &[usize],
    src: &str,
    sep: &str,
    inner_sep: &str,
) -> String {
    let first_comps = attrpath_comp_nodes(items[group[0]].node).expect("mergeable binding");
    let head_text = text(first_comps[0], src);
    let mut inner: Vec<String> = Vec::new();
    for &i in group {
        let item = &items[i];
        let comps = attrpath_comp_nodes(item.node).expect("mergeable binding");
        inner.push(render_item_with(item, src, inner_sep, "", |node| {
            Some(src[comps[1].start_byte()..node.end_byte()].to_string())
        }));
    }
    format!(
        "{head_text} = {{{inner_sep}{}{sep}}};",
        inner.join(inner_sep)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg(toml_str: &str) -> Config {
        toml::from_str(toml_str).unwrap()
    }

    fn merged(src: &str, config: &Config) -> String {
        merge_source(src, config).unwrap()
    }

    #[test]
    fn merges_shared_heads() {
        let config = cfg("[attrs]\nmerge = true\n");
        let out = merged(
            "{\n  programs.emacs.enable = true;\n  programs.kitty.enable = true;\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  programs = {\n    emacs.enable = true;\n    kitty.enable = true;\n  };\n}\n"
        );
    }

    #[test]
    fn off_by_default() {
        let src = "{\n  a.b = 1;\n  a.c = 2;\n}\n";
        assert_eq!(merged(src, &Config::default()), src);
    }

    #[test]
    fn single_occurrence_is_untouched() {
        let config = cfg("[attrs]\nmerge = true\n");
        let src = "{\n  a.b = 1;\n  c.d = 2;\n}\n";
        assert_eq!(merged(src, &config), src);
    }

    #[test]
    fn merges_recursively() {
        let config = cfg("[attrs]\nmerge = true\n");
        let out = merged("{\n  a.b.c = 1;\n  a.b.d = 2;\n}\n", &config);
        assert_eq!(
            out,
            "{\n  a = {\n    b = {\n      c = 1;\n      d = 2;\n    };\n  };\n}\n"
        );
    }

    #[test]
    fn plain_binding_blocks_merge() {
        let config = cfg("[attrs]\nmerge = true\n");
        let src = "{\n  a = 1;\n  a.b = 2;\n  a.c = 3;\n}\n";
        assert_eq!(merged(src, &config), src);
    }

    #[test]
    fn inherit_blocks_merge() {
        let config = cfg("[attrs]\nmerge = true\n");
        let src = "{\n  inherit a;\n  a.b = 1;\n  a.c = 2;\n}\n";
        assert_eq!(merged(src, &config), src);
    }

    #[test]
    fn dynamic_heads_are_skipped() {
        let config = cfg("[attrs]\nmerge = true\n");
        let src = "{\n  ${x}.a = 1;\n  ${x}.b = 2;\n}\n";
        assert_eq!(merged(src, &config), src);
    }

    #[test]
    fn interpolated_string_heads_are_not_merged() {
        let config = cfg("[attrs]\nmerge = true\n");
        let src = "{\n  \"a${x}\".b = 1;\n  \"a${x}\".c = 2;\n}\n";
        assert_eq!(merged(src, &config), src);
    }

    #[test]
    fn comments_move_into_the_merged_set() {
        let config = cfg("[attrs]\nmerge = true\n");
        let out = merged(
            "{\n  # about b\n  a.b = 1; # trailing b\n  a.c = 2;\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  a = {\n    # about b\n    b = 1; # trailing b\n    c = 2;\n  };\n}\n"
        );
    }

    #[test]
    fn quoted_head_merges_with_identifier_head() {
        let config = cfg("[attrs]\nmerge = true\n");
        let out = merged("{\n  \"a\".b = 1;\n  a.c = 2;\n}\n", &config);
        assert_eq!(out, "{\n  \"a\" = {\n    b = 1;\n    c = 2;\n  };\n}\n");
    }

    #[test]
    fn other_bindings_keep_their_place() {
        let config = cfg("[attrs]\nmerge = true\n");
        let out = merged(
            "{\n  z = 0;\n  a.b = 1;\n  q = 9;\n  a.c = 2;\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  z = 0;\n  a = {\n    b = 1;\n    c = 2;\n  };\n  q = 9;\n}\n"
        );
    }

    #[test]
    fn per_path_override_disables_merge() {
        let config =
            cfg("[attrs]\nmerge = true\n[[overrides]]\npath = \"**.keep\"\nattrs.merge = false\n");
        let out = merged(
            "{\n  keep = {\n    a.b = 1;\n    a.c = 2;\n  };\n  x.y = 1;\n  x.z = 2;\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  keep = {\n    a.b = 1;\n    a.c = 2;\n  };\n  x = {\n    y = 1;\n    z = 2;\n  };\n}\n"
        );
    }

    #[test]
    fn comments_in_attrpaths_merge_conservatively() {
        let config = cfg("[attrs]\nmerge = true\n");
        let out = merged("{\n  a/*x*/.b = 1;\n  a.c = 2;\n  a.d = 3;\n}\n", &config);
        assert_eq!(
            out,
            "{\n  a/*x*/.b = 1;\n  a = {\n    c = 2;\n    d = 3;\n  };\n}\n"
        );
        assert!(!crate::sort::parse(&out).unwrap().root_node().has_error());

        let src = "{\n  a/*x*/ = 1;\n  a.b = 2;\n  a.c = 3;\n}\n";
        assert_eq!(merged(src, &config), src);
    }

    #[test]
    fn single_line_input_stays_valid() {
        let config = cfg("[attrs]\nmerge = true\n");
        let out = merged("{ a.b = 1; a.c = 2; }\n", &config);
        assert!(!crate::sort::parse(&out).unwrap().root_node().has_error());
        assert!(out.contains("a = {"));
    }

    #[test]
    fn invalid_nix_is_rejected() {
        let config = cfg("[attrs]\nmerge = true\n");
        assert!(merge_source("{ a = ; }", &config).is_err());
    }
}
