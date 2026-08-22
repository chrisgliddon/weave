use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::tempdir;
use weave_patterns::PackageRegistry;

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
fn emits_the_published_json_schema() {
    let output = Command::new(binary())
        .arg("--schema")
        .output()
        .expect("run schema command");
    assert!(output.status.success());
    let schema: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("schema output is JSON");
    assert_eq!(schema["$id"], "urn:weave:schema:story-ir:2");
    assert_eq!(schema["properties"]["version"]["const"], 2);
}

#[test]
fn compiles_a_story_against_an_installed_community_pattern() {
    let directory = tempdir().expect("temporary directory");
    let registry_path = directory.path().join("registry");
    let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../patterns/community/ember-omens/package.weave-pattern.json");
    PackageRegistry::new(&registry_path)
        .install(package)
        .expect("install sample package");
    let source = directory.path().join("community.weave");
    fs::write(
        &source,
        "VAR sign = ember_omens.spread.three_signs.draw()\n=== start ===\n{sign.kindling.meaning}\n-> END\n",
    )
    .expect("write source");
    let output_path = directory.path().join("community.json");

    let output = Command::new(binary())
        .args(["--format", "json", "--output"])
        .arg(&output_path)
        .args(["--pattern-registry"])
        .arg(&registry_path)
        .args(["--pattern", "ember_omens@^1.0"])
        .arg(&source)
        .output()
        .expect("run compiler");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled: serde_json::Value =
        serde_json::from_slice(&fs::read(output_path).expect("read output"))
            .expect("compiled JSON");
    assert_eq!(
        compiled["patterns"]["ember_omens"]["draw_method"]["kind"],
        "weighted_by"
    );
}

#[test]
fn missing_community_package_fails_without_partial_output() {
    let directory = tempdir().expect("temporary directory");
    let source = source_fixture(&directory);
    let destination = directory.path().join("missing.json");
    let output = Command::new(binary())
        .args(["--format", "json", "--output"])
        .arg(&destination)
        .args(["--pattern-registry"])
        .arg(directory.path().join("registry"))
        .args(["--pattern", "missing@^1"])
        .arg(source)
        .output()
        .expect("run compiler");
    assert_eq!(output.status.code(), Some(2));
    assert!(!destination.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no installed community package"));
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

    let blocked_parent = directory.path().join("not-created");
    let destination = blocked_parent.join("story.json");
    let failed_write = Command::new(binary())
        .args(["--format", "json", "--output"])
        .arg(&destination)
        .arg(source_fixture(&directory))
        .output()
        .expect("run compiler with invalid destination");
    assert_eq!(failed_write.status.code(), Some(2));
    assert!(!destination.exists());
    assert!(!blocked_parent.exists());
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

fn source_fixture(directory: &tempfile::TempDir) -> std::path::PathBuf {
    let source = directory.path().join("valid.weave");
    fs::write(&source, "=== start ===\nHello.\n-> END\n").expect("write valid source");
    source
}

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
