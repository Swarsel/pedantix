use crate::syntax::{binding_components, named_non_comment_children, parse, static_name};
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use tree_sitter::Node;

#[derive(Clone, Copy)]
struct Opts {
    unordered_lists: bool,
    flatten_attrs: bool,
    unstyled_names: bool,
}

pub fn fingerprint(src: &str) -> Result<String> {
    fingerprint_with(src, false, false, false)
}

pub fn fingerprint_with(
    src: &str,
    unordered_lists: bool,
    flatten_attrs: bool,
    unstyled_names: bool,
) -> Result<String> {
    let opts = Opts {
        unordered_lists,
        flatten_attrs,
        unstyled_names,
    };
    let tree = parse(src)?;
    let mut out = String::new();
    canon(tree.root_node(), src, &mut out, opts);
    let mut comments = Vec::new();
    collect_comments(tree.root_node(), src, &mut comments);
    comments.sort();
    out.push_str("\n#comments:");
    for c in comments {
        out.push('\n');
        out.push_str(c.trim_end());
    }
    Ok(out)
}

pub fn check_same_content(
    before: &str,
    after: &str,
    unordered_lists: bool,
    flatten_attrs: bool,
    unstyled_names: bool,
) -> Result<()> {
    if fingerprint_with(before, unordered_lists, flatten_attrs, unstyled_names)?
        != fingerprint_with(after, unordered_lists, flatten_attrs, unstyled_names)?
    {
        bail!(
            "internal error: reordering would have changed the file's content; \
             refusing to write output. Please report this as a pedantix bug."
        );
    }
    Ok(())
}

fn canon(node: Node, src: &str, out: &mut String, opts: Opts) {
    if node.kind() == "indented_string_expression" {
        canon_indented_string(node, src, out, opts);
        return;
    }
    if opts.unstyled_names && matches!(node.kind(), "attrpath" | "inherited_attrs") {
        out.push('(');
        out.push_str(node.kind());
        let mut parts: Vec<String> = named_non_comment_children(node)
            .into_iter()
            .map(|c| match static_name(c, src) {
                Some(name) => format!("(name {name})"),
                None => {
                    let mut s = String::new();
                    canon(c, src, &mut s, opts);
                    s
                }
            })
            .collect();
        if node.kind() == "inherited_attrs" {
            parts.sort();
        }
        for p in parts {
            out.push_str(&p);
        }
        out.push(')');
        return;
    }
    if opts.flatten_attrs && node.kind() == "binding_set" {
        out.push_str("(binding_set");
        write_entry(set_entry(node, src, opts), out);
        out.push(')');
        return;
    }
    let children = named_non_comment_children(node);
    let order_insensitive = matches!(node.kind(), "binding_set" | "formals" | "inherited_attrs")
        || (opts.unordered_lists && node.kind() == "list_expression");
    out.push('(');
    out.push_str(node.kind());
    if node.child_count() == 0 {
        out.push(' ');
        out.push_str(&src[node.start_byte()..node.end_byte()]);
    } else if order_insensitive {
        let mut parts: Vec<String> = children
            .into_iter()
            .map(|c| {
                let mut s = String::new();
                canon(c, src, &mut s, opts);
                s
            })
            .collect();
        parts.sort();
        for p in parts {
            out.push_str(&p);
        }
    } else {
        let mut cursor = node.walk();
        for c in node.children(&mut cursor) {
            if !c.is_named() {
                out.push_str("(t ");
                out.push_str(&src[c.start_byte()..c.end_byte()]);
                out.push(')');
            } else if c.kind() != "comment" {
                canon(c, src, out, opts);
            }
        }
    }
    out.push(')');
}

fn canon_indented_string(node: Node, src: &str, out: &mut String, opts: Opts) {
    const PLACEHOLDER: char = '\u{0}';
    let mut template = String::new();
    let mut interpolations = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "string_fragment" | "escape_sequence" => {
                template.push_str(&src[child.start_byte()..child.end_byte()]);
            }
            "interpolation" => {
                template.push(PLACEHOLDER);
                let mut s = String::new();
                canon(child, src, &mut s, opts);
                interpolations.push(s);
            }
            _ => {}
        }
    }
    out.push_str("(indented_string ");
    out.push_str(&strip_common_indent(&template));
    for i in interpolations {
        out.push_str("(interp ");
        out.push_str(&i);
        out.push(')');
    }
    out.push(')');
}

fn strip_common_indent(s: &str) -> String {
    let leading_spaces = |line: &str| line.chars().take_while(|&c| c == ' ').count();
    let mut lines = s.split('\n');
    let mut out = lines.next().unwrap_or("").to_string();
    let rest: Vec<&str> = lines.collect();
    let min = rest
        .iter()
        .filter(|line| line.chars().any(|c| c != ' '))
        .map(|line| leading_spaces(line))
        .min()
        .unwrap_or(0);
    for line in rest {
        out.push('\n');
        out.push_str(&line[leading_spaces(line).min(min)..]);
    }
    out
}

#[derive(Default)]
struct Entry {
    values: Vec<String>,
    opaque: Vec<String>,
    children: BTreeMap<String, Entry>,
}

fn set_entry(binding_set: Node, src: &str, opts: Opts) -> Entry {
    let mut entry = Entry::default();
    for child in named_non_comment_children(binding_set) {
        if child.kind() == "binding" {
            let comps = binding_components(child, src);
            if let Some(value) = child.child_by_field_name("expression") {
                add(&mut entry, &comps, value, src, opts);
            }
        } else {
            let mut s = String::new();
            canon(child, src, &mut s, opts);
            entry.opaque.push(s);
        }
    }
    entry
}

fn add(entry: &mut Entry, comps: &[String], value: Node, src: &str, opts: Opts) {
    let mut cur = entry;
    for comp in comps {
        cur = cur.children.entry(comp.clone()).or_default();
    }
    if value.kind() == "attrset_expression" {
        if let Some(bs) = named_non_comment_children(value)
            .into_iter()
            .find(|c| c.kind() == "binding_set")
        {
            absorb(cur, set_entry(bs, src, opts));
        }
    } else {
        let mut s = String::new();
        canon(value, src, &mut s, opts);
        cur.values.push(s);
    }
}

fn absorb(dst: &mut Entry, src_entry: Entry) {
    dst.values.extend(src_entry.values);
    dst.opaque.extend(src_entry.opaque);
    for (key, child) in src_entry.children {
        absorb(dst.children.entry(key).or_default(), child);
    }
}

fn write_entry(entry: Entry, out: &mut String) {
    let Entry {
        mut values,
        mut opaque,
        children,
    } = entry;
    out.push('(');
    values.sort();
    for v in values {
        out.push_str("(v ");
        out.push_str(&v);
        out.push(')');
    }
    opaque.sort();
    for o in opaque {
        out.push_str("(i ");
        out.push_str(&o);
        out.push(')');
    }
    for (key, child) in children {
        out.push_str("(k ");
        out.push_str(&key);
        write_entry(child, out);
        out.push(')');
    }
    out.push(')');
}

fn collect_comments<'a>(node: Node<'a>, src: &'a str, out: &mut Vec<&'a str>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "comment" {
            out.push(&src[child.start_byte()..child.end_byte()]);
        } else {
            collect_comments(child, src, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_same_content_refuses_a_changed_file() {
        check_same_content(
            "{ a = 1; b = 2; }",
            "{ b = 2; a = 1; }",
            false,
            false,
            false,
        )
        .unwrap();
        let err = check_same_content("{ a = 1; }", "{ a = 2; }", false, false, false)
            .expect_err("changed value must be refused")
            .to_string();
        assert!(
            err.contains("would have changed the file's content"),
            "{err}"
        );
        check_same_content(
            "{ xs = [ 1 2 ]; }",
            "{ xs = [ 2 1 ]; }",
            false,
            false,
            false,
        )
        .expect_err("list reorder must be refused unless flagged");
        check_same_content("{ xs = [ 1 2 ]; }", "{ xs = [ 2 1 ]; }", true, false, false).unwrap();
    }

    #[test]
    fn reordering_is_equal_content_is_not() {
        assert_eq!(
            fingerprint("{ a = 1; b = 2; }").unwrap(),
            fingerprint("{\n  b = 2;\n  a = 1;\n}").unwrap()
        );
        assert_ne!(
            fingerprint("{ a = 1; }").unwrap(),
            fingerprint("{ a = 2; }").unwrap()
        );
        assert_ne!(
            fingerprint("{ a = 1; # hi\n}").unwrap(),
            fingerprint("{ a = 1; }").unwrap()
        );
    }

    #[test]
    fn empty_collections_ignore_interior_whitespace() {
        assert_eq!(
            fingerprint("{ p = x or {}; }").unwrap(),
            fingerprint("{ p = x or { }; }").unwrap()
        );
        assert_eq!(fingerprint("[]").unwrap(), fingerprint("[ ]").unwrap());
        assert_eq!(
            fingerprint("{}: rec {}").unwrap(),
            fingerprint("{ }: rec { }").unwrap()
        );
        assert_ne!(fingerprint("{ }").unwrap(), fingerprint("[ ]").unwrap());
    }

    #[test]
    fn operators_are_distinguished() {
        assert_ne!(fingerprint("a + b").unwrap(), fingerprint("a - b").unwrap());
        assert_ne!(fingerprint("!a").unwrap(), fingerprint("-a").unwrap());
    }

    #[test]
    fn list_order_matters_unless_flagged() {
        assert_ne!(
            fingerprint("[ 1 2 ]").unwrap(),
            fingerprint("[ 2 1 ]").unwrap()
        );
        assert_eq!(
            fingerprint_with("[ 1 2 ]", true, false, false).unwrap(),
            fingerprint_with("[ 2 1 ]", true, false, false).unwrap()
        );
    }

    #[test]
    fn formals_and_inherits_are_order_insensitive() {
        assert_eq!(
            fingerprint("{ a, b, ... }: { inherit (x) c d; }").unwrap(),
            fingerprint("{ b, a, ... }: { inherit (x) d c; }").unwrap()
        );
    }

    #[test]
    fn nesting_matters_unless_flagged() {
        assert_ne!(
            fingerprint("{ a.b = 1; a.c = 2; }").unwrap(),
            fingerprint("{ a = { b = 1; c = 2; }; }").unwrap()
        );
        assert_eq!(
            fingerprint_with("{ a.b = 1; a.c = 2; }", false, true, false).unwrap(),
            fingerprint_with("{ a = { b = 1; c = 2; }; }", false, true, false).unwrap()
        );
        assert_eq!(
            fingerprint_with("{ a.b.c = 1; }", false, true, false).unwrap(),
            fingerprint_with("{ a = { b = { c = 1; }; }; }", false, true, false).unwrap()
        );
    }

    #[test]
    fn flattening_still_detects_content_changes() {
        assert_ne!(
            fingerprint_with("{ a.b = 1; }", false, true, false).unwrap(),
            fingerprint_with("{ a.b = 2; }", false, true, false).unwrap()
        );
        assert_ne!(
            fingerprint_with("{ a.b = 1; }", false, true, false).unwrap(),
            fingerprint_with("{ a.c = 1; }", false, true, false).unwrap()
        );
        assert_ne!(
            fingerprint_with("{ a = rec { b = 1; }; }", false, true, false).unwrap(),
            fingerprint_with("{ a.b = 1; }", false, true, false).unwrap()
        );
        assert_ne!(
            fingerprint_with("{ a = { }; }", false, true, false).unwrap(),
            fingerprint_with("{ }", false, true, false).unwrap()
        );
    }

    #[test]
    fn indented_string_reindentation_is_equal_content_is_not() {
        assert_eq!(
            fingerprint("{ a = ''\n      foo\n    '';\n}").unwrap(),
            fingerprint("{ a = ''\n    foo\n  '';\n}").unwrap()
        );
        assert_ne!(
            fingerprint("''\n  foo\n    bar\n''").unwrap(),
            fingerprint("''\n  foo\n  bar\n''").unwrap()
        );
        assert_ne!(
            fingerprint("''\n  foo\n    \n''").unwrap(),
            fingerprint("''\n  foo\n  \n''").unwrap()
        );
    }

    #[test]
    fn name_styles_match_unless_flagged() {
        let names = |src: &str| fingerprint_with(src, false, false, true).unwrap();
        assert_ne!(
            fingerprint("{ \"a\" = 1; }").unwrap(),
            fingerprint("{ a = 1; }").unwrap()
        );
        assert_eq!(names("{ \"a\" = 1; }"), names("{ a = 1; }"));
        assert_eq!(
            names("{ inherit (x) \"a\" b; }"),
            names("{ inherit (x) b a; }")
        );
        assert_ne!(names("{ \"a b\" = 1; }"), names("{ ab = 1; }"));
        assert_eq!(
            fingerprint_with("{ \"a${[ 1 2 ]}\" = 1; }", true, false, true).unwrap(),
            fingerprint_with("{ \"a${[ 2 1 ]}\" = 1; }", true, false, true).unwrap()
        );
    }
}
