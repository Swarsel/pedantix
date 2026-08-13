use crate::config::{BlankLinesMode, Config, RuleKind};
use crate::syntax::{
    binding_components, collect_items, container_region, descend_binding, is_multiline, parse_valid,
};
use anyhow::Result;
use tree_sitter::Node;

pub fn space_top_level(src: &str, cfg: &Config, is_flake: bool) -> Result<String> {
    if !cfg.blank_lines_may_apply() {
        return Ok(src.to_string());
    }
    let tree = parse_valid(src, "adjust spacing")?;
    let mut spacer = Spacer {
        src,
        cfg,
        is_flake,
        edits: Vec::new(),
    };
    let mut path: Vec<String> = Vec::new();
    spacer.walk(tree.root_node(), &mut path, Some(1), None);
    spacer.edits.sort_by_key(|&(start, _, _)| start);
    let mut out = String::new();
    let mut pos = 0;
    for (start, end, replacement) in spacer.edits {
        out.push_str(&src[pos..start]);
        out.push_str(&replacement);
        pos = end;
    }
    out.push_str(&src[pos..]);
    Ok(out)
}

struct Spacer<'a> {
    src: &'a str,
    cfg: &'a Config,
    is_flake: bool,
    edits: Vec<(usize, usize, String)>,
}

#[derive(Clone, Copy)]
struct Inherited {
    n: usize,
    mode: BlankLinesMode,
    window: usize,
}

impl Spacer<'_> {
    fn walk(
        &mut self,
        node: Node,
        path: &mut Vec<String>,
        root: Option<usize>,
        inherited: Option<Inherited>,
    ) {
        match node.kind() {
            "attrset_expression" | "rec_attrset_expression" | "let_attrset_expression" => {
                self.space_container(node, path, root, inherited);
            }
            "binding" => self.walk_binding(node, path, None, None),
            "source_code" | "parenthesized_expression" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.walk(child, path, root, inherited);
                }
            }
            "function_expression" | "let_expression" | "with_expression" | "assert_expression" => {
                let body_id = node.child_by_field_name("body").map(|b| b.id());
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    let (child_root, child_inherited) = if Some(child.id()) == body_id {
                        (root, inherited)
                    } else {
                        (None, None)
                    };
                    self.walk(child, path, child_root, child_inherited);
                }
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.walk(child, path, None, None);
                }
            }
        }
    }

    fn walk_binding(
        &mut self,
        node: Node,
        path: &mut Vec<String>,
        root: Option<usize>,
        inherited: Option<Inherited>,
    ) {
        let expr_id = node.child_by_field_name("expression").map(|e| e.id());
        descend_binding(node, self.src, path, |child, path| {
            if Some(child.id()) == expr_id {
                self.walk(child, path, root, inherited);
            } else {
                self.walk(child, path, None, None);
            }
            None
        });
    }

    fn space_container(
        &mut self,
        node: Node,
        path: &mut Vec<String>,
        root: Option<usize>,
        inherited: Option<Inherited>,
    ) {
        let Some((flat, _, _)) = container_region(node) else {
            return;
        };
        let (items, _) = collect_items(flat, |_| ());

        let rules = self.cfg.rules_at(RuleKind::Attrs, path);
        let blank_lines = rules
            .blank_lines
            .or(inherited.map(|inh| inh.n))
            .or(root.and(self.cfg.top_level_blank_lines));
        let mode = match (rules.blank_lines_mode, rules.blank_lines, inherited) {
            (Some(mode), _, _) => mode,
            (None, Some(_), _) => BlankLinesMode::Multiline,
            (None, None, Some(inh)) => inh.mode,
            (None, None, None) => self.cfg.top_level_blank_lines_mode,
        };
        let child_inherited = match rules.blank_lines {
            Some(n) => (rules.blank_lines_depth > 1).then(|| Inherited {
                n,
                mode,
                window: rules.blank_lines_depth - 1,
            }),
            None => inherited.and_then(|inh| {
                (inh.window > 1).then(|| Inherited {
                    window: inh.window - 1,
                    ..inh
                })
            }),
        };
        if let Some(n) = blank_lines
            && mode != BlankLinesMode::Off
        {
            for pair in items.windows(2) {
                let prev_end = pair[0]
                    .trailing
                    .map(|t| t.end_byte())
                    .unwrap_or(pair[0].node.end_byte());
                let next_start = pair[1]
                    .leading
                    .first()
                    .map(|c| c.start_byte())
                    .unwrap_or(pair[1].node.start_byte());
                let gap = &self.src[prev_end..next_start];
                let Some(last_newline) = gap.rfind('\n') else {
                    continue;
                };
                let gap_lines = match mode {
                    BlankLinesMode::All => n,
                    BlankLinesMode::Multiline => {
                        if is_multiline(pair[0].node) || is_multiline(pair[1].node) {
                            n
                        } else {
                            0
                        }
                    }
                    BlankLinesMode::Off => unreachable!("checked above"),
                };
                let eol = if gap[..last_newline].ends_with('\r') {
                    "\r\n"
                } else {
                    "\n"
                };
                let replacement =
                    format!("{}{}", eol.repeat(gap_lines + 1), &gap[last_newline + 1..]);
                self.edits.push((prev_end, next_start, replacement));
            }
        }

        for item in &items {
            if item.node.kind() != "binding" {
                self.walk(item.node, path, None, None);
                continue;
            }
            let comps = binding_components(item.node, self.src);
            let flake_body =
                self.is_flake && root == Some(1) && (comps == ["inputs"] || comps == ["outputs"]);
            let child_root = if flake_body {
                Some(1)
            } else {
                root.and_then(|depth| {
                    (depth < self.cfg.top_level_blank_lines_depth).then_some(depth + 1)
                })
            };
            self.walk_binding(item.node, path, child_root, child_inherited);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(n: usize) -> Config {
        Config {
            top_level_blank_lines: Some(n),
            ..Config::default()
        }
    }

    fn cfg_all(n: usize) -> Config {
        Config {
            top_level_blank_lines_mode: BlankLinesMode::All,
            ..cfg(n)
        }
    }

    fn spaced(src: &str, config: &Config) -> String {
        space_top_level(src, config, false).unwrap()
    }

    fn spaced_flake(src: &str, config: &Config) -> String {
        space_top_level(src, config, true).unwrap()
    }

    #[test]
    fn unset_leaves_input_untouched() {
        let src = "{\n  a = 1;\n\n  b = 2;\n}\n";
        assert_eq!(spaced(src, &Config::default()), src);
    }

    #[test]
    fn single_line_bindings_stay_adjacent() {
        let out = spaced("{\n  a = 1;\n  b = 2;\n\n\n  c = 3;\n}\n", &cfg(1));
        assert_eq!(out, "{\n  a = 1;\n  b = 2;\n  c = 3;\n}\n");
    }

    #[test]
    fn multiline_bindings_get_surrounded() {
        let out = spaced(
            "{\n  a = 1;\n  b = {\n    x = 1;\n    y = 2;\n  };\n  c = 3;\n  d = 4;\n}\n",
            &cfg(1),
        );
        assert_eq!(
            out,
            "{\n  a = 1;\n\n  b = {\n    x = 1;\n    y = 2;\n  };\n\n  c = 3;\n  d = 4;\n}\n"
        );
    }

    #[test]
    fn all_mode_spaces_every_gap() {
        let out = spaced("{\n  a = 1;\n  b = 2;\n\n\n  c = 3;\n}\n", &cfg_all(1));
        assert_eq!(out, "{\n  a = 1;\n\n  b = 2;\n\n  c = 3;\n}\n");
    }

    #[test]
    fn zero_removes_blank_lines() {
        let out = spaced("{\n  a = 1;\n\n  b = {\n    x = 1;\n  };\n}\n", &cfg(0));
        assert_eq!(out, "{\n  a = 1;\n  b = {\n    x = 1;\n  };\n}\n");
    }

    #[test]
    fn dynamic_attrpaths_are_spaced_like_let_bindings() {
        let body = "${f {\n    aa = {\n      p = 2;\n    };\n\n    bb = 1;\n  }} = 1;";
        let src = |open: &str, close: &str| {
            format!(
                "{open}\n  ${{f {{\n    aa = {{\n      p = 2;\n    }};\n    bb = 1;\n  }}}} = 1;\n{close}\n"
            )
        };
        let config: Config = toml::from_str("[attrs]\nblank-lines = 1\n").unwrap();
        assert_eq!(
            spaced(&src("{", "}"), &config),
            format!("{{\n  {body}\n}}\n")
        );
        assert_eq!(
            spaced(&src("let", "in 1"), &config),
            format!("let\n  {body}\nin 1\n")
        );
    }

    #[test]
    fn crlf_line_endings_survive_spacing() {
        let src = "{\r\n  a = 1;\r\n  b = {\r\n    x = 1;\r\n  };\r\n}\r\n";
        assert_eq!(
            spaced(src, &cfg(1)),
            "{\r\n  a = 1;\r\n\r\n  b = {\r\n    x = 1;\r\n  };\r\n}\r\n"
        );
        let blanked = "{\r\n  a = 1;\r\n\r\n  b = {\r\n    x = 1;\r\n  };\r\n}\r\n";
        assert_eq!(
            spaced(blanked, &cfg(0)),
            "{\r\n  a = 1;\r\n  b = {\r\n    x = 1;\r\n  };\r\n}\r\n"
        );
        let lf = "{\n  a = 1;\n  b = {\n    x = 1;\n  };\n}\n";
        assert_eq!(
            spaced(lf, &cfg(1)),
            "{\n  a = 1;\n\n  b = {\n    x = 1;\n  };\n}\n"
        );
    }

    #[test]
    fn unwraps_to_a_module_body() {
        let out = spaced(
            "{ lib, ... }:\n{\n  a = 1;\n  b = [\n    1\n    2\n  ];\n}\n",
            &cfg(1),
        );
        assert_eq!(
            out,
            "{ lib, ... }:\n{\n  a = 1;\n\n  b = [\n    1\n    2\n  ];\n}\n"
        );
    }

    #[test]
    fn default_depth_leaves_nested_bodies_alone() {
        let src =
            "{\n  outputs = { self }: {\n    p = 1;\n    q = {\n      r = 2;\n    };\n  };\n}\n";
        assert_eq!(spaced(src, &cfg(1)), src);
    }

    #[test]
    fn raised_depth_spaces_the_body_of_a_binding() {
        let mut config = cfg(1);
        config.top_level_blank_lines_depth = 2;
        let out = spaced(
            "{\n  a = 1;\n  b =\n    { self }:\n    let\n      x = 1;\n    in\n    {\n      p = 1;\n      q = {\n        r = 2;\n      };\n      s = 3;\n    };\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  a = 1;\n\n  b =\n    { self }:\n    let\n      x = 1;\n    in\n    {\n      p = 1;\n\n      q = {\n        r = 2;\n      };\n\n      s = 3;\n    };\n}\n"
        );
    }

    #[test]
    fn depth_does_not_descend_into_function_calls() {
        let mut config = cfg(1);
        config.top_level_blank_lines_depth = 2;
        let src =
            "{\n  apps = mkApps (s: {\n    p = 1;\n    q = {\n      r = 2;\n    };\n  });\n}\n";
        assert_eq!(spaced(src, &config), src);
    }

    #[test]
    fn flake_inputs_and_outputs_bodies_count_as_top_level() {
        let src = "{\n  description = \"d\";\n  inputs = {\n    a.url = \"u\";\n    b = {\n      url = \"v\";\n    };\n  };\n  outputs =\n    { self }:\n    {\n      p = 1;\n      q = {\n        r = 2;\n      };\n      s = 3;\n    };\n}\n";
        let out = spaced_flake(src, &cfg(1));
        assert_eq!(
            out,
            "{\n  description = \"d\";\n\n  inputs = {\n    a.url = \"u\";\n\n    b = {\n      url = \"v\";\n    };\n  };\n\n  outputs =\n    { self }:\n    {\n      p = 1;\n\n      q = {\n        r = 2;\n      };\n\n      s = 3;\n    };\n}\n"
        );
        assert_eq!(
            spaced(src, &cfg(1)),
            "{\n  description = \"d\";\n\n  inputs = {\n    a.url = \"u\";\n    b = {\n      url = \"v\";\n    };\n  };\n\n  outputs =\n    { self }:\n    {\n      p = 1;\n      q = {\n        r = 2;\n      };\n      s = 3;\n    };\n}\n"
        );
    }

    #[test]
    fn blank_line_goes_before_leading_comments() {
        let out = spaced(
            "{\n  a = 1; # trailing\n  # about b\n  b = {\n    x = 1;\n  };\n}\n",
            &cfg(1),
        );
        assert_eq!(
            out,
            "{\n  a = 1; # trailing\n\n  # about b\n  b = {\n    x = 1;\n  };\n}\n"
        );
    }

    #[test]
    fn single_line_sets_are_left_alone() {
        let src = "{ a = 1; b = 2; }\n";
        assert_eq!(spaced(src, &cfg(1)), src);
        assert_eq!(spaced(src, &cfg_all(1)), src);
    }

    #[test]
    fn non_attrset_files_are_left_alone() {
        let src = "[\n  1\n  2\n]\n";
        assert_eq!(spaced(src, &cfg(1)), src);
    }

    #[test]
    fn override_enables_spacing_on_a_nested_set() {
        let config: Config =
            toml::from_str("[[overrides]]\npath = \"ff.inputs\"\nattrs.blank-lines = 1\n").unwrap();
        let src = "{\n  a = 1;\n  ff.inputs = {\n    x = {\n      u = 1;\n    };\n    y = {\n      u = 2;\n    };\n  };\n}\n";
        let out = spaced(src, &config);
        assert_eq!(
            out,
            "{\n  a = 1;\n  ff.inputs = {\n    x = {\n      u = 1;\n    };\n\n    y = {\n      u = 2;\n    };\n  };\n}\n"
        );
    }

    #[test]
    fn override_applies_behind_let_bindings() {
        let config: Config =
            toml::from_str("[[overrides]]\npath = \"foo\"\nattrs.blank-lines = 1\n").unwrap();
        let out = spaced(
            "let\n  foo = {\n    p = 1;\n    q = {\n      r = 2;\n    };\n  };\nin\nfoo\n",
            &config,
        );
        assert_eq!(
            out,
            "let\n  foo = {\n    p = 1;\n\n    q = {\n      r = 2;\n    };\n  };\nin\nfoo\n"
        );
    }

    #[test]
    fn anchored_override_sees_full_path_behind_let() {
        let config: Config =
            toml::from_str("[[overrides]]\npath = \"a\"\nattrs.blank-lines = 1\n").unwrap();
        let src = "let\n  foo = {\n    a = {\n      p = 1;\n      q = {\n        r = 2;\n      };\n    };\n  };\nin\nfoo\n";
        assert_eq!(spaced(src, &config), src);
    }

    #[test]
    fn let_attrset_bodies_are_spaced() {
        let config: Config = toml::from_str("[attrs]\nblank-lines = 1\n").unwrap();
        let out = spaced(
            "let {\n  body = a;\n  a = {\n    x = 1;\n  };\n}\n",
            &config,
        );
        assert_eq!(out, "let {\n  body = a;\n\n  a = {\n    x = 1;\n  };\n}\n");
    }

    #[test]
    fn override_changes_the_count_of_a_root() {
        let config: Config = toml::from_str(
            "top-level-blank-lines = 1\ntop-level-blank-lines-depth = 2\n\n[[overrides]]\npath = \"b\"\nattrs.blank-lines = 2\n",
        )
        .unwrap();
        let src = "{\n  a = 1;\n  b = {\n    p = 1;\n    q = {\n      r = 2;\n    };\n  };\n}\n";
        let out = spaced(src, &config);
        assert_eq!(
            out,
            "{\n  a = 1;\n\n  b = {\n    p = 1;\n\n\n    q = {\n      r = 2;\n    };\n  };\n}\n"
        );
    }

    #[test]
    fn off_override_disables_spacing_for_a_root() {
        let config: Config = toml::from_str(
            "top-level-blank-lines = 1\ntop-level-blank-lines-depth = 2\n\n[[overrides]]\npath = \"b\"\nattrs.blank-lines-mode = \"off\"\n",
        )
        .unwrap();
        let src = "{\n  a = 1;\n  b = {\n    p = 1;\n    q = {\n      r = 2;\n    };\n  };\n}\n";
        let out = spaced(src, &config);
        assert_eq!(
            out,
            "{\n  a = 1;\n\n  b = {\n    p = 1;\n    q = {\n      r = 2;\n    };\n  };\n}\n"
        );
    }

    #[test]
    fn override_depth_spaces_nested_levels_below_the_match() {
        let config: Config = toml::from_str(
            "[[overrides]]\npath = \"o\"\nattrs = { blank-lines = 1, blank-lines-depth = 2 }\n",
        )
        .unwrap();
        let src = "{\n  o = {\n    l1a = {\n      l2a = {\n        m = 1;\n        n = {\n          z = 9;\n        };\n      };\n      l2b = 1;\n    };\n    l1b = 1;\n  };\n}\n";
        let out = spaced(src, &config);
        assert_eq!(
            out,
            "{\n  o = {\n    l1a = {\n      l2a = {\n        m = 1;\n        n = {\n          z = 9;\n        };\n      };\n\n      l2b = 1;\n    };\n\n    l1b = 1;\n  };\n}\n"
        );
    }

    #[test]
    fn top_level_mode_off_and_rules_level_mode_all() {
        let src = "{\n  a = 1;\n\n\n  b = {\n    x = 1;\n  };\n}\n";
        let config = Config {
            top_level_blank_lines: Some(1),
            top_level_blank_lines_mode: BlankLinesMode::Off,
            ..Config::default()
        };
        assert_eq!(spaced(src, &config), src);

        let config: Config =
            toml::from_str("[attrs]\nblank-lines = 1\nblank-lines-mode = \"all\"\n").unwrap();
        let out = spaced(
            "{\n  a = 1;\n  b = 2;\n  c = {\n    x = 1;\n    y = 2;\n  };\n}\n",
            &config,
        );
        assert_eq!(
            out,
            "{\n  a = 1;\n\n  b = 2;\n\n  c = {\n    x = 1;\n\n    y = 2;\n  };\n}\n"
        );
    }

    #[test]
    fn global_attrs_blank_lines_applies_at_every_depth() {
        let config: Config = toml::from_str("[attrs]\nblank-lines = 1\n").unwrap();
        let src = "{\n  a = 1;\n  b = {\n    p = 1;\n    q = {\n      r = 2;\n    };\n  };\n}\n";
        let out = spaced(src, &config);
        assert_eq!(
            out,
            "{\n  a = 1;\n\n  b = {\n    p = 1;\n\n    q = {\n      r = 2;\n    };\n  };\n}\n"
        );
    }
}
