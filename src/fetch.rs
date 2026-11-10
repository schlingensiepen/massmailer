//! Loading inputs (local files or http(s) URLs) once per run and keeping a
//! copy with its SHA-256 in the run folder (`inputs/`, `manifest.json`).

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::is_url;

/// Where relative references inside a document point to.
#[derive(Clone, Debug)]
pub enum Base {
    Dir(PathBuf),
    Url(url::Url),
}

impl Base {
    pub fn of(source: &str) -> Result<Base> {
        if is_url(source) {
            Ok(Base::Url(
                url::Url::parse(source).with_context(|| format!("invalid URL {source}"))?,
            ))
        } else {
            let p = Path::new(source);
            Ok(Base::Dir(
                p.parent().map(Path::to_path_buf).unwrap_or_default(),
            ))
        }
    }

    /// Resolve a reference found in a document to an absolute path or URL.
    /// `data:`, `cid:`, `mailto:` and fragment references are returned unchanged.
    pub fn resolve(&self, reference: &str) -> String {
        let r = reference.trim();
        if r.is_empty()
            || r.starts_with('#')
            || r.starts_with("data:")
            || r.starts_with("cid:")
            || r.starts_with("mailto:")
            || is_url(r)
        {
            return r.to_string();
        }
        if let Some(rest) = r.strip_prefix("//") {
            return format!("https://{rest}");
        }
        match self {
            Base::Url(u) => u
                .join(r)
                .map(|x| x.to_string())
                .unwrap_or_else(|_| r.to_string()),
            Base::Dir(d) => {
                if Path::new(r).is_absolute() {
                    r.to_string()
                } else {
                    d.join(r).to_string_lossy().into_owned()
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
    pub local: PathBuf,
    pub sha256: String,
}

#[derive(Serialize)]
struct ManifestEntry {
    source: String,
    file: String,
    size: usize,
    sha256: String,
    content_type: Option<String>,
    loaded_at: String,
}

pub struct Fetcher {
    dir: PathBuf,
    timeout: Duration,
    max_bytes: u64,
    cache: HashMap<String, Fetched>,
    manifest: Vec<ManifestEntry>,
    counter: usize,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn file_name_hint(source: &str) -> String {
    let tail = source
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("");
    let clean: String = tail
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.is_empty() {
        "input".into()
    } else {
        clean
    }
}

impl Fetcher {
    pub fn new(run_dir: &Path, timeout_s: f64, max_mb: f64) -> Result<Self> {
        let dir = run_dir.join("inputs");
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            timeout: Duration::from_secs_f64(timeout_s),
            max_bytes: (max_mb * 1024.0 * 1024.0) as u64,
            cache: HashMap::new(),
            manifest: Vec::new(),
            counter: 0,
        })
    }

    /// Load a local path or http(s) URL; the same source is loaded only once per run.
    pub fn get(&mut self, source: &str, bearer: Option<&str>) -> Result<Fetched> {
        if let Some(f) = self.cache.get(source) {
            return Ok(f.clone());
        }
        let (bytes, content_type) = if is_url(source) {
            self.download(source, bearer)?
        } else {
            let p = Path::new(source);
            let meta = fs::metadata(p).with_context(|| format!("cannot read {source}"))?;
            if meta.len() > self.max_bytes {
                bail!("{source}: larger than fetch.max_size_mb");
            }
            (
                fs::read(p).with_context(|| format!("cannot read {source}"))?,
                None,
            )
        };
        self.counter += 1;
        let name = format!("{:03}-{}", self.counter, file_name_hint(source));
        let local = self.dir.join(&name);
        fs::write(&local, &bytes)?;
        let sha256 = sha256_hex(&bytes);
        self.manifest.push(ManifestEntry {
            source: source.to_string(),
            file: format!("inputs/{name}"),
            size: bytes.len(),
            sha256: sha256.clone(),
            content_type: content_type.clone(),
            loaded_at: chrono::Local::now().to_rfc3339(),
        });
        let f = Fetched {
            bytes,
            content_type,
            local,
            sha256,
        };
        self.cache.insert(source.to_string(), f.clone());
        Ok(f)
    }

    pub fn get_text(&mut self, source: &str) -> Result<String> {
        let f = self.get(source, None)?;
        let s = String::from_utf8(f.bytes).with_context(|| format!("{source}: not UTF-8 text"))?;
        Ok(s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s))
    }

    fn download(&self, url: &str, bearer: Option<&str>) -> Result<(Vec<u8>, Option<String>)> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .build()
            .into();
        let mut req = agent.get(url).header("User-Agent", "massmailer");
        if let Some(t) = bearer {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let mut resp = req
            .call()
            .with_context(|| format!("download failed: {url}"))?;
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let mut bytes = Vec::new();
        resp.body_mut()
            .as_reader()
            .take(self.max_bytes + 1)
            .read_to_end(&mut bytes)
            .with_context(|| format!("download failed: {url}"))?;
        if bytes.len() as u64 > self.max_bytes {
            bail!("{url}: larger than fetch.max_size_mb");
        }
        Ok((bytes, ct))
    }

    pub fn write_manifest(&self, run_dir: &Path) -> Result<()> {
        fs::write(
            run_dir.join("manifest.json"),
            serde_json::to_vec_pretty(&self.manifest)?,
        )?;
        Ok(())
    }
}
