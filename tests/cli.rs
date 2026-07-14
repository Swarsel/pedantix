use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const OFF: &str = r#"formatter = "off""#;

fn pedantix(args: &[&str], stdin: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pedantix"));
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    match stdin {
        Some(input) => {
            cmd.stdin(Stdio::piped());
            let mut child = cmd.spawn().unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        }
        None => {
            cmd.stdin(Stdio::null());
            cmd.output().unwrap()
        }
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pedantix-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn stdin_mode_formats_and_checks() {
    let out = pedantix(&["--config-toml", OFF], Some("{ b = 1; a = 2; }\n"));
    assert!(out.status.success());
    assert_eq!(stdout(&out), "{ a = 2; b = 1; }\n");

    let out = pedantix(&["--config-toml", OFF, "-"], Some("{ b = 1; a = 2; }\n"));
    assert_eq!(stdout(&out), "{ a = 2; b = 1; }\n");

    let out = pedantix(
        &["--config-toml", OFF, "--check"],
        Some("{ b = 1; a = 2; }\n"),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());

    let out = pedantix(
        &["--config-toml", OFF, "--check"],
        Some("{ a = 2; b = 1; }\n"),
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn file_mode_formats_in_place_with_discovered_config() {
    let dir = temp_dir("files");
    std::fs::write(
        dir.join("pedantix.toml"),
        "formatter = \"off\"\n\n[lets]\nmerge = true\n",
    )
    .unwrap();
    let one = dir.join("one.nix");
    let two = dir.join("two.nix");
    std::fs::write(&one, "{ b = 1; a = 2; }\n").unwrap();
    std::fs::write(&two, "{ d = 1; c = 2; }\n").unwrap();
    let (one, two) = (one.to_str().unwrap(), two.to_str().unwrap());

    let out = pedantix(&["--check", one, two], None);
    assert_eq!(out.status.code(), Some(1));
    let errs = stderr(&out);
    assert_eq!(errs.matches("would reformat").count(), 2, "{errs}");
    assert_eq!(
        errs.matches("`lets.merge` has no effect").count(),
        1,
        "{errs}"
    );

    assert!(pedantix(&[one, two], None).status.success());
    assert_eq!(std::fs::read_to_string(one).unwrap(), "{ a = 2; b = 1; }\n");
    assert_eq!(std::fs::read_to_string(two).unwrap(), "{ c = 2; d = 1; }\n");
    assert_eq!(
        pedantix(&["--check", one, two], None).status.code(),
        Some(0)
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stdin_filepath_selects_config_and_flake_spacing() {
    let dir = temp_dir("flake");
    std::fs::write(
        dir.join("pedantix.toml"),
        "formatter = \"off\"\ntop-level-blank-lines = 1\n",
    )
    .unwrap();
    let src = "{\n  outputs =\n    { self }:\n    {\n      p = 1;\n      q = {\n        r = 2;\n      };\n    };\n}\n";
    let as_flake = pedantix(
        &["--stdin-filepath", dir.join("flake.nix").to_str().unwrap()],
        Some(src),
    );
    let as_module = pedantix(
        &["--stdin-filepath", dir.join("module.nix").to_str().unwrap()],
        Some(src),
    );
    assert!(stdout(&as_flake).contains("p = 1;\n\n"));
    assert_eq!(stdout(&as_module), src);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn formatter_flag_overrides_formatter_command() {
    let broken = r#"formatter-command = ["pedantix-missing-base-formatter"]"#;
    let out = pedantix(
        &["--config-toml", broken, "--formatter", "off"],
        Some("{ b = 1; a = 2; }\n"),
    );
    assert!(out.status.success());
    assert_eq!(stdout(&out), "{ a = 2; b = 1; }\n");

    let out = pedantix(&["--config-toml", broken], Some("{ a = 1; }\n"));
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("is it on PATH"));
}

#[test]
fn discovered_formatter_command_is_not_executed() {
    let dir = temp_dir("untrusted");
    let config = dir.join("pedantix.toml");
    std::fs::write(&config, "formatter-command = [\"cat\"]\n").unwrap();
    let file = dir.join("f.nix");
    std::fs::write(&file, "{ b = 1; a = 2; }\n").unwrap();
    let (config, file) = (config.to_str().unwrap(), file.to_str().unwrap());

    let out = pedantix(&[file], None);
    assert_eq!(out.status.code(), Some(2));
    let errs = stderr(&out);
    assert!(errs.contains("--allow-formatter-command"), "{errs}");
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        "{ b = 1; a = 2; }\n"
    );

    let out = pedantix(
        &["--stdin-filepath", file, "--check"],
        Some("{ b = 1; a = 2; }\n"),
    );
    assert_eq!(out.status.code(), Some(2));

    for trusted in [
        vec!["--allow-formatter-command", file],
        vec!["--formatter", "off", file],
        vec!["--set", r#"formatter-command=["cat"]"#, file],
        vec!["--config", config, file],
    ] {
        std::fs::write(file, "{ b = 1; a = 2; }\n").unwrap();
        let out = pedantix(&trusted, None);
        assert!(out.status.success(), "{trusted:?}: {}", stderr(&out));
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            "{ a = 2; b = 1; }\n",
            "{trusted:?}"
        );
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn global_config_formatter_command_is_executed() {
    let dir = temp_dir("xdg-command");
    let cfg_dir = dir.join("pedantix");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(
        cfg_dir.join("pedantix.toml"),
        "formatter-command = [\"cat\"]\n",
    )
    .unwrap();
    let file = dir.join("f.nix");
    std::fs::write(&file, "{ b = 1; a = 2; }\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pedantix"))
        .env("XDG_CONFIG_HOME", &dir)
        .arg(&file)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "{ a = 2; b = 1; }\n"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn invalid_nix_exits_2() {
    let out = pedantix(&["--config-toml", OFF], Some("{ a = ; }\n"));
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("not valid Nix"));
}
