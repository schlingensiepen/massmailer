//! Settings from config files, environment variables and command-line values.
//!
//! Precedence: command line > environment (`MASSMAILER_…`) > config files;
//! among config files the later one wins. Relative paths are resolved
//! against the directory of the config file that contains them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

/// All keys the tool knows; used for environment lookup and validation.
pub const KNOWN_KEYS: &[&str] = &[
    "smtp.host",
    "smtp.port",
    "smtp.security",
    "smtp.username",
    "smtp.password",
    "smtp.timeout_seconds",
    "ses.configuration_set",
    "sending.rate_per_second",
    "sending.redirect_to",
    "sending.limit",
    "run.state_dir",
    "sender.address",
    "sender.name",
    "sender.reply_to",
    "sender.on_behalf_of",
    "letterhead.dir",
    "message.body",
    "message.format",
    "message.text",
    "message.subject",
    "message.preheader",
    "message.placeholders",
    "message.images",
    "message.parts",
    "message.to",
    "message.cc",
    "message.bcc",
    "message.attachments",
    "message.attachment_column",
    "message.attachment_dir",
    "message.max_mail_size_mb",
    "message.text_width",
    "recipients.source",
    "recipients.format",
    "recipients.to_columns",
    "recipients.cc_columns",
    "recipients.bcc_columns",
    "recipients.skip_column",
    "recipients.csv.delimiter",
    "recipients.sheet",
    "recipients.url.bearer_token",
    "fetch.timeout_seconds",
    "fetch.max_size_mb",
];

/// Keys whose values are file paths (or URLs) and are resolved relative to
/// the config file that sets them.
const PATH_KEYS: &[&str] = &[
    "message.body",
    "message.text",
    "message.attachments",
    "message.attachment_dir",
    "letterhead.dir",
    "recipients.source",
    "run.state_dir",
];

/// Keys whose values must never be printed.
const SECRET_KEYS: &[&str] = &[
    "smtp.password",
    "smtp.username",
    "recipients.url.bearer_token",
];

pub fn is_secret(key: &str) -> bool {
    SECRET_KEYS.contains(&key)
}

/// Environment variable name for a key: `smtp.password` → `MASSMAILER_SMTP_PASSWORD`.
pub fn env_name(key: &str) -> String {
    format!("MASSMAILER_{}", key.to_ascii_uppercase().replace('.', "_"))
}

#[derive(Default, Debug)]
pub struct Settings {
    values: BTreeMap<String, Value>,
    origin: BTreeMap<String, String>,
}

fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, Value)>) {
    match v {
        Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, v, out);
            }
        }
        _ => out.push((prefix.to_string(), v.clone())),
    }
}

pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn resolve_path_value(v: &Value, base: &Path) -> Value {
    let fix = |s: &str| -> Value {
        if s.is_empty() || is_url(s) || Path::new(s).is_absolute() {
            Value::String(s.to_string())
        } else {
            Value::String(base.join(s).to_string_lossy().into_owned())
        }
    };
    match v {
        Value::String(s) => fix(s),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|x| x.as_str().map(fix).unwrap_or_else(|| x.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Parse a command-line or environment value: JSON if it parses, else a string.
fn parse_loose(s: &str) -> Value {
    match serde_json::from_str::<Value>(s) {
        Ok(v @ (Value::Array(_) | Value::Number(_) | Value::Bool(_) | Value::Object(_))) => v,
        _ => Value::String(s.to_string()),
    }
}

impl Settings {
    pub fn load(configs: &[PathBuf], overrides: &[(String, String)]) -> Result<Self> {
        let mut s = Settings::default();
        for path in configs {
            let text = fs::read_to_string(path)
                .with_context(|| format!("cannot read config {}", path.display()))?;
            let json: Value = serde_json::from_str(&text)
                .with_context(|| format!("invalid JSON in {}", path.display()))?;
            if !json.is_object() {
                bail!("{}: top level must be an object", path.display());
            }
            // `absolute`, not `canonicalize`: on Windows the latter yields
            // `\\?\UNC\…` paths in which `..` is not resolved.
            let base = std::path::absolute(path)
                .unwrap_or_else(|_| path.clone())
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            let mut flat = Vec::new();
            flatten("", &json, &mut flat);
            for (key, mut value) in flat {
                if !KNOWN_KEYS.contains(&key.as_str()) {
                    bail!("{}: unknown setting `{key}`", path.display());
                }
                if PATH_KEYS.contains(&key.as_str()) {
                    value = resolve_path_value(&value, &base);
                }
                s.set(&key, value, &format!("config {}", path.display()));
            }
        }
        for key in KNOWN_KEYS {
            let name = env_name(key);
            if let Ok(v) = std::env::var(&name) {
                s.set(key, parse_loose(&v), &format!("env {name}"));
            }
        }
        for (key, v) in overrides {
            if !KNOWN_KEYS.contains(&key.as_str()) {
                bail!("unknown setting `{key}`");
            }
            s.set(key, parse_loose(v), "command line");
        }
        Ok(s)
    }

    pub fn set(&mut self, key: &str, value: Value, origin: &str) {
        self.values.insert(key.to_string(), value);
        self.origin.insert(key.to_string(), origin.to_string());
    }

    /// One line per value with its origin; secrets without value.
    pub fn describe(&self) -> Vec<String> {
        self.values
            .iter()
            .map(|(k, v)| {
                let shown = if is_secret(k) {
                    "***".to_string()
                } else {
                    v.to_string()
                };
                format!("{k} = {shown}  ({})", self.origin[k])
            })
            .collect()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key).filter(|v| !v.is_null())
    }

    pub fn str(&self, key: &str) -> Option<String> {
        match self.get(key)? {
            Value::String(s) if s.is_empty() => None,
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    pub fn req(&self, key: &str) -> Result<String> {
        self.str(key).with_context(|| {
            format!(
                "missing setting `{key}` (config, {} or command line)",
                env_name(key)
            )
        })
    }

    pub fn list(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect(),
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => vec![],
        }
    }

    pub fn num(&self, key: &str) -> Result<Option<f64>> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::Number(n)) => Ok(n.as_f64()),
            Some(Value::String(s)) => s
                .trim()
                .parse::<f64>()
                .map(Some)
                .with_context(|| format!("`{key}` must be a number")),
            Some(_) => bail!("`{key}` must be a number"),
        }
    }

    pub fn bool(&self, key: &str, default: bool) -> Result<bool> {
        match self.get(key) {
            None => Ok(default),
            Some(Value::Bool(b)) => Ok(*b),
            Some(Value::String(s)) => match s.to_ascii_lowercase().as_str() {
                "true" | "yes" | "ja" | "1" => Ok(true),
                "false" | "no" | "nein" | "0" => Ok(false),
                _ => bail!("`{key}` must be true or false"),
            },
            Some(_) => bail!("`{key}` must be true or false"),
        }
    }
}

/// Split `key=value` from the command line.
pub fn parse_set(s: &str) -> Result<(String, String)> {
    let (k, v) = s
        .split_once('=')
        .with_context(|| format!("--set expects KEY=VALUE, got `{s}`"))?;
    Ok((k.trim().to_string(), v.to_string()))
}

#[allow(dead_code)]
fn _assert_map(_: Map<String, Value>) {}
