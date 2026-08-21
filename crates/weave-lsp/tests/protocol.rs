use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;
use tower_lsp_server::ls_types::Uri;

struct LspClient {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
    pending: Vec<Value>,
}

impl LspClient {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_weave-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn weave-lsp");
        let input = child.stdin.take().expect("server stdin");
        let output = BufReader::new(child.stdout.take().expect("server stdout"));
        Self {
            child,
            input,
            output,
            next_id: 1,
            pending: Vec::new(),
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        loop {
            let message = self.read();
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                assert!(
                    message.get("error").is_none(),
                    "request {method} failed: {message}"
                );
                return message.get("result").cloned().unwrap_or(Value::Null);
            }
            self.pending.push(message);
        }
    }

    fn notification(&mut self, method: &str, params: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    fn malformed_request(&mut self) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 999_998,
            "method": "textDocument/hover",
            "params": { "malformed": true }
        }));
    }

    fn wait_for_notification(&mut self, method: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self.pending.iter().position(|message| {
            message.get("method").and_then(Value::as_str) == Some(method) && predicate(message)
        }) {
            return self.pending.remove(index);
        }
        loop {
            let message = self.read();
            if message.get("method").and_then(Value::as_str) == Some(method) && predicate(&message)
            {
                return message;
            }
            self.pending.push(message);
        }
    }

    fn send(&mut self, message: &Value) {
        let body = serde_json::to_vec(message).expect("serialize request");
        write!(self.input, "Content-Length: {}\r\n\r\n", body.len()).expect("request header");
        self.input.write_all(&body).expect("request body");
        self.input.flush().expect("flush request");
    }

    fn read(&mut self) -> Value {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            let read = self
                .output
                .read_line(&mut line)
                .expect("read response header");
            assert_ne!(read, 0, "language server exited before responding");
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some(length) = line.strip_prefix("Content-Length:") {
                content_length = Some(length.trim().parse::<usize>().expect("content length"));
            }
        }
        let mut body = vec![0; content_length.expect("Content-Length response header")];
        self.output.read_exact(&mut body).expect("response body");
        serde_json::from_slice(&body).expect("JSON response")
    }

    fn shutdown(mut self) {
        assert_eq!(self.request("shutdown", Value::Null), Value::Null);
        self.notification("exit", Value::Null);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.child.try_wait().expect("server status") {
                assert!(status.success(), "language server exited unsuccessfully");
                return;
            }
            if Instant::now() >= deadline {
                self.child
                    .kill()
                    .expect("stop unresponsive language server");
                panic!("language server did not exit after shutdown");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn serves_incremental_workspace_language_features_over_stdio() {
    let workspace = TempDir::new().expect("temporary workspace");
    let main_path = workspace.path().join("main.weave");
    let ending_path = workspace.path().join("ending.weave");
    let main_source = "VAR courage=1\n=== start ===\nHello {courage}.\n-> ending\n";
    let ending_source = "=== ending ===\n-> END\n";
    fs::write(&main_path, main_source).expect("write main story");
    fs::write(&ending_path, ending_source).expect("write ending story");

    let root_uri = Uri::from_file_path(workspace.path()).expect("workspace URI");
    let main_uri = Uri::from_file_path(&main_path).expect("main URI");
    let ending_uri = Uri::from_file_path(&ending_path).expect("ending URI");
    let root_json = serde_json::to_value(&root_uri).expect("root JSON");
    let main_json = serde_json::to_value(&main_uri).expect("main JSON");
    let ending_json = serde_json::to_value(&ending_uri).expect("ending JSON");

    let mut client = LspClient::spawn();
    let initialized = client.request(
        "initialize",
        json!({
            "processId": null,
            "rootUri": root_json,
            "capabilities": {
                "general": { "positionEncodings": ["utf-16"] },
                "workspace": { "workspaceFolders": true }
            },
            "workspaceFolders": [{ "uri": root_json, "name": "story" }]
        }),
    );
    assert_eq!(initialized["serverInfo"]["name"], "weave-lsp");
    assert_eq!(initialized["capabilities"]["textDocumentSync"]["change"], 2);
    assert_eq!(initialized["capabilities"]["positionEncoding"], "utf-16");
    for capability in [
        "completionProvider",
        "hoverProvider",
        "definitionProvider",
        "referencesProvider",
        "documentSymbolProvider",
        "documentFormattingProvider",
        "renameProvider",
    ] {
        assert!(
            initialized["capabilities"].get(capability).is_some(),
            "missing {capability}"
        );
    }

    client.notification("initialized", json!({}));
    client.notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": main_json,
                "languageId": "weave",
                "version": 1,
                "text": main_source
            }
        }),
    );
    client.notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": ending_json,
                "languageId": "weave",
                "version": 1,
                "text": ending_source
            }
        }),
    );

    let diagnostics = client.wait_for_notification("textDocument/publishDiagnostics", |message| {
        message["params"]["uri"] == main_json
            && message["params"]["version"] == 1
            && message["params"]["diagnostics"].is_array()
    });
    assert_eq!(diagnostics["params"]["diagnostics"], json!([]));

    let symbols = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": main_json } }),
    );
    assert!(
        symbols
            .as_array()
            .is_some_and(|symbols| symbols.iter().any(|symbol| symbol["name"] == "start"))
    );
    assert!(
        symbols
            .as_array()
            .is_some_and(|symbols| symbols.iter().any(|symbol| symbol["name"] == "courage"))
    );

    let definition = client.request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": main_json },
            "position": { "line": 3, "character": 4 }
        }),
    );
    assert!(definition.as_array().is_some_and(|locations| {
        locations
            .iter()
            .any(|location| location["uri"] == ending_json)
    }));

    let references = client.request(
        "textDocument/references",
        json!({
            "textDocument": { "uri": ending_json },
            "position": { "line": 0, "character": 5 },
            "context": { "includeDeclaration": true }
        }),
    );
    assert_eq!(references.as_array().map(Vec::len), Some(2));

    let hover = client.request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": main_json },
            "position": { "line": 2, "character": 9 }
        }),
    );
    assert!(
        hover["contents"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("Number"))
    );

    let completion = client.request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": main_json },
            "position": { "line": 3, "character": 2 }
        }),
    );
    assert!(
        completion
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["label"] == "ending"))
    );

    let rename = client.request(
        "textDocument/rename",
        json!({
            "textDocument": { "uri": main_json },
            "position": { "line": 2, "character": 9 },
            "newName": "resolve"
        }),
    );
    assert_eq!(
        rename["changes"][main_uri.as_str()]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    let formatting = client.request(
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": main_json },
            "options": { "tabSize": 4, "insertSpaces": true }
        }),
    );
    assert!(formatting.as_array().is_some_and(|edits| {
        edits.first().is_some_and(|edit| {
            edit["newText"]
                .as_str()
                .is_some_and(|text| text.contains("VAR courage = 1"))
        })
    }));

    client.notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": main_json, "version": 2 },
            "contentChanges": [{
                "range": {
                    "start": { "line": 2, "character": 7 },
                    "end": { "line": 2, "character": 14 }
                },
                "rangeLength": 7,
                "text": "missing.value"
            }]
        }),
    );
    let broken = client.wait_for_notification("textDocument/publishDiagnostics", |message| {
        message["params"]["uri"] == main_json && message["params"]["version"] == 2
    });
    assert!(
        broken["params"]["diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| {
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic["code"] == "W2028")
            }),
        "{broken}"
    );

    client.notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": main_json, "version": 3 },
            "contentChanges": [{
                "range": {
                    "start": { "line": 2, "character": 7 },
                    "end": { "line": 2, "character": 20 }
                },
                "rangeLength": 13,
                "text": "courage"
            }]
        }),
    );
    let repaired = client.wait_for_notification("textDocument/publishDiagnostics", |message| {
        message["params"]["uri"] == main_json && message["params"]["version"] == 3
    });
    assert_eq!(repaired["params"]["diagnostics"], json!([]));

    client.malformed_request();
    client.notification("$/cancelRequest", json!({ "id": 999_999 }));
    let still_alive = client.request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": main_json },
            "position": { "line": 0, "character": 0 }
        }),
    );
    assert!(still_alive.is_array());

    client.shutdown();
}
