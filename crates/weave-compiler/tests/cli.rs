use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::tempdir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_weavec")
}

#[test]
fn compiles_default_ron_and_explicit_json() {
    let directory = tempdir().expect("temporary directory");
    let source = directory.path().join("story.weave");
    fs::write(&source, "=== start ===\nHello.\n-> END\n").expect("write source");

    let status = Command::new(binary())
        .arg(&source)
        .status()
        .expect("run compiler");
    assert!(status.success());
    let ron = fs::read_to_string(source.with_extension("ron")).expect("read RON");
    assert!(ron.contains("version: 2"));
    assert!(ron.contains("Hello."));

    let json_path = directory.path().join("custom.json");
    let status = Command::new(binary())
        .args(["--format", "json", "--output"])
        .arg(&json_path)
        .arg(&source)
        .status()
        .expect("run compiler");
    assert!(status.success());
    let json = fs::read_to_string(json_path).expect("read JSON");
    assert!(json.contains("\"version\": 2"));
}

#[test]
fn reports_compiler_and_io_failures_with_conventional_codes() {
    let directory = tempdir().expect("temporary directory");
    let invalid = directory.path().join("invalid.weave");
    fs::write(&invalid, "=== start ===\n-> nowhere\n").expect("write source");
    let output = Command::new(binary())
        .arg(&invalid)
        .output()
        .expect("run compiler");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("W2021"));
    assert!(!invalid.with_extension("ron").exists());

    let missing = Command::new(binary())
        .arg(directory.path().join("missing.weave"))
        .output()
        .expect("run compiler");
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("could not read"));
}

#[test]
fn watch_recompiles_after_source_changes() {
    let directory = tempdir().expect("temporary directory");
    let source = directory.path().join("watch.weave");
    let output = source.with_extension("ron");
    fs::write(&source, "=== start ===\nFirst.\n-> END\n").expect("write source");

    let child = Command::new(binary())
        .arg("--watch")
        .arg(&source)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start watcher");
    let mut child = ChildGuard(Some(child));
    wait_until(Duration::from_secs(5), || output.exists());
    fs::write(&source, "=== start ===\nSecond.\n-> END\n").expect("update source");
    wait_until(Duration::from_secs(5), || {
        fs::read_to_string(&output)
            .map(|contents| contents.contains("Second."))
            .unwrap_or(false)
    });
    child.stop();
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("condition was not met within {timeout:?}");
}

struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn stop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.stop();
    }
}
