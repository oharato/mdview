//! End-to-end CLI tests for the Rust implementation.

use std::path::PathBuf;
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mdview")
}

fn demo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/demo.md")
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("run mdview")
}

#[test]
fn prints_version() {
    let output = run(&["--version"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("mdview "));
}

#[test]
fn renders_demo_without_colour() {
    let output = run(&["-c", demo().to_str().expect("path")]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("┄┄┄表"));
    assert!(stdout.contains("Kitty graphics"));
    assert!(stdout.contains("[画像: mermaid]"));
    assert!(!stdout.contains("\x1b["), "no ANSI expected with -c");
}

#[test]
fn lists_toc() {
    let output = run(&["--toc", demo().to_str().expect("path")]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("  1  mdview デモ"));
    assert!(stdout.contains("  5      フローチャート"));
}

#[test]
fn extracts_section() {
    let output = run(&["-c", "--section", "2", demo().to_str().expect("path")]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("テキスト表現"));
    assert!(!stdout.contains("Mermaid"));
}

#[test]
fn check_exits_successfully() {
    let output = run(&["--check"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("mdview"));
    assert!(stdout.contains("renderer"));
}

#[test]
fn renders_mermaid_to_half_blocks() {
    let markdown = "# Diagram\n\n```mermaid\nflowchart LR\n  A --> B\n```\n";
    let path = std::env::temp_dir().join(format!("mdview-cli-test-{}.md", std::process::id()));
    std::fs::write(&path, markdown).expect("write fixture");
    let output = run(&["-r", "blocks", path.to_str().expect("path")]);
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains('\u{2580}'));
    assert!(stdout.contains("\x1b[38;2;"));
}
