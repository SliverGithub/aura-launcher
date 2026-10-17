//! Thin JSON bridge to Aurelia's own CLI.
//!
//! The same executable is the protocol client: it owns the CM session, library scans, CDN
//! downloads, Proton launch pipeline and web/community surfaces. This module
//! only spawns it and turns its NDJSON output into events for the TUI.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Backend {
    pub path: PathBuf,
}

/// One line from a long-running child (`install`, `update`, `verify`, `chat`).
#[derive(Debug)]
pub enum Stream {
    Line {
        text: String,
        stderr: bool,
    },
    Done {
        success: bool,
        error: Option<String>,
    },
}

/// Handle to a live `aurelia chat open` child.
pub struct ChatProcess {
    stdin: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
}

impl Drop for ChatProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

impl ChatProcess {
    pub fn send(&self, text: &str) -> Result<()> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| anyhow!("chat input closed"))?;
        writeln!(stdin, "{}", text.replace(['\n', '\r'], " "))
            .context("writing to chat process")?;
        stdin.flush().context("flushing chat input")?;
        Ok(())
    }

    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Download and decode one image (store artwork, avatar or market icon).
/// Runs on a plain worker thread, never on the TUI thread.
pub fn fetch_image(url: &str) -> Result<image::DynamicImage> {
    let response = reqwest::blocking::get(url).with_context(|| format!("requesting {url}"))?;
    if !response.status().is_success() {
        bail!("image request failed with {}", response.status());
    }
    let bytes = response.bytes().context("reading image bytes")?;
    image::load_from_memory(&bytes).context("decoding image")
}

impl Backend {
    /// Run a one-shot JSON command. Aurelia prints a single JSON document;
    /// errors are an NDJSON object with `error`, `type` and an optional `hint`.
    pub fn json(&self, args: &[&str]) -> Result<Value> {
        let output = Command::new(&self.path)
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("running {}", self.path.display()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if let Some(error) = find_error(&stdout).or_else(|| find_error(&stderr)) {
            return Err(anyhow!(error));
        }
        if !output.status.success() {
            let detail = json_document(&stdout)
                .map(|value| value.to_string())
                .or_else(|| json_document(&stderr).map(|value| value.to_string()))
                .or_else(|| first_nonempty(&stderr))
                .or_else(|| first_nonempty(&stdout))
                .unwrap_or_else(|| format!("aurelia exited with {}", output.status));
            bail!("{detail}");
        }
        json_document(&stdout).ok_or_else(|| anyhow!("aurelia returned no JSON: {stdout}"))
    }

    /// Run a one-shot command whose JSON result is ignored.
    pub fn ok(&self, args: &[&str]) -> Result<()> {
        self.json(args).map(|_| ())
    }

    /// Same as [`json`](Self::json), for owned argument vectors built by the UI.
    pub fn json_vec(&self, args: &[String]) -> Result<Value> {
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        self.json(&borrowed)
    }

    /// Spawn a long-running JSON command and report stdout/stderr lines to
    /// `on_event` from a dedicated thread. Returns immediately.
    pub fn stream(
        &self,
        args: Vec<String>,
        mut on_event: impl FnMut(Stream) + Send + 'static,
    ) -> Result<()> {
        let mut child = self
            .command()
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning {} {}", self.path.display(), args.join(" ")))?;

        let stdout = child.stdout.take().context("capturing child stdout")?;
        let stderr = child.stderr.take().context("capturing child stderr")?;
        let (tx, rx) = mpsc::channel::<(bool, String)>();
        spawn_reader(stdout, false, tx.clone());
        spawn_reader(stderr, true, tx);

        thread::spawn(move || {
            for (is_stderr, text) in rx {
                on_event(Stream::Line {
                    text,
                    stderr: is_stderr,
                });
            }
            let status = child.wait();
            let (success, error) = match status {
                Ok(s) if s.success() => (true, None),
                Ok(s) => (false, Some(format!("aurelia exited with {s}"))),
                Err(e) => (false, Some(format!("waiting for aurelia: {e}"))),
            };
            on_event(Stream::Done { success, error });
        });
        Ok(())
    }

    /// Open a live chat. Lines from stdout/stderr are delivered to `on_event`;
    /// `Done` fires when the child exits.
    pub fn chat(
        &self,
        steam_id: u64,
        mut on_event: impl FnMut(Stream) + Send + 'static,
    ) -> Result<ChatProcess> {
        let mut child = self
            .command()
            .args(["chat", "open", &steam_id.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("opening chat with {steam_id}"))?;

        let stdin = child.stdin.take().context("capturing chat stdin")?;
        let stdout = child.stdout.take().context("capturing chat stdout")?;
        let stderr = child.stderr.take().context("capturing chat stderr")?;
        let child = Arc::new(Mutex::new(child));

        let (tx, rx) = mpsc::channel::<(bool, String)>();
        spawn_reader(stdout, false, tx.clone());
        spawn_reader(stderr, true, tx);
        thread::spawn(move || {
            for (is_stderr, text) in rx {
                on_event(Stream::Line {
                    text,
                    stderr: is_stderr,
                });
            }
            on_event(Stream::Done {
                success: true,
                error: None,
            });
        });

        Ok(ChatProcess {
            stdin: Arc::new(Mutex::new(stdin)),
            child,
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.path);
        command.arg("--json");
        command
    }

    /// Open a URL with the platform's default handler (no browser dependency).
    pub fn open_url(url: &str) -> Result<()> {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new(opener)
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("opening {url}"))?;
        Ok(())
    }
}

fn spawn_reader(
    reader: impl std::io::Read + Send + 'static,
    stderr: bool,
    tx: mpsc::Sender<(bool, String)>,
) {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    if tx.send((stderr, line)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn last_json(text: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        serde_json::from_str::<Value>(line.trim())
            .ok()
            .map(|_| line.trim().to_string())
    })
}

fn find_error(text: &str) -> Option<String> {
    if let Some(error) = json_document(text).and_then(|value| error_from_value(&value)) {
        return Some(error);
    }
    // NDJSON fallback: one error object per line.
    for line in text.lines().rev() {
        if let Some(error) = serde_json::from_str::<Value>(line.trim())
            .ok()
            .and_then(|value| error_from_value(&value))
        {
            return Some(error);
        }
    }
    None
}

fn error_from_value(value: &Value) -> Option<String> {
    let message = value.get("error").and_then(Value::as_str)?;
    let mut out = message.to_string();
    if let Some(kind) = value.get("type").and_then(Value::as_str)
        && !kind.eq_ignore_ascii_case("unknown")
    {
        out.push_str(&format!(" ({kind})"));
    }
    if let Some(hint) = value.get("hint").and_then(Value::as_str) {
        out.push_str(&format!(" — {hint}"));
    }
    Some(out)
}

/// Parse a complete JSON document, tolerating log lines before it.
fn json_document(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Some(value);
    }
    for (index, ch) in trimmed.char_indices() {
        if matches!(ch, '{' | '[')
            && let Ok(value) = serde_json::from_str(&trimmed[index..])
        {
            return Some(value);
        }
    }
    // NDJSON: take the last line that parses on its own.
    last_json(trimmed).and_then(|line| serde_json::from_str(&line).ok())
}

fn first_nonempty(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_json_skips_preamble() {
        let text = "warning: something\n{\"a\":1}\n{\"b\":2}\n";
        assert_eq!(last_json(text).as_deref(), Some("{\"b\":2}"));
    }

    #[test]
    fn error_object_is_readable() {
        let text = "{\"error\":\"not logged in\",\"type\":\"AuthRequired\",\"hint\":\"run `aurelia login`\"}";
        assert_eq!(
            find_error(text).as_deref(),
            Some("not logged in (AuthRequired) — run `aurelia login`")
        );
    }

    #[test]
    fn non_json_output_has_no_json() {
        assert!(last_json("plain text only").is_none());
    }

    #[test]
    fn parses_pretty_printed_documents() {
        let text = "daemon log\n{\n  \"logged_in\": false,\n  \"daemon\": true\n}\n";
        let value = json_document(text).unwrap();
        assert_eq!(value["logged_in"], false);
    }

    #[cfg(unix)]
    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(unix)]
    fn fake_backend(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let unique = format!(
            "aura-launcher-{name}-{}-{}-{stamp}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let tmp = std::env::temp_dir().join(format!("{unique}.tmp"));
        let path = std::env::temp_dir().join(format!("{unique}.sh"));
        std::fs::write(&tmp, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut permissions = std::fs::metadata(&tmp).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&tmp, permissions).unwrap();
        // Rename after closing: executing a path that is still open for write
        // produces ETXTBSY on some filesystems.
        std::fs::rename(&tmp, &path).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn runs_a_pretty_json_command() {
        let _guard = test_lock();
        let path = fake_backend("ok", "printf 'log line\\n{\\n  \"value\": 7\\n}\\n'");
        let backend = Backend { path: path.clone() };
        let value = backend.json(&["list"]).unwrap();
        assert_eq!(value["value"], 7);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn streams_stdout_and_stderr_then_reports_done() {
        let _guard = test_lock();
        let path = fake_backend(
            "stream",
            "printf '{\"event\":\"progress\",\"percent\":50}\\n' >&2; printf '{\"event\":\"result\"}\\n'",
        );
        let backend = Backend { path: path.clone() };
        let (tx, rx) = mpsc::channel();
        backend
            .stream(vec!["install".into(), "1".into()], move |event| {
                let _ = tx.send(event);
            })
            .unwrap();
        let mut events = Vec::new();
        for event in rx {
            let done = matches!(event, Stream::Done { .. });
            events.push(event);
            if done {
                break;
            }
        }
        assert!(events
            .iter()
            .any(|event| matches!(event, Stream::Line { text, stderr: true } if text.contains("progress"))));
        assert!(events.iter().any(
            |event| matches!(event, Stream::Line { text, stderr: false } if text.contains("result"))
        ));
        assert!(matches!(
            events.last(),
            Some(Stream::Done { success: true, .. })
        ));
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn chat_relays_incoming_events_and_sends_lines() {
        let _guard = test_lock();
        let path = fake_backend(
            "chat",
            "printf '{\"event\":\"ready\",\"with\":1}\\n'
while IFS= read -r line; do
  if [ \"$line\" = \"quit\" ]; then printf '{\"event\":\"closed\"}\\n'; exit 0; fi
  printf '{\"event\":\"echo\",\"to\":1,\"text\":\"%s\"}\\n' \"$line\"
done",
        );
        let backend = Backend { path: path.clone() };
        let (tx, rx) = mpsc::channel();
        let process = backend
            .chat(1, move |event| {
                let _ = tx.send(event);
            })
            .unwrap();
        let ready = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert!(matches!(ready, Stream::Line { text, .. } if text.contains("ready")));
        process.send("hello").unwrap();
        let echo = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert!(matches!(echo, Stream::Line { text, .. } if text.contains("hello")));
        process.kill();
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn surfaces_an_error_document() {
        let _guard = test_lock();
        let path = fake_backend(
            "err",
            "printf '{\"error\":\"not logged in\",\"type\":\"AuthRequired\",\"hint\":\"run login\"}\\n'; exit 1",
        );
        let backend = Backend { path: path.clone() };
        let error = backend.json(&["list"]).unwrap_err().to_string();
        assert!(error.contains("not logged in"), "error was: {error}");
        assert!(error.contains("AuthRequired"));
        let _ = std::fs::remove_file(path);
    }
}
