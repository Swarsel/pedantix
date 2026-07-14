use crate::config::Config;
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn run_base_formatter(cfg: &Config, input: &str) -> Result<String> {
    let argv = cfg.formatter_argv();
    if argv.is_empty() {
        return Ok(input.to_string());
    }
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "failed to run base formatter `{}` (is it on PATH?)",
                argv[0]
            )
        })?;

    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input_owned = input.to_string();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        stdin.write_all(input_owned.as_bytes())
    });
    let output = child
        .wait_with_output()
        .with_context(|| format!("base formatter `{}` failed to run", argv[0]))?;
    // A broken-pipe write error is subsumed by the formatter's exit status.
    let _ = writer.join().expect("stdin writer thread panicked");

    if !output.status.success() {
        bail!(
            "base formatter `{}` exited with {}:\n{}",
            argv.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        );
    }
    String::from_utf8(output.stdout)
        .with_context(|| format!("base formatter `{}` produced invalid UTF-8", argv[0]))
}
