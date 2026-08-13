use crate::config::{Config, NameStyle, RuleKind};
use crate::syntax::{
    descend_binding, normalize_key, rewrite_source, splice_children_with, text, unquote,
};
use anyhow::Result;
use tree_sitter::Node;

const KEYWORDS: &[&str] = &[
    "assert", "else", "if", "in", "inherit", "let", "or", "rec", "then", "with",
];

pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '\'' | '-'))
        && !KEYWORDS.contains(&s)
}

pub fn restyle_source(src: &str, cfg: &Config) -> Result<String> {
    rewrite_source(src, "restyle names", |node, path| {
        rewrite(node, src, path, cfg)
    })
}

fn rewrite(node: Node, src: &str, path: &mut Vec<String>, cfg: &Config) -> Option<String> {
    match node.kind() {
        "binding" => {
            let kind = binding_kind(node);
            descend_binding(node, src, path, |child, path| match (kind, child.kind()) {
                (Some(kind), "attrpath") => restyle_attrpath(child, src, path, cfg, kind),
                _ => rewrite(child, src, path, cfg),
            })
        }
        "inherited_attrs" => {
            let style = cfg.rules_at(RuleKind::Inherits, path).name_style;
            splice_children_with(node, src, |child| {
                restyle(child, src, style).or_else(|| rewrite(child, src, path, cfg))
            })
        }
        _ => splice_children_with(node, src, |child| rewrite(child, src, path, cfg)),
    }
}

fn binding_kind(node: Node) -> Option<RuleKind> {
    match node.parent().and_then(|p| p.parent())?.kind() {
        "let_expression" => Some(RuleKind::Lets),
        "attrset_expression" | "rec_attrset_expression" | "let_attrset_expression" => {
            Some(RuleKind::Attrs)
        }
        _ => None,
    }
}

fn restyle_attrpath(
    node: Node,
    src: &str,
    path: &mut Vec<String>,
    cfg: &Config,
    kind: RuleKind,
) -> Option<String> {
    let depth = path.len();
    let out = splice_children_with(node, src, |child| {
        if !child.is_named() || child.kind() == "comment" {
            return None;
        }
        let style = cfg.rules_at(kind, path).name_style;
        let result = restyle(child, src, style).or_else(|| rewrite(child, src, path, cfg));
        path.push(normalize_key(child, src));
        result
    });
    path.truncate(depth);
    out
}

fn restyle(node: Node, src: &str, style: NameStyle) -> Option<String> {
    let raw = text(node, src);
    match style {
        NameStyle::Preserve => None,
        NameStyle::Identifier => {
            if node.kind() != "string_expression" {
                return None;
            }
            let inner = unquote(raw)?;
            is_identifier(inner).then(|| inner.to_string())
        }
        NameStyle::String => {
            (node.kind() == "identifier" && is_identifier(raw)).then(|| format!("\"{raw}\""))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg(toml_str: &str) -> Config {
        toml::from_str(toml_str).unwrap()
    }

    fn restyled(src: &str, config: &Config) -> String {
        restyle_source(src, config).unwrap()
    }

    #[test]
    fn identifier_charset_rejects_edge_cases() {
        for invalid in ["", "-a", "'a", "or", "ä"] {
            assert!(!is_identifier(invalid), "{invalid}");
        }
    }

    #[test]
    fn identifier_style_unquotes_valid_names() {
        let config = cfg("[attrs]\nname-style = \"identifier\"\n");
        let out = restyled(
            "{\n  \"valid\" = true;\n  \"not valid\" = false;\n  \"_underscore\" = true;\n  \"123abc\" = false;\n  \"abc123\" = true;\n  \"kebab-case\" = true;\n  \"apostrophe's_are_valid\" = true;\n  \"if\" = false;\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  valid = true;\n  \"not valid\" = false;\n  _underscore = true;\n  \"123abc\" = false;\n  abc123 = true;\n  kebab-case = true;\n  apostrophe's_are_valid = true;\n  \"if\" = false;\n}\n"
        );
    }

    #[test]
    fn off_by_default() {
        let src = "{\n  \"a\" = 1;\n  b = 2;\n}\n";
        assert_eq!(restyled(src, &Config::default()), src);
    }

    #[test]
    fn every_attrpath_component_is_restyled() {
        let config = cfg("[attrs]\nname-style = \"identifier\"\n");
        let out = restyled("{\n  \"a\".\"b c\".\"d\" = 1;\n}\n", &config);
        assert_eq!(out, "{\n  a.\"b c\".d = 1;\n}\n");
    }

    #[test]
    fn dynamic_names_are_untouched() {
        let config = cfg("[attrs]\nname-style = \"identifier\"\n");
        for src in ["{\n  \"a${x}\" = 1;\n}\n", "{\n  ${x} = 1;\n}\n"] {
            assert_eq!(restyled(src, &config), src);
        }
    }

    #[test]
    fn string_style_quotes_names() {
        let config = cfg("[attrs]\nname-style = \"string\"\n");
        let out = restyled("{\n  a = 1;\n  \"b c\" = 2;\n  ${x} = 3;\n}\n", &config);
        assert_eq!(out, "{\n  \"a\" = 1;\n  \"b c\" = 2;\n  ${x} = 3;\n}\n");
    }

    #[test]
    fn inherit_names_follow_the_inherits_rules() {
        let config = cfg("[inherits]\nname-style = \"identifier\"\n");
        let out = restyled(
            "{\n  inherit \"a\" \"b c\";\n  inherit (x) \"d\";\n}\n",
            &config,
        );
        assert_eq!(out, "{\n  inherit a \"b c\";\n  inherit (x) d;\n}\n");
        let config = cfg("[attrs]\nname-style = \"identifier\"\n");
        let src = "{\n  inherit (x) \"d\";\n}\n";
        assert_eq!(restyled(src, &config), src);
    }

    #[test]
    fn let_names_follow_the_lets_rules() {
        let config = cfg("[lets]\nname-style = \"identifier\"\n");
        let out = restyled("let\n  \"a\" = 1;\nin\na\n", &config);
        assert_eq!(out, "let\n  a = 1;\nin\na\n");
        let config = cfg("[attrs]\nname-style = \"identifier\"\n");
        let src = "let\n  \"a\" = 1;\nin\na\n";
        assert_eq!(restyled(src, &config), src);
    }

    #[test]
    fn nested_values_and_interpolations_are_reached() {
        let config = cfg("[attrs]\nname-style = \"identifier\"\n");
        let out = restyled(
            "{\n  a = {\n    \"b\" = 1;\n  };\n  ${f {\n    \"c\" = 2;\n  }} = 3;\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  a = {\n    b = 1;\n  };\n  ${f {\n    c = 2;\n  }} = 3;\n}\n"
        );
    }

    #[test]
    fn select_expressions_are_untouched() {
        let config = cfg("[attrs]\nname-style = \"identifier\"\n");
        let src = "{\n  x = a.\"b\";\n}\n";
        assert_eq!(restyled(src, &config), src);
    }

    #[test]
    fn per_path_overrides_toggle_the_style() {
        let config = cfg(
            "[attrs]\nname-style = \"identifier\"\n[[overrides]]\npath = \"**.keep\"\nattrs.name-style = \"preserve\"\n",
        );
        let out = restyled(
            "{\n  keep = {\n    \"a\" = 1;\n  };\n  other = {\n    \"b\" = 2;\n  };\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  keep = {\n    \"a\" = 1;\n  };\n  other = {\n    b = 2;\n  };\n}\n"
        );
    }

    #[test]
    fn overrides_resolve_per_attrpath_component() {
        let config = cfg(
            "[attrs]\nname-style = \"identifier\"\n[[overrides]]\npath = \"**.users.users\"\nattrs.name-style = \"string\"\n",
        );
        for (src, want) in [
            (
                "{\n  users.users.bob.isNormalUser = true;\n}\n",
                "{\n  users.users.\"bob\".isNormalUser = true;\n}\n",
            ),
            (
                "{\n  users.users = {\n    bob = {\n      \"isNormalUser\" = true;\n    };\n  };\n}\n",
                "{\n  users.users = {\n    \"bob\" = {\n      isNormalUser = true;\n    };\n  };\n}\n",
            ),
        ] {
            assert_eq!(restyled(src, &config), want);
        }
    }

    #[test]
    fn invalid_nix_is_rejected() {
        let config = cfg("[attrs]\nname-style = \"identifier\"\n");
        assert!(restyle_source("{ a = ; }", &config).is_err());
    }
}
