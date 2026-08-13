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

pub(crate) fn parse_valid(src: &str, action: &str) -> Result<tree_sitter::Tree> {
    let tree = parse(src)?;
    if tree.root_node().has_error() {
        bail!("input is not valid Nix (parse error); refusing to {action}");
    }
    Ok(tree)
}

pub(crate) fn rewrite_source<F>(src: &str, action: &str, mut rewrite: F) -> Result<String>
where
    F: FnMut(Node, &mut Vec<String>) -> Option<String>,
{
    let tree = parse_valid(src, action)?;
    let mut path = Vec::new();
    Ok(rewrite(tree.root_node(), &mut path).unwrap_or_else(|| src.to_string()))
}

pub(crate) fn text<'a>(node: Node, src: &'a str) -> &'a str {
    &src[node.start_byte()..node.end_byte()]
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
    named_non_comment_children(attrpath)
        .into_iter()
        .map(|c| normalize_key(c, src))
        .collect()
}

pub(crate) fn named_non_comment_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .collect()
}

pub(crate) fn unquote(s: &str) -> Option<&str> {
    s.strip_prefix('"')?.strip_suffix('"')
}

pub(crate) fn normalize_key(node: Node, src: &str) -> String {
    normalize_name(node, text(node, src))
}

pub(crate) fn normalize_name(node: Node, raw: &str) -> String {
    if node.kind() == "string_expression"
        && let Some(inner) = unquote(raw)
    {
        return inner.to_string();
    }
    raw.to_string()
}

pub(crate) fn static_name(node: Node, src: &str) -> Option<String> {
    let raw = text(node, src);
    match node.kind() {
        "identifier" => Some(raw.to_string()),
        "string_expression" => {
            let mut cursor = node.walk();
            if node
                .children(&mut cursor)
                .any(|c| c.kind() == "interpolation")
            {
                return None;
            }
            Some(unquote(raw)?.to_string())
        }
        _ => None,
    }
}

pub(crate) fn inherit_names(node: Node, src: &str) -> Vec<String> {
    node.child_by_field_name("attrs")
        .map(|attrs| {
            named_non_comment_children(attrs)
                .into_iter()
                .map(|c| normalize_key(c, src))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) struct Item<'a, K> {
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

pub(crate) fn line_indent_of(node: Node, src: &str) -> String {
    src[line_start_of(node, src)..]
        .chars()
        .take_while(|&c| c == ' ' || c == '\t')
        .collect()
}

pub(crate) fn is_multiline(node: Node) -> bool {
    node.start_position().row != node.end_position().row
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
