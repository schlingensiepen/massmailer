//! `bounces`: fetch SES events (bounce, complaint, delivery, …) from the SQS
//! queue an SNS topic of the configuration set feeds, match them to run
//! folders and record them there.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use aws_credential_types::Credentials;
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::client::identity::Identity;
use serde_json::{Value, json};

use crate::config::Settings;

pub struct Options {
    /// never delete messages from the queue
    pub keep: bool,
    /// long-poll seconds per receive call (0–20)
    pub wait: u32,
}

/// One affected recipient of an event.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipient {
    pub address: String,
    pub diagnostic: String,
}

/// An SES event, reduced to what `bounces` needs.
#[derive(Debug, Clone)]
pub struct Event {
    /// Bounce, Complaint, Delivery, Reject, DeliveryDelay, … or `other`
    pub kind: String,
    /// bounceType / complaint feedback type / delay type
    pub kind_type: String,
    pub subtype: String,
    pub recipients: Vec<Recipient>,
    pub time: String,
    pub tags: HashMap<String, String>,
    pub raw: Value,
}

fn s(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for p in path {
        cur = &cur[*p];
    }
    cur.as_str().unwrap_or_default().to_string()
}

fn addresses(list: &Value, field: Option<&str>) -> Vec<Recipient> {
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let (address, diagnostic) = match field {
                        Some(f) => (
                            r[f].as_str()?.to_string(),
                            r["diagnosticCode"].as_str().unwrap_or_default().to_string(),
                        ),
                        None => (r.as_str()?.to_string(), String::new()),
                    };
                    Some(Recipient {
                        address,
                        diagnostic,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parses an SQS message body: an SNS envelope whose `Message` holds the SES
/// event, or (raw message delivery) the SES event itself.
pub fn parse_event(body: &str) -> Result<Event> {
    let outer: Value = serde_json::from_str(body).context("queue message is not JSON")?;
    let ev: Value = match outer.get("Message").and_then(Value::as_str) {
        Some(m) => serde_json::from_str(m).context("SNS Message is not JSON")?,
        None => outer,
    };
    let kind = ev["eventType"]
        .as_str()
        .or_else(|| ev["notificationType"].as_str())
        .unwrap_or("other")
        .to_string();
    let destination = addresses(&ev["mail"]["destination"], None);
    let (kind_type, subtype, recipients, time) = match kind.as_str() {
        "Bounce" => (
            s(&ev, &["bounce", "bounceType"]),
            s(&ev, &["bounce", "bounceSubType"]),
            addresses(&ev["bounce"]["bouncedRecipients"], Some("emailAddress")),
            s(&ev, &["bounce", "timestamp"]),
        ),
        "Complaint" => (
            s(&ev, &["complaint", "complaintFeedbackType"]),
            s(&ev, &["complaint", "complaintSubType"]),
            addresses(
                &ev["complaint"]["complainedRecipients"],
                Some("emailAddress"),
            ),
            s(&ev, &["complaint", "timestamp"]),
        ),
        "Delivery" => (
            String::new(),
            String::new(),
            addresses(&ev["delivery"]["recipients"], None),
            s(&ev, &["delivery", "timestamp"]),
        ),
        "DeliveryDelay" => (
            s(&ev, &["deliveryDelay", "delayType"]),
            String::new(),
            addresses(
                &ev["deliveryDelay"]["delayedRecipients"],
                Some("emailAddress"),
            ),
            s(&ev, &["deliveryDelay", "timestamp"]),
        ),
        "Reject" => (
            s(&ev, &["reject", "reason"]),
            String::new(),
            destination,
            s(&ev, &["mail", "timestamp"]),
        ),
        _ => (
            String::new(),
            String::new(),
            destination,
            s(&ev, &["mail", "timestamp"]),
        ),
    };
    let mut tags = HashMap::new();
    if let Some(t) = ev["mail"]["tags"].as_object() {
        for (k, v) in t {
            if let Some(first) = v.as_array().and_then(|a| a.first()).and_then(Value::as_str) {
                tags.insert(k.clone(), first.to_string());
            }
        }
    }
    Ok(Event {
        kind,
        kind_type,
        subtype,
        recipients,
        time,
        tags,
        raw: ev,
    })
}

/// One sent mail of a run, from its log.jsonl.
#[derive(Debug, Clone)]
struct Sent {
    mail: String,
    line: u64,
}

struct Run {
    dir: PathBuf,
    by_mail: HashMap<String, Sent>,
    by_address: HashMap<String, Sent>,
}

/// Run folders of `state_dir` that contain a log.jsonl, newest first.
fn load_runs(state_dir: &Path) -> Result<Vec<Run>> {
    let mut dirs: Vec<PathBuf> = match fs::read_dir(state_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.join("log.jsonl").is_file())
            .collect(),
        Err(_) => Vec::new(),
    };
    dirs.sort();
    dirs.reverse();
    let mut runs = Vec::new();
    for dir in dirs {
        let mut by_mail = HashMap::new();
        let mut by_address = HashMap::new();
        for l in fs::read_to_string(dir.join("log.jsonl"))?.lines() {
            let Ok(v) = serde_json::from_str::<Value>(l) else {
                continue;
            };
            if v["status"] != "sent" {
                continue;
            }
            let mail = v["mail"]
                .as_str()
                .unwrap_or_default()
                .trim_start_matches("mails/")
                .trim_end_matches(".eml")
                .to_string();
            let sent = Sent {
                mail: mail.clone(),
                line: v["line"].as_u64().unwrap_or_default(),
            };
            for a in v["to"].as_str().unwrap_or_default().split(',') {
                let a = a.trim().to_lowercase();
                if !a.is_empty() {
                    by_address.insert(a, sent.clone());
                }
            }
            by_mail.insert(mail, sent);
        }
        runs.push(Run {
            dir,
            by_mail,
            by_address,
        });
    }
    Ok(runs)
}

/// Run index and mail for one recipient of an event: the run named in the
/// SES tags, otherwise the newest run that sent to the address.
fn find<'a>(runs: &'a [Run], ev: &Event, address: &str) -> Option<(usize, Option<&'a Sent>)> {
    let addr = address.to_lowercase();
    if let Some(name) = ev.tags.get("massmailer-run")
        && let Some(i) = runs.iter().position(|r| {
            r.dir
                .file_name()
                .is_some_and(|n| n.to_string_lossy() == *name)
        })
    {
        let r = &runs[i];
        let sent = ev
            .tags
            .get("massmailer-mail")
            .and_then(|m| r.by_mail.get(m))
            .or_else(|| r.by_address.get(&addr));
        return Some((i, sent));
    }
    runs.iter()
        .position(|r| r.by_address.contains_key(&addr))
        .map(|i| (i, runs[i].by_address.get(&addr)))
}

fn append(path: &Path, line: &str) -> Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    writeln!(f, "{line}")?;
    Ok(())
}

fn csv_field(v: &str) -> String {
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}

const CSV_HEADER: &str = "mail,line,address,event,type,subtype,diagnostic,time";

/// Writes one event into its run folder(s) or the unmatched file. Returns the
/// run folders it was written to (empty: unmatched).
fn record(state_dir: &Path, runs: &[Run], ev: &Event) -> Result<Vec<PathBuf>> {
    // group recipients by run
    let mut per_run: BTreeMap<usize, Vec<(&Recipient, Option<&Sent>)>> = BTreeMap::new();
    let mut unmatched = Vec::new();
    for r in &ev.recipients {
        match find(runs, ev, &r.address) {
            Some((i, sent)) => per_run.entry(i).or_default().push((r, sent)),
            None => unmatched.push(r),
        }
    }
    let received = chrono::Local::now().to_rfc3339();
    if per_run.is_empty() || !unmatched.is_empty() {
        let entry = json!({"received": received, "event": ev.kind, "raw": ev.raw});
        append(
            &state_dir.join("unmatched-events.jsonl"),
            &entry.to_string(),
        )?;
    }
    let mut written = Vec::new();
    for (i, list) in per_run {
        let dir = &runs[i].dir;
        let entry = json!({"received": received, "event": ev.kind, "raw": ev.raw});
        append(&dir.join("events.jsonl"), &entry.to_string())?;
        if ev.kind != "Delivery" {
            let csv = dir.join("bounces.csv");
            if !csv.exists() {
                append(&csv, CSV_HEADER)?;
            }
            for (r, sent) in list {
                let row = [
                    sent.map(|s| s.mail.clone()).unwrap_or_default(),
                    sent.map(|s| s.line.to_string()).unwrap_or_default(),
                    r.address.clone(),
                    ev.kind.clone(),
                    ev.kind_type.clone(),
                    ev.subtype.clone(),
                    r.diagnostic.clone(),
                    ev.time.clone(),
                ]
                .iter()
                .map(|v| csv_field(v))
                .collect::<Vec<_>>()
                .join(",");
                append(&csv, &row)?;
            }
        }
        written.push(dir.clone());
    }
    Ok(written)
}

/// Minimal SQS client (AWS JSON 1.0 protocol, SigV4) on top of ureq.
struct Sqs {
    agent: ureq::Agent,
    endpoint: String,
    host: String,
    region: String,
    queue_url: String,
    identity: Identity,
}

impl Sqs {
    fn new(s: &Settings) -> Result<Self> {
        let queue_url = s.req("aws.sqs_queue_url")?;
        let url = url::Url::parse(&queue_url).context("aws.sqs_queue_url is not a URL")?;
        let host = url
            .host_str()
            .context("aws.sqs_queue_url has no host")?
            .to_string();
        let region = match s.str("aws.region") {
            Some(r) => r,
            None => host
                .split('.')
                .nth(1)
                .filter(|_| host.starts_with("sqs."))
                .context("aws.region missing")?
                .to_string(),
        };
        let identity: Identity = Credentials::new(
            s.req("aws.access_key_id")?,
            s.req("aws.secret_access_key")?,
            None,
            None,
            "massmailer",
        )
        .into();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self {
            agent,
            endpoint: format!("https://{host}/"),
            host,
            region,
            queue_url,
            identity,
        })
    }

    fn call(&self, action: &str, body: &Value) -> Result<Value> {
        let body = body.to_string();
        let target = format!("AmazonSQS.{action}");
        let headers = [
            ("host", self.host.as_str()),
            ("content-type", "application/x-amz-json-1.0"),
            ("x-amz-target", target.as_str()),
        ];
        let params = v4::SigningParams::builder()
            .identity(&self.identity)
            .region(&self.region)
            .name("sqs")
            .time(SystemTime::now())
            .settings(SigningSettings::default())
            .build()
            .context("signing parameters")?
            .into();
        let signable = SignableRequest::new(
            "POST",
            &self.endpoint,
            headers.iter().copied(),
            SignableBody::Bytes(body.as_bytes()),
        )
        .context("signable request")?;
        let (instructions, _) = sign(signable, &params)
            .context("signing the SQS request")?
            .into_parts();
        let mut req = self
            .agent
            .post(&self.endpoint)
            .header("content-type", "application/x-amz-json-1.0")
            .header("x-amz-target", &target);
        for (k, v) in instructions.headers() {
            req = req.header(k, v);
        }
        let mut resp = req
            .send(body.as_bytes())
            .with_context(|| format!("SQS {action}: request failed"))?;
        let status = resp.status();
        let text = resp
            .body_mut()
            .read_to_string()
            .with_context(|| format!("SQS {action}: no response body"))?;
        if !status.is_success() {
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            let kind = v["__type"].as_str().unwrap_or_default();
            let msg = v["message"]
                .as_str()
                .or_else(|| v["Message"].as_str())
                .unwrap_or(&text);
            bail!("SQS {action}: HTTP {status} {kind} {msg}");
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).with_context(|| format!("SQS {action}: response is not JSON"))
    }

    fn receive(&self, wait: u32) -> Result<Vec<(String, String)>> {
        let v = self.call(
            "ReceiveMessage",
            &json!({
                "QueueUrl": self.queue_url,
                "MaxNumberOfMessages": 10,
                "WaitTimeSeconds": wait.min(20),
            }),
        )?;
        Ok(v["Messages"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| {
                        Some((
                            m["ReceiptHandle"].as_str()?.to_string(),
                            m["Body"].as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn delete(&self, receipt: &str) -> Result<()> {
        self.call(
            "DeleteMessage",
            &json!({"QueueUrl": self.queue_url, "ReceiptHandle": receipt}),
        )?;
        Ok(())
    }
}

pub fn execute(s: &Settings, opt: &Options) -> Result<()> {
    let state_dir = s
        .str("run.state_dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("massmailer-runs"));
    fs::create_dir_all(&state_dir)
        .with_context(|| format!("cannot create {}", state_dir.display()))?;
    let runs = load_runs(&state_dir)?;
    let sqs = Sqs::new(s)?;
    let mut per_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_run: BTreeMap<String, usize> = BTreeMap::new();
    let (mut total, mut unmatched, mut unreadable) = (0, 0, 0);
    // with --keep, received messages become visible again after the
    // visibility timeout; stop at the first message seen twice
    let mut seen = std::collections::HashSet::new();
    'outer: loop {
        let batch = sqs.receive(opt.wait)?;
        if batch.is_empty() {
            break;
        }
        for (receipt, body) in batch {
            if !seen.insert(body.clone()) {
                break 'outer;
            }
            total += 1;
            match parse_event(&body) {
                Ok(ev) => {
                    let written = record(&state_dir, &runs, &ev)?;
                    *per_kind.entry(ev.kind.clone()).or_default() += 1;
                    if written.is_empty() {
                        unmatched += 1;
                    }
                    for d in written {
                        let name = d
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        *per_run.entry(name).or_default() += 1;
                    }
                }
                Err(e) => {
                    unreadable += 1;
                    let entry = json!({
                        "received": chrono::Local::now().to_rfc3339(),
                        "event": "unreadable",
                        "error": format!("{e:#}"),
                        "body": body,
                    });
                    append(
                        &state_dir.join("unmatched-events.jsonl"),
                        &entry.to_string(),
                    )?;
                }
            }
            if !opt.keep {
                sqs.delete(&receipt)?;
            }
        }
    }
    eprintln!(
        "{total} message(s) from the queue{}",
        if opt.keep { " (kept in the queue)" } else { "" }
    );
    for (k, n) in &per_kind {
        eprintln!("  {k}: {n}");
    }
    for (r, n) in &per_run {
        eprintln!("  run {r}: {n} event(s)");
    }
    if unmatched + unreadable > 0 {
        eprintln!(
            "  {unmatched} unmatched, {unreadable} unreadable: {}",
            state_dir.join("unmatched-events.jsonl").display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sns(inner: &Value) -> String {
        json!({"Type": "Notification", "Message": inner.to_string()}).to_string()
    }

    #[test]
    fn bounce_from_configuration_set() {
        let inner = json!({
            "eventType": "Bounce",
            "bounce": {
                "bounceType": "Permanent",
                "bounceSubType": "General",
                "bouncedRecipients": [{"emailAddress": "anna@example.org", "diagnosticCode": "550 5.1.1 user unknown"}],
                "timestamp": "2026-11-10T15:00:00.000Z"
            },
            "mail": {"destination": ["anna@example.org"], "tags": {"massmailer-run": ["20261110-150000-send"], "massmailer-mail": ["0001"]}}
        });
        let ev = parse_event(&sns(&inner)).unwrap();
        assert_eq!(ev.kind, "Bounce");
        assert_eq!(ev.kind_type, "Permanent");
        assert_eq!(ev.recipients[0].address, "anna@example.org");
        assert!(ev.recipients[0].diagnostic.contains("550"));
        assert_eq!(ev.tags["massmailer-mail"], "0001");
    }

    #[test]
    fn complaint_from_identity_notification_raw() {
        let inner = json!({
            "notificationType": "Complaint",
            "complaint": {"complainedRecipients": [{"emailAddress": "b@example.org"}], "complaintFeedbackType": "abuse", "timestamp": "t"},
            "mail": {"destination": ["b@example.org"]}
        });
        let ev = parse_event(&inner.to_string()).unwrap();
        assert_eq!(ev.kind, "Complaint");
        assert_eq!(ev.kind_type, "abuse");
        assert_eq!(ev.recipients.len(), 1);
    }

    #[test]
    fn matching_by_tag_and_by_address() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-runs/bounces");
        let _ = fs::remove_dir_all(&base);
        for (name, to) in [
            ("20261101-100000-send", "anna@example.org"),
            ("20261110-150000-send", "Anna@Example.org"),
        ] {
            let d = base.join(name);
            fs::create_dir_all(&d).unwrap();
            let l = json!({"mail": "mails/0001.eml", "line": 2, "to": to, "status": "sent"});
            fs::write(d.join("log.jsonl"), format!("{l}\n")).unwrap();
        }
        let runs = load_runs(&base).unwrap();
        let mut ev = parse_event(
            &json!({"eventType": "Bounce", "bounce": {"bounceType": "Transient", "bouncedRecipients": [{"emailAddress": "anna@example.org"}]}, "mail": {}})
                .to_string(),
        )
        .unwrap();
        // newest run by address
        let (i, sent) = find(&runs, &ev, "anna@example.org").unwrap();
        assert!(runs[i].dir.ends_with("20261110-150000-send"));
        assert_eq!(sent.unwrap().line, 2);
        // explicit run tag wins
        ev.tags
            .insert("massmailer-run".into(), "20261101-100000-send".into());
        let (i, _) = find(&runs, &ev, "anna@example.org").unwrap();
        assert!(runs[i].dir.ends_with("20261101-100000-send"));
        // record writes CSV into the run
        record(&base, &runs, &ev).unwrap();
        let csv = fs::read_to_string(base.join("20261101-100000-send/bounces.csv")).unwrap();
        assert!(csv.starts_with(CSV_HEADER));
        assert!(csv.contains("0001,2,anna@example.org,Bounce,Transient"));
        // unknown address goes to unmatched
        let ev2 = parse_event(
            &json!({"eventType": "Bounce", "bounce": {"bouncedRecipients": [{"emailAddress": "x@example.net"}]}, "mail": {}}).to_string(),
        )
        .unwrap();
        assert!(record(&base, &runs, &ev2).unwrap().is_empty());
        assert!(base.join("unmatched-events.jsonl").exists());
    }
}
