//! MIME message of one recipient row.

use anyhow::{Result, bail};
use lettre::Message;
use lettre::message::header::{ContentType, HeaderName, HeaderValue};
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};

use crate::render::Rendered;

pub struct Sender {
    pub from: Mailbox,
    /// Set when the mail is sent on behalf of another address: `From` is that
    /// address and `Sender` is `from`.
    pub on_behalf_of: Option<Mailbox>,
    pub reply_to: Option<Mailbox>,
}

#[derive(Default, Clone)]
pub struct Addresses {
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    pub bcc: Vec<Mailbox>,
}

pub struct FileAttachment {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub mime: String,
}

fn raw(name: &'static str, value: &str) -> HeaderValue {
    HeaderValue::new(HeaderName::new_from_ascii_str(name), value.to_string())
}

fn random_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut seed = [0u8; 16];
    getrandom_fill(&mut seed);
    let tail: String = seed.iter().map(|b| format!("{b:02x}")).collect();
    format!("{nanos:x}.{tail}")
}

/// 16 random bytes from the OS-seeded hasher (no extra dependency).
fn getrandom_fill(buf: &mut [u8; 16]) {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    for chunk in buf.chunks_mut(8) {
        let mut h = RandomState::new().build_hasher();
        h.write_usize(chunk.as_ptr() as usize);
        let v = h.finish().to_le_bytes();
        chunk.copy_from_slice(&v[..chunk.len()]);
    }
}

pub fn content_type_for(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();
    match lower.rsplit('.').next().unwrap_or("") {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "ics" => "text/calendar",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "odt" => "application/vnd.oasis.opendocument.text",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

pub struct Extra<'a> {
    pub configuration_set: Option<&'a str>,
    pub original_recipients: Option<&'a str>,
}

pub fn build(
    sender: &Sender,
    addr: &Addresses,
    r: &Rendered,
    attachments: &[FileAttachment],
    extra: &Extra,
) -> Result<Message> {
    if addr.to.is_empty() {
        bail!("no To address");
    }
    // Message-ID in the sender's domain (lettre would use the local host name).
    let domain = sender.from.email.domain().to_string();
    let mut b = Message::builder()
        .subject(r.subject.clone())
        .message_id(Some(format!("<{}@{domain}>", random_id())));
    b = match &sender.on_behalf_of {
        Some(principal) => b.from(principal.clone()).sender(sender.from.clone()),
        None => b.from(sender.from.clone()),
    };
    if let Some(rt) = &sender.reply_to {
        b = b.reply_to(rt.clone());
    }
    for m in &addr.to {
        b = b.to(m.clone());
    }
    for m in &addr.cc {
        b = b.cc(m.clone());
    }
    for m in &addr.bcc {
        b = b.bcc(m.clone());
    }
    if let Some(cs) = extra.configuration_set {
        b = b.raw_header(raw("X-SES-CONFIGURATION-SET", cs));
    }
    if let Some(orig) = extra.original_recipients {
        b = b.raw_header(raw("X-Massmailer-Original-Recipients", orig));
    }
    b = b.raw_header(raw("Auto-Submitted", "auto-generated"));

    let text = SinglePart::plain(r.text.clone());
    let content: MultiPart = match &r.html {
        Some(html) => {
            let html_part = SinglePart::html(html.clone());
            if r.images.is_empty() {
                MultiPart::alternative()
                    .singlepart(text)
                    .singlepart(html_part)
            } else {
                let mut related = MultiPart::related().singlepart(html_part);
                for img in &r.images {
                    let ct = ContentType::parse(&img.mime)?;
                    let part =
                        Attachment::new_inline_with_name(img.cid.clone(), img.filename.clone())
                            .body(img.bytes.clone(), ct);
                    related = related.singlepart(part);
                }
                MultiPart::alternative().singlepart(text).multipart(related)
            }
        }
        None if attachments.is_empty() => return Ok(b.singlepart(text)?),
        None => MultiPart::mixed().singlepart(text),
    };
    if attachments.is_empty() {
        return Ok(b.multipart(content)?);
    }
    let mut mixed = if r.html.is_some() {
        MultiPart::mixed().multipart(content)
    } else {
        content
    };
    for a in attachments {
        let ct = ContentType::parse(&a.mime)?;
        mixed = mixed.singlepart(Attachment::new(a.filename.clone()).body(a.bytes.clone(), ct));
    }
    Ok(b.multipart(mixed)?)
}
