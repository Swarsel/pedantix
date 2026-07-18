use std::fs;
use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("presets directory")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();

    for (i, path) in files.iter().enumerate() {
        if i > 0 {
            println!();
        }
        let name = path.file_stem().unwrap().to_string_lossy();
        let (prose, body) = split(&fs::read_to_string(path).expect("read preset"));
        println!("## `{name}`\n");
        if !prose.is_empty() {
            println!("{prose}\n");
        }
        println!("```toml\n{body}\n```");
    }
}

fn split(text: &str) -> (String, String) {
    let mut prose: Vec<&str> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.peek() {
        match line.trim_start().strip_prefix('#') {
            Some(comment) => {
                prose.push(comment.strip_prefix(' ').unwrap_or(comment));
                lines.next();
            }
            None => break,
        }
    }
    let body: Vec<&str> = lines.collect();
    (
        prose.join("\n").trim().to_string(),
        body.join("\n").trim().to_string(),
    )
}
