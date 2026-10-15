//! The model: any OpenAI-compatible chat API (OpenAI, OpenRouter, DeepSeek, a local Ollama or
//! LM Studio), reached through the system's `curl` as the updater does (`crate::update`).
//! The key is handed to curl in a config file only this user can read, never on its command
//! line.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::text::prepare;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The API's base, up to and with `/v1`.
    pub endpoint: String,
    /// Empty for a local server.
    pub api_key: String,
    pub model: String,
    /// The model's context window in tokens: it sizes the batches.
    pub context: u32,
    /// Sent as `reasoning_effort` when not empty (`none` turns thinking off in Ollama); left
    /// out by itself if the server refuses it.
    pub reasoning_effort: String,
    /// Ask for a JSON object (`response_format`).
    pub json_mode: bool,
    pub temperature: f32,
    /// Seconds one answer may take.
    pub timeout: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            endpoint: "http://localhost:11434/v1".into(),
            api_key: String::new(),
            model: "gemma4".into(),
            context: 8192,
            reasoning_effort: "none".into(),
            json_mode: true,
            temperature: 0.2,
            timeout: 600,
        }
    }
}

impl Settings {
    /// `translate.json` next to Razdor's remembered install (`crate::dt::install::config_file`).
    pub fn path() -> Option<PathBuf> {
        Some(crate::dt::install::config_file()?.with_file_name("translate.json"))
    }

    pub fn load() -> Settings {
        Settings::path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Settings::path().ok_or_else(|| std::io::Error::other("no config folder"))?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        // It may hold the key: written owner-only from the start, then renamed into place.
        let tmp = path.with_extension("json.tmp");
        let _ = std::fs::remove_file(&tmp);
        write_private(&tmp, json.as_bytes())?;
        std::fs::rename(&tmp, &path)
    }

    /// Characters of Russian per batch: about 2.5 characters a token, the answer as long
    /// again, and room for the instructions and the glossary.
    pub fn batch_chars(&self) -> usize {
        (self.context as usize * 2 / 5).clamp(400, 6000)
    }

    /// Entries per batch: small models lose count in long lists.
    pub fn batch_items(&self) -> usize {
        if self.context >= 32768 { 60 } else { 30 }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.endpoint.trim().trim_end_matches('/'), path)
    }
}

/// Creates `path` readable by its owner only from the start, never reusing a file (or a
/// symlink planted) there: the shared temp folder is open to every user.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)?.write_all(bytes)
}

/// The instructions sent with every batch.
pub const SYSTEM_PROMPT: &str = "You translate texts of the Russian fantasy strategy game \"Discord Times\" \
(\"Времена раздора\") into English for its English edition.

You get a JSON object of numbered Russian texts. Reply with ONLY a JSON object with the same keys, \
each value the English translation of that text. Translate every entry; never merge, split, skip or \
add entries; never explain.

Keep exactly, in the same places:
- placeholders like {0}, {1};
- <br> and line breaks (\\n): the same number of lines;
- the markup characters * | @ ^ (they style a line; keep each where it stands, usually at the line's start).

Style: medieval fantasy, plain and vivid, the tone of the original. Keep it about as long as the Russian. \
Names: translate names that have a meaning (\"Проклятое озеро\" -> \"Cursed Lake\", \"Старый лес\" -> \"Old Forest\"); \
transliterate personal names (\"Иван\" -> \"Ivan\"). Use the glossary's English for the names it lists. \
Use plain ASCII only: straight quotes, '-' for dashes, '...'; no Cyrillic in the answer.";

/// The user message for `texts` (the batch's Russian, in order, numbered from 1).
pub fn user_prompt(texts: &[&str], glossary: &[(String, String)]) -> String {
    let mut out = String::new();
    if !glossary.is_empty() {
        out.push_str("Glossary (Russian = English):\n");
        for (ru, en) in glossary {
            out.push_str(&format!("{ru} = {en}\n"));
        }
        out.push('\n');
    }
    let obj: serde_json::Map<String, Value> =
        texts.iter().enumerate().map(|(k, t)| ((k + 1).to_string(), Value::String(prepare(t).text))).collect();
    out.push_str("Translate:\n");
    out.push_str(&serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default());
    out
}

/// The JSON object in a model's answer (code fences and chatter around it ignored): its
/// string values by key.
pub fn parse_answer(answer: &str) -> Result<HashMap<String, String>, String> {
    let start = answer.find('{').ok_or("no JSON object in the answer")?;
    let end = answer.rfind('}').ok_or("no JSON object in the answer")?;
    if end < start {
        return Err("no JSON object in the answer".into());
    }
    let v: Value = serde_json::from_str(&answer[start..=end]).map_err(|e| format!("the answer is not valid JSON: {e}"))?;
    let obj = v.as_object().ok_or("the answer is not a JSON object")?;
    Ok(obj.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
}

/// `curl` with its window hidden on Windows.
fn curl() -> Command {
    let mut c = Command::new("curl");
    c.args(["-sS", "-A", concat!("razdor-translate/", env!("CARGO_PKG_VERSION"))]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// A request to `url`: `body` (JSON) posted, or a GET without one. Returns the status and
/// the body.
fn request(s: &Settings, url: &str, body: Option<&Value>) -> Result<(u32, String), String> {
    // The key goes in a curl config file only this user can read.
    let dir = std::env::temp_dir();
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let conf = dir.join(format!("razdor-translate-{}-{n}.curl", std::process::id()));
    let mut lines = format!("url = \"{}\"\nheader = \"Content-Type: application/json\"\n", url.replace('"', "%22"));
    if !s.api_key.trim().is_empty() {
        lines.push_str(&format!("header = \"Authorization: Bearer {}\"\n", s.api_key.trim().replace('"', "")));
    }
    write_private(&conf, lines.as_bytes()).map_err(|e| format!("{}: {e}", conf.display()))?;
    let mut cmd = curl();
    cmd.args(["--max-time", &s.timeout.to_string(), "-w", "\n%{http_code}", "-K"]).arg(&conf);
    if body.is_some() {
        cmd.args(["--data-binary", "@-"]);
    }
    let mut run = || -> Result<std::process::Output, String> {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("curl: {e}"))?;
        if let (Some(mut stdin), Some(b)) = (child.stdin.take(), body) {
            stdin.write_all(b.to_string().as_bytes()).map_err(|e| format!("curl: {e}"))?;
        }
        child.wait_with_output().map_err(|e| format!("curl: {e}"))
    };
    let out = run();
    let _ = std::fs::remove_file(&conf);
    let out = out?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    let code: u32 = code.trim().parse().unwrap_or(0);
    if code == 0 {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if why.is_empty() { "no answer from the server".into() } else { why });
    }
    Ok((code, body.to_string()))
}

fn api_error(code: u32, body: &str) -> String {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").or_else(|| v.get("error")).map(|m| m.as_str().map_or(m.to_string(), str::to_string)));
    format!("HTTP {code}: {}", msg.unwrap_or_else(|| body.chars().take(300).collect()))
}

/// One chat completion: the model's answer text.
pub fn chat(s: &Settings, system: &str, user: &str) -> Result<String, String> {
    let mut body = json!({
        "model": s.model.trim(),
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "temperature": s.temperature,
        "stream": false,
    });
    if s.json_mode {
        body["response_format"] = json!({"type": "json_object"});
    }
    let effort = s.reasoning_effort.trim();
    if !effort.is_empty() {
        body["reasoning_effort"] = json!(effort);
    }
    let url = s.url("chat/completions");
    let (mut code, mut text) = request(s, &url, Some(&body))?;
    // A server that does not know an option: once more without the optional ones.
    if code == 400 && (body.get("reasoning_effort").is_some() || body.get("response_format").is_some()) {
        let obj = body.as_object_mut().expect("a JSON object");
        obj.remove("reasoning_effort");
        obj.remove("response_format");
        (code, text) = request(s, &url, Some(&body))?;
    }
    if code != 200 {
        return Err(api_error(code, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("unreadable answer: {e}"))?;
    let choice = v.pointer("/choices/0").ok_or_else(|| api_error(code, &text))?;
    if choice.get("finish_reason").and_then(Value::as_str) == Some("length") {
        return Err("the answer was cut off (the context is too small for this batch)".into());
    }
    choice.pointer("/message/content").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "the answer has no text".into())
}

/// The models the server offers (`GET /models`): checks the address and the key.
pub fn models(s: &Settings) -> Result<Vec<String>, String> {
    let (code, text) = request(s, &s.url("models"), None)?;
    if code != 200 {
        return Err(api_error(code, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("unreadable answer: {e}"))?;
    let list = v.get("data").and_then(Value::as_array).ok_or("the server lists no models")?;
    Ok(list.iter().filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_numbers_texts_and_hides_tokens() {
        let p = user_prompt(&["Привет, #HERONAME", "Да"], &[("Иван".into(), "Ivan".into())]);
        assert!(p.starts_with("Glossary (Russian = English):\nИван = Ivan\n\nTranslate:\n"));
        let json = &p[p.find('{').unwrap()..];
        let v: Value = serde_json::from_str(json).unwrap();
        assert_eq!(v, json!({"1": "Привет, {0}", "2": "Да"}));
    }

    #[test]
    fn reads_the_json_out_of_an_answer() {
        let got = parse_answer("Sure!\n```json\n{\"1\": \"Hello\", \"2\": \"Yes\", \"3\": 5}\n```").unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got["1"], "Hello");
        assert!(parse_answer("no json here").is_err());
        assert!(parse_answer("{\"1\": ").is_err());
    }

    #[test]
    fn the_key_file_is_private_and_never_reused() {
        let path = std::env::temp_dir().join(format!("razdor-translate-test-{}.curl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        write_private(&path, b"x").unwrap();
        assert!(write_private(&path, b"y").is_err(), "an existing file is not reused");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn batches_grow_with_the_context() {
        let s = |context| Settings { context, ..Settings::default() };
        assert_eq!(s(8192).batch_chars(), 3276);
        assert_eq!(s(131072).batch_chars(), 6000);
        assert_eq!(s(512).batch_chars(), 400);
        assert_eq!(s(8192).batch_items(), 30);
    }
}
