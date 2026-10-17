//! Claude Code's user settings (`~/.claude/settings.json`), `env` block only.

use std::path::{Path, PathBuf};

use hud_application::ports::telemetry::ClaudeSettings;
use serde_json::{Map, Value};

/// The settings file of Claude Code.
#[derive(Debug, Clone)]
pub struct ClaudeSettingsFile {
    path: PathBuf,
}

impl ClaudeSettingsFile {
    /// Settings at `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`.
    #[must_use]
    pub fn default_path(home: &Path, config_dir: Option<&Path>) -> PathBuf {
        config_dir
            .filter(|dir| dir.is_absolute())
            .map_or_else(|| home.join(".claude"), Path::to_path_buf)
            .join("settings.json")
    }

    async fn read(&self) -> Result<Option<Map<String, Value>>, String> {
        let text = match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        match serde_json::from_str(&text) {
            Ok(Value::Object(root)) => Ok(Some(root)),
            Ok(_) => Err(format!("{} is not a JSON object", self.path.display())),
            Err(error) => Err(format!("{}: {error}", self.path.display())),
        }
    }
}

impl ClaudeSettings for ClaudeSettingsFile {
    async fn env(&self) -> Result<Vec<(String, String)>, String> {
        let Some(root) = self.read().await? else {
            return Ok(Vec::new());
        };
        Ok(match root.get("env") {
            Some(Value::Object(env)) => env
                .iter()
                .map(|(key, value)| {
                    let text = value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned);
                    (key.clone(), text)
                })
                .collect(),
            _ => Vec::new(),
        })
    }

    async fn add_env(&self, entries: Vec<(&'static str, String)>) -> Result<(), String> {
        let mut root = self.read().await?.unwrap_or_default();
        let env = root
            .entry("env")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| "env in settings.json is not an object".to_owned())?;
        for (key, value) in entries {
            // Never replace an existing value, even if it differs.
            env.entry(key).or_insert(Value::String(value));
        }
        let mut text = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
        text.push('\n');
        write_atomically(&self.path, &text).await
    }
}

/// Writes next to the target and renames, keeping the file's permissions.
async fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| e.to_string())?;
    }
    let temporary = path.with_extension("json.hud-tmp");
    tokio::fs::write(&temporary, text)
        .await
        .map_err(|e| e.to_string())?;
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        tokio::fs::set_permissions(&temporary, metadata.permissions())
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::rename(&temporary, path)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXISTING: &str = r#"{
  "model": "opus",
  "hooks": {"Stop": []},
  "env": {"FOO": "bar"},
  "statusLine": {"type": "command"}
}"#;

    #[tokio::test]
    async fn adding_entries_keeps_every_other_key_and_its_order() {
        let dir = tempfile::tempdir().expect("BUG");
        let path = dir.path().join("settings.json");
        std::fs::write(&path, EXISTING).expect("BUG");
        let file = ClaudeSettingsFile::new(path.clone());
        file.add_env(vec![
            ("CLAUDE_CODE_ENABLE_TELEMETRY", "1".into()),
            ("FOO", "changed".into()),
        ])
        .await
        .expect("BUG");

        let text = std::fs::read_to_string(&path).expect("BUG");
        let keys: Vec<&str> = ["\"model\"", "\"hooks\"", "\"env\"", "\"statusLine\""]
            .into_iter()
            .filter(|key| text.contains(key))
            .collect();
        assert_eq!(keys.len(), 4, "{text}");
        assert!(
            text.find("\"model\"") < text.find("\"statusLine\""),
            "{text}"
        );
        let env = file.env().await.expect("BUG");
        assert!(
            env.contains(&("FOO".to_owned(), "bar".to_owned())),
            "existing value kept"
        );
        assert!(env.contains(&("CLAUDE_CODE_ENABLE_TELEMETRY".to_owned(), "1".to_owned())));
    }

    #[tokio::test]
    async fn a_missing_file_is_created_with_the_env_block() {
        let dir = tempfile::tempdir().expect("BUG");
        let file = ClaudeSettingsFile::new(dir.path().join("sub/settings.json"));
        assert_eq!(file.env().await.expect("BUG"), Vec::new());
        file.add_env(vec![("OTEL_METRICS_EXPORTER", "otlp".into())])
            .await
            .expect("BUG");
        assert_eq!(file.env().await.expect("BUG").len(), 1);
    }

    #[tokio::test]
    async fn invalid_json_is_never_overwritten() {
        let dir = tempfile::tempdir().expect("BUG");
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").expect("BUG");
        let file = ClaudeSettingsFile::new(path.clone());
        assert!(file.add_env(vec![("X", "1".into())]).await.is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("BUG"), "{ not json");
    }
}
