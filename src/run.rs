//! One run: run folder, recipients, rendering, sending (or dry run).

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lettre::message::Mailbox;
use lettre::transport::smtp::PoolConfig;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{SmtpTransport, Transport};
use serde_json::json;

use crate::config::Settings;
use crate::fetch::{Base, Fetcher};
use crate::mail::{self, Addresses, Extra, FileAttachment, Sender};
use crate::recipients::{self, Row, Table};
use crate::render::{Format, Images, Letterhead, Renderer, RendererConfig};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    /// validate everything, write nothing but the run folder
    Check,
    /// write .eml, .html and .txt per mail
    Render,
    /// full run without SMTP
    DryRun,
    /// send via SMTP
    Send,
}

pub struct Options {
    pub mode: Mode,
    pub yes: bool,
    pub verbose: bool,
}

fn mailbox(address: &str, name: Option<String>) -> Result<Mailbox> {
    let a = address
        .parse()
        .with_context(|| format!("invalid address `{address}`"))?;
    Ok(Mailbox::new(name, a))
}

fn run_dir(s: &Settings, mode: Mode) -> Result<PathBuf> {
    let base = s
        .str("run.state_dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("massmailer-runs"));
    let suffix = match mode {
        Mode::Check => "check",
        Mode::Render => "render",
        Mode::DryRun => "dry-run",
        Mode::Send => "send",
    };
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut dir = base.join(format!("{stamp}-{suffix}"));
    let mut n = 2;
    while dir.exists() {
        dir = base.join(format!("{stamp}-{suffix}-{n}"));
        n += 1;
    }
    fs::create_dir_all(dir.join("mails"))
        .with_context(|| format!("cannot create {}", dir.display()))?;
    Ok(dir)
}

pub fn load_table(s: &Settings, fetcher: &mut Fetcher) -> Result<Table> {
    let source = s.req("recipients.source")?;
    let bearer = s.str("recipients.url.bearer_token");
    let f = fetcher.get(&source, bearer.as_deref())?;
    recipients::read(
        &source,
        &f,
        s.str("recipients.format").as_deref(),
        s.str("recipients.csv.delimiter").as_deref(),
        s.str("recipients.sheet").as_deref(),
    )
}

/// Print the column names of the recipient list (no data).
pub fn columns(s: &Settings) -> Result<()> {
    let tmp = std::env::temp_dir().join(format!("massmailer-columns-{}", std::process::id()));
    let mut fetcher = Fetcher::new(&tmp, 30.0, 50.0)?;
    let t = load_table(s, &mut fetcher);
    let _ = fs::remove_dir_all(&tmp);
    let t = t?;
    println!("{} rows; columns:", t.rows.len());
    for c in &t.columns {
        println!("  {c}");
    }
    Ok(())
}

fn addresses_from(
    row: &Row,
    cols: &[String],
    fixed: &[String],
    what: &str,
) -> Result<Vec<Mailbox>> {
    let mut out = Vec::new();
    for c in cols {
        let v = row
            .get(c)
            .with_context(|| format!("{what} column `{c}` not in recipient list"))?;
        out.extend(recipients::parse_addresses(v)?);
    }
    for f in fixed {
        out.extend(recipients::parse_addresses(f)?);
    }
    Ok(out)
}

fn confirm(question: &str) -> Result<bool> {
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes" | "j" | "ja"
    ))
}

pub fn execute(s: &Settings, opt: &Options) -> Result<()> {
    let dir = run_dir(s, opt.mode)?;
    eprintln!("run folder: {}", dir.display());
    if opt.verbose {
        for l in s.describe() {
            eprintln!("  {l}");
        }
    }
    let mut fetcher = Fetcher::new(
        &dir,
        s.num("fetch.timeout_seconds")?.unwrap_or(30.0),
        s.num("fetch.max_size_mb")?.unwrap_or(20.0),
    )?;

    // inputs
    let body_src = s.req("message.body")?;
    let format = Format::detect(&body_src, s.str("message.format").as_deref())?;
    let body = fetcher.get_text(&body_src)?;
    let text_override = match s.str("message.text") {
        Some(t) => Some(fetcher.get_text(&t)?),
        None => None,
    };
    let letterhead = match s.str("letterhead.dir") {
        Some(d) => Some(Letterhead::load(&d, &mut fetcher)?),
        None => None,
    };
    let images = match s.str("message.images").as_deref() {
        None | Some("embed") => Images::Embed,
        Some("link") => Images::Link,
        Some(o) => bail!("message.images must be embed or link, not `{o}`"),
    };
    let text_only = match s.str("message.parts").as_deref() {
        None | Some("both") => false,
        Some("text") => true,
        Some(o) => bail!("message.parts must be both or text, not `{o}`"),
    };
    let mut renderer = Renderer::new(RendererConfig {
        format,
        body,
        body_base: Base::of(&body_src)?,
        text_override,
        placeholders: s.bool("message.placeholders", true)?,
        subject: s.req("message.subject")?,
        preheader: s.str("message.preheader"),
        letterhead,
        images,
        text_only,
        text_width: s.num("message.text_width")?.unwrap_or(78.0) as usize,
    });

    let sender = Sender {
        from: mailbox(&s.req("sender.address")?, s.str("sender.name"))?,
        on_behalf_of: match s.str("sender.on_behalf_of") {
            Some(a) => Some(
                a.parse()
                    .with_context(|| format!("invalid sender.on_behalf_of `{a}`"))?,
            ),
            None => None,
        },
        reply_to: match s.str("sender.reply_to") {
            Some(a) => Some(
                a.parse()
                    .with_context(|| format!("invalid sender.reply_to `{a}`"))?,
            ),
            None => None,
        },
    };
    let redirect = match s.str("sending.redirect_to") {
        Some(a) => Some(mailbox(&a, None)?),
        None => None,
    };
    let max_mb = s.num("message.max_mail_size_mb")?.unwrap_or(5.0);
    if max_mb > 40.0 {
        bail!("message.max_mail_size_mb may not exceed 40 (Amazon SES limit after encoding)");
    }

    // attachments for all
    let mut common_attachments = Vec::new();
    for a in s.list("message.attachments") {
        let f = fetcher.get(&a, None)?;
        let filename = a
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("attachment")
            .to_string();
        common_attachments.push((
            FileAttachment {
                mime: mail::content_type_for(&filename).into(),
                filename,
                bytes: f.bytes,
            },
            f.sha256,
        ));
    }

    // recipients
    let table = load_table(s, &mut fetcher)?;
    let to_cols = s.list("recipients.to_columns");
    let cc_cols = s.list("recipients.cc_columns");
    let bcc_cols = s.list("recipients.bcc_columns");
    let (fixed_to, fixed_cc, fixed_bcc) = (
        s.list("message.to"),
        s.list("message.cc"),
        s.list("message.bcc"),
    );
    if to_cols.is_empty() && fixed_to.is_empty() {
        bail!("set recipients.to_columns or message.to");
    }
    let skip_col = s.str("recipients.skip_column");
    let limit = s.num("sending.limit")?.map(|n| n as usize);
    let att_col = s.str("message.attachment_column");
    let att_dir = s.str("message.attachment_dir");

    let mut rows: Vec<(usize, Row)> = Vec::new();
    let mut skipped = 0;
    for (i, row) in table.rows.iter().enumerate() {
        if let Some(c) = &skip_col
            && row.get(c).is_some_and(|v| recipients::is_skip_value(v))
        {
            skipped += 1;
            continue;
        }
        rows.push((i + 2, recipients::sanitize(row)));
    }
    if let Some(l) = limit {
        rows.truncate(l);
    }

    // build all mails first; nothing is sent if one fails
    struct Prepared {
        line: usize,
        to: String,
        message: lettre::Message,
        bytes: usize,
        attachments: Vec<(String, String)>,
    }
    let mut prepared = Vec::new();
    let configuration_set = s.str("ses.configuration_set");
    let run_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut warnings_seen = std::collections::BTreeSet::new();
    for (line, row) in &rows {
        let ctx = || format!("recipient list line {line}");
        let mut addr = Addresses {
            to: addresses_from(row, &to_cols, &fixed_to, "to").with_context(ctx)?,
            cc: addresses_from(row, &cc_cols, &fixed_cc, "cc").with_context(ctx)?,
            bcc: addresses_from(row, &bcc_cols, &fixed_bcc, "bcc").with_context(ctx)?,
        };
        if addr.to.is_empty() {
            bail!("{}: no To address", ctx());
        }
        let original: Vec<String> = addr
            .to
            .iter()
            .chain(&addr.cc)
            .chain(&addr.bcc)
            .map(|m| m.email.to_string())
            .collect();
        let mut rendered = renderer.render(row, &mut fetcher).with_context(ctx)?;
        for w in &rendered.warnings {
            if warnings_seen.insert(w.clone()) {
                eprintln!("warning: {w}");
            }
        }
        if let Some(r) = &redirect {
            rendered.subject = format!("[TEST an {}] {}", original.join(", "), rendered.subject);
            addr = Addresses {
                to: vec![r.clone()],
                ..Default::default()
            };
        }
        let mut atts: Vec<FileAttachment> = common_attachments
            .iter()
            .map(|(a, _)| FileAttachment {
                filename: a.filename.clone(),
                bytes: a.bytes.clone(),
                mime: a.mime.clone(),
            })
            .collect();
        let mut att_log: Vec<(String, String)> = common_attachments
            .iter()
            .map(|(a, h)| (a.filename.clone(), h.clone()))
            .collect();
        if let Some(col) = &att_col {
            let v = row
                .get(col)
                .with_context(|| format!("attachment column `{col}` not in recipient list"))?;
            if !v.is_empty() {
                let src = match &att_dir {
                    Some(d) if !crate::config::is_url(v) && !Path::new(v).is_absolute() => {
                        Path::new(d).join(v).to_string_lossy().into_owned()
                    }
                    _ => v.clone(),
                };
                let f = fetcher.get(&src, None).with_context(ctx)?;
                let filename = v
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or("attachment")
                    .to_string();
                att_log.push((filename.clone(), f.sha256.clone()));
                atts.push(FileAttachment {
                    mime: mail::content_type_for(&filename).into(),
                    filename,
                    bytes: f.bytes,
                });
            }
        }
        let orig_header = redirect.as_ref().map(|_| original.join(", "));
        // lets `bounces` match SES events to this run and mail (SES replaces the Message-ID)
        let ses_tags = configuration_set.as_ref().map(|_| {
            format!(
                "massmailer-run={run_name}, massmailer-mail={:04}",
                prepared.len() + 1
            )
        });
        let message = mail::build(
            &sender,
            &addr,
            &rendered,
            &atts,
            &Extra {
                configuration_set: configuration_set.as_deref(),
                original_recipients: orig_header.as_deref(),
                ses_tags: ses_tags.as_deref(),
            },
        )
        .with_context(ctx)?;
        let raw = message.formatted();
        if raw.len() as f64 > max_mb * 1024.0 * 1024.0 {
            bail!(
                "{}: mail is {:.1} MB, above message.max_mail_size_mb ({max_mb} MB)",
                ctx(),
                raw.len() as f64 / 1048576.0
            );
        }
        let n = prepared.len() + 1;
        fs::write(dir.join("mails").join(format!("{n:04}.eml")), &raw)?;
        if opt.mode == Mode::Render {
            if let Some(h) = &rendered.html {
                fs::write(dir.join("mails").join(format!("{n:04}.html")), h)?;
            }
            fs::write(
                dir.join("mails").join(format!("{n:04}.txt")),
                &rendered.text,
            )?;
        }
        prepared.push(Prepared {
            line: *line,
            to: addr
                .to
                .iter()
                .map(|m| m.email.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            message,
            bytes: raw.len(),
            attachments: att_log,
        });
    }
    fetcher.write_manifest(&dir)?;
    eprintln!(
        "{} mail(s) prepared from {} row(s){}{}",
        prepared.len(),
        table.rows.len(),
        if skipped > 0 {
            format!(", {skipped} skipped")
        } else {
            String::new()
        },
        if redirect.is_some() {
            ", redirected"
        } else {
            ""
        },
    );
    if matches!(opt.mode, Mode::Check | Mode::Render) {
        return Ok(());
    }

    // sending (or dry run)
    let connections = s
        .num("sending.connections")?
        .map(|n| n as usize)
        .unwrap_or(3)
        .clamp(1, 10);
    let transport = if opt.mode == Mode::Send {
        if !opt.yes && !confirm(&format!("Send {} mail(s) now?", prepared.len()))? {
            bail!("aborted, nothing sent");
        }
        let host = s.req("smtp.host")?;
        let security = s.str("smtp.security").unwrap_or_else(|| "starttls".into());
        let builder = match security.as_str() {
            "starttls" => SmtpTransport::starttls_relay(&host)?,
            "tls" => SmtpTransport::relay(&host)?,
            o => bail!("smtp.security must be starttls or tls, not `{o}`"),
        };
        let port = s
            .num("smtp.port")?
            .unwrap_or(if security == "tls" { 465.0 } else { 587.0 }) as u16;
        Some(
            builder
                .port(port)
                .credentials(Credentials::new(
                    s.req("smtp.username")?,
                    s.req("smtp.password")?,
                ))
                .timeout(Some(Duration::from_secs_f64(
                    s.num("smtp.timeout_seconds")?.unwrap_or(30.0),
                )))
                .pool_config(PoolConfig::new().max_size(connections as u32))
                .build(),
        )
    } else {
        None
    };
    if let Some(t) = &transport {
        t.test_connection()
            .context("SMTP connection or login failed")?;
    }
    let rate = s.num("sending.rate_per_second")?.unwrap_or(1.0).max(0.01);
    let gap = Duration::from_secs_f64(1.0 / rate);
    let mut log = File::create(dir.join("log.jsonl"))?;
    let (mut ok, mut failed) = (0, 0);
    let total = prepared.len();
    let dry = opt.mode == Mode::DryRun;
    // writes the log line and progress of mail i (called in mail order)
    let mut record = |i: usize, result: Result<String, String>| -> Result<()> {
        let p = &prepared[i];
        let message_id = p
            .message
            .headers()
            .get_raw("Message-ID")
            .unwrap_or_default()
            .to_string();
        let (status, detail) = match result {
            Ok(m) => {
                ok += 1;
                ("sent", m)
            }
            Err(e) => {
                failed += 1;
                ("failed", e)
            }
        };
        let status = if dry { "dry-run" } else { status };
        let entry = json!({
            "time": chrono::Local::now().to_rfc3339(),
            "mail": format!("mails/{:04}.eml", i + 1),
            "line": p.line,
            "to": p.to,
            "message_id": message_id,
            "bytes": p.bytes,
            "attachments": p.attachments.iter().map(|(f, h)| json!({"file": f, "sha256": h})).collect::<Vec<_>>(),
            "status": status,
            "detail": detail,
        });
        writeln!(log, "{entry}")?;
        eprintln!("{:>4}/{total} {status} {}", i + 1, p.to);
        Ok(())
    };
    match &transport {
        None => {
            for i in 0..total {
                record(i, Ok("dry run, not sent".to_string()))?;
            }
        }
        Some(t) => {
            // A dispatcher releases one mail per `gap`; `connections` workers
            // send over pooled SMTP connections; results are logged in order.
            let (job_tx, job_rx) = mpsc::sync_channel::<usize>(0);
            let job_rx = Mutex::new(job_rx);
            let (res_tx, res_rx) = mpsc::channel::<(usize, Result<String, String>)>();
            let prepared = &prepared;
            let mut write_error = None;
            std::thread::scope(|sc| {
                for _ in 0..connections.min(total.max(1)) {
                    let res_tx = res_tx.clone();
                    let job_rx = &job_rx;
                    sc.spawn(move || {
                        loop {
                            let next = job_rx.lock().map(|rx| rx.recv());
                            let Ok(Ok(i)) = next else { break };
                            let r = t
                                .send(&prepared[i].message)
                                .map(|r| r.message().collect::<Vec<_>>().join(" "))
                                .map_err(|e| e.to_string());
                            if res_tx.send((i, r)).is_err() {
                                break;
                            }
                        }
                    });
                }
                drop(res_tx);
                sc.spawn(move || {
                    let mut last: Option<Instant> = None;
                    for i in 0..total {
                        if let Some(l) = last
                            && let Some(rest) = gap.checked_sub(l.elapsed())
                        {
                            std::thread::sleep(rest);
                        }
                        // blocks until a worker is free
                        if job_tx.send(i).is_err() {
                            break;
                        }
                        last = Some(Instant::now());
                    }
                });
                let mut pending = BTreeMap::new();
                let mut next = 0;
                for (i, r) in res_rx {
                    pending.insert(i, r);
                    while let Some(r) = pending.remove(&next) {
                        if write_error.is_none()
                            && let Err(e) = record(next, r)
                        {
                            write_error = Some(e);
                        }
                        next += 1;
                    }
                }
            });
            if let Some(e) = write_error {
                return Err(e);
            }
        }
    }
    eprintln!(
        "done: {ok} ok, {failed} failed; log: {}",
        dir.join("log.jsonl").display()
    );
    if failed > 0 {
        bail!("{failed} mail(s) failed");
    }
    Ok(())
}
