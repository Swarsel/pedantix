//! Tests needing a base formatter are skipped when it is not on PATH

use pedantix::config::{BlankLinesMode, Config, FormatterChoice};
use pedantix::pipeline::{process, process_file};
use pedantix::semantic::fingerprint;
use std::path::Path;

const EXAMPLE: &str = include_str!("../example.nix");

fn have(binary: &str) -> bool {
    std::process::Command::new(binary)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn repo_config() -> Config {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/pedantix.toml"))
        .expect("repo pedantix.toml");
    Config::from_toml_str(&text).expect("repo pedantix.toml is valid")
}

#[test]
fn example_content_is_preserved_and_formatting_is_idempotent() {
    if !have("nixfmt") {
        eprintln!("skipping: nixfmt not on PATH");
        return;
    }
    for cfg in [Config::default(), repo_config()] {
        let once = process(EXAMPLE, &cfg).unwrap();
        assert_eq!(
            fingerprint(EXAMPLE).unwrap(),
            fingerprint(&once).unwrap(),
            "content changed with config {cfg:?}"
        );
        assert_eq!(
            once,
            process(&once, &cfg).unwrap(),
            "not idempotent with config {cfg:?}"
        );
    }
}

#[test]
fn example_args_and_attrs_are_ordered() {
    if !have("nixfmt") {
        eprintln!("skipping: nixfmt not on PATH");
        return;
    }
    let out = process(EXAMPLE, &repo_config()).unwrap();
    let lib_pos = out.find("\n      lib,").expect("lib arg");
    let config_pos = out.find("\n      config,").expect("config arg");
    let ellipsis_pos = out.find("...").expect("ellipsis");
    assert!(lib_pos < config_pos && config_pos < ellipsis_pos);
    let git = out.find("programs.git = {").expect("programs.git");
    assert!(
        out[git..]
            .trim_start_matches("programs.git = {")
            .trim_start()
            .starts_with("enable = true;")
    );
    let a = out.find("a = \"add\";").expect("alias a");
    let b = out.find("b = \"branch\";").expect("alias b");
    let c = out.find("c = \"commit\";").expect("alias c");
    assert!(a < b && b < c);
}

#[test]
fn alejandra_and_nixpkgs_fmt_work_as_base() {
    for (name, choice) in [
        ("alejandra", FormatterChoice::Alejandra),
        ("nixpkgs-fmt", FormatterChoice::NixpkgsFmt),
    ] {
        if !have(name) {
            eprintln!("skipping: {name} not on PATH");
            continue;
        }
        let cfg = Config {
            formatter: choice,
            ..Config::default()
        };
        let out = process(EXAMPLE, &cfg).unwrap();
        assert_eq!(fingerprint(EXAMPLE).unwrap(), fingerprint(&out).unwrap());
        assert_eq!(out, process(&out, &cfg).unwrap(), "{name} not idempotent");
    }
}

#[test]
fn merge_through_the_full_pipeline() {
    if !have("nixfmt") {
        eprintln!("skipping: nixfmt not on PATH");
        return;
    }
    let cfg: Config = toml::from_str("[attrs]\nmerge = true\n").unwrap();
    let src = "{\n  programs.emacs.enable = true;\n  services.foo.port = 1;\n  programs.kitty.enable = true;\n  services.foo.address = \"::\";\n}\n";
    let out = process(src, &cfg).unwrap();
    assert_eq!(
        out,
        "{\n  programs = {\n    emacs.enable = true;\n    kitty.enable = true;\n  };\n  services = {\n    foo = {\n      address = \"::\";\n      port = 1;\n    };\n  };\n}\n"
    );
    assert_eq!(out, process(&out, &cfg).unwrap(), "merge not idempotent");
}

#[test]
fn flatten_through_the_full_pipeline() {
    if !have("nixfmt") {
        eprintln!("skipping: nixfmt not on PATH");
        return;
    }
    let cfg: Config = toml::from_str("[attrs]\nflatten = true\n").unwrap();
    let src = "{\n  a = {\n    b = {\n      c = 1;\n    };\n  };\n  d = {\n    e = 2;\n    f = 3;\n  };\n}\n";
    let out = process(src, &cfg).unwrap();
    assert_eq!(
        out,
        "{\n  a.b.c = 1;\n  d = {\n    e = 2;\n    f = 3;\n  };\n}\n"
    );
    assert_eq!(out, process(&out, &cfg).unwrap(), "flatten not idempotent");
}

#[test]
fn example_content_is_preserved_with_flatten() {
    if !have("nixfmt") {
        eprintln!("skipping: nixfmt not on PATH");
        return;
    }
    let mut cfg = repo_config();
    cfg.attrs.flatten = true;
    let out = process(EXAMPLE, &cfg).unwrap();
    assert_eq!(
        pedantix::semantic::fingerprint_with(EXAMPLE, false, true).unwrap(),
        pedantix::semantic::fingerprint_with(&out, false, true).unwrap()
    );
    assert_eq!(out, process(&out, &cfg).unwrap());
}

#[test]
fn example_content_is_preserved_with_merge() {
    if !have("nixfmt") {
        eprintln!("skipping: nixfmt not on PATH");
        return;
    }
    let mut cfg = repo_config();
    cfg.attrs.merge = true;
    let out = process(EXAMPLE, &cfg).unwrap();
    assert_eq!(
        pedantix::semantic::fingerprint_with(EXAMPLE, false, true).unwrap(),
        pedantix::semantic::fingerprint_with(&out, false, true).unwrap()
    );
    assert_eq!(out, process(&out, &cfg).unwrap());
}

#[test]
fn top_level_spacing_through_the_full_pipeline() {
    if !have("nixfmt") {
        eprintln!("skipping: nixfmt not on PATH");
        return;
    }
    let src = "{\n  b = 2;\n\n\n  a = 1;\n  c = { y = 2; x = 1; };\n}\n";
    for n in [0, 1, 2] {
        let cfg = Config {
            top_level_blank_lines: Some(n),
            ..Config::default()
        };
        let out = process(src, &cfg).unwrap();
        let gap = "\n".repeat(n + 1);
        assert_eq!(
            out,
            format!("{{\n  a = 1;\n  b = 2;{gap}  c = {{\n    x = 1;\n    y = 2;\n  }};\n}}\n"),
            "n = {n}"
        );
        assert_eq!(out, process(&out, &cfg).unwrap(), "n = {n} not idempotent");
    }

    let cfg = Config {
        top_level_blank_lines: Some(1),
        top_level_blank_lines_mode: BlankLinesMode::All,
        ..Config::default()
    };
    let out = process(src, &cfg).unwrap();
    assert_eq!(
        out,
        "{\n  a = 1;\n\n  b = 2;\n\n  c = {\n    x = 1;\n    y = 2;\n  };\n}\n"
    );
    assert_eq!(out, process(&out, &cfg).unwrap(), "all mode not idempotent");
}

#[test]
fn flake_files_space_the_outputs_body() {
    let cfg: Config = toml::from_str("formatter = \"off\"\ntop-level-blank-lines = 1").unwrap();
    let src = "{\n  description = \"d\";\n\n  inputs = {\n    a.url = \"u\";\n    b.url = \"v\";\n  };\n\n  outputs =\n    { self }:\n    {\n      p = 1;\n      q = {\n        r = 2;\n      };\n    };\n}\n";
    let as_flake = process_file(src, &cfg, Some(Path::new("flake.nix"))).unwrap();
    assert_eq!(
        as_flake,
        "{\n  description = \"d\";\n\n  inputs = {\n    a.url = \"u\";\n    b.url = \"v\";\n  };\n\n  outputs =\n    { self }:\n    {\n      p = 1;\n\n      q = {\n        r = 2;\n      };\n    };\n}\n"
    );
    assert_eq!(
        process_file(src, &cfg, Some(Path::new("module.nix"))).unwrap(),
        src
    );
}

#[test]
fn formatter_command_is_used_and_failures_are_reported() {
    let cfg: Config = toml::from_str(r#"formatter-command = ["cat"]"#).unwrap();
    let src = "{ b = 1; a = 2; }\n";
    assert_eq!(process(src, &cfg).unwrap(), "{ a = 2; b = 1; }\n");

    let cfg = Config {
        formatter_command: Some(vec!["false".into()]),
        ..Config::default()
    };
    let err = format!("{:#}", process(src, &cfg).unwrap_err());
    assert!(err.contains("exited with"), "{err}");

    let cfg = Config {
        formatter_command: Some(vec!["pedantix-missing-base-formatter".into()]),
        ..Config::default()
    };
    let err = format!("{:#}", process(src, &cfg).unwrap_err());
    assert!(err.contains("is it on PATH"), "{err}");

    let cfg = Config {
        formatter_command: Some(vec!["printf".into(), r"\xff".into()]),
        ..Config::default()
    };
    let err = format!("{:#}", process(src, &cfg).unwrap_err());
    assert!(err.contains("invalid UTF-8"), "{err}");
}

#[test]
fn format_passes_can_be_disabled() {
    let base = Config {
        formatter_command: Some(vec!["sed".into(), "1s/^/# marked\\n/".into()]),
        ..Config::default()
    };
    let src = "{ b = 1; a = 2; }\n";
    assert_eq!(
        process(src, &base).unwrap(),
        "# marked\n# marked\n{ a = 2; b = 1; }\n"
    );
    assert_eq!(
        process("{ a = 2; b = 1; }\n", &base).unwrap(),
        "# marked\n{ a = 2; b = 1; }\n"
    );
    let no_before = Config {
        format_before_sort: false,
        ..base.clone()
    };
    assert_eq!(
        process(src, &no_before).unwrap(),
        "# marked\n{ a = 2; b = 1; }\n"
    );
    let no_after = Config {
        format_after_sort: false,
        ..base.clone()
    };
    assert_eq!(
        process(src, &no_after).unwrap(),
        "# marked\n{ a = 2; b = 1; }\n"
    );
    let neither = Config {
        format_before_sort: false,
        format_after_sort: false,
        ..base
    };
    assert_eq!(process(src, &neither).unwrap(), "{ a = 2; b = 1; }\n");
}
