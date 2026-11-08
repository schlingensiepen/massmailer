//! Render pipeline spike (FUNC-003 in the workshop).
//!
//! Order under test:
//! 1. fill placeholders in the Markdown template (values escaped for Markdown),
//! 2. derive the HTML part (comrak) and the text part (own walker over the same AST),
//! 3. insert both into the letterhead frames `layout.html` / `layout.txt`,
//! 4. inline `style.css` into the HTML,
//! 5. build the MIME message (multipart/alternative, logo as CID in multipart/related).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use comrak::nodes::{ListType, NodeValue};
use comrak::{Arena, Options, format_html, parse_document};
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::Message;
use minijinja::{Environment, ErrorKind, UndefinedBehavior, Value};

/// Content ID under which the logo is referenced in `layout.html` (`cid:logo`).
pub const LOGO_CID: &str = "logo";

/// A letterhead directory: frames for both parts, stylesheet, optional logo.
pub struct Letterhead {
    pub layout_html: String,
    pub layout_txt: String,
    pub css: String,
    pub logo_png: Option<Vec<u8>>,
}

impl Letterhead {
    pub fn load(dir: &Path) -> Result<Self> {
        let read = |name: &str| {
            fs::read_to_string(dir.join(name))
                .with_context(|| format!("letterhead: cannot read {}", dir.join(name).display()))
        };
        let layout_html = read("layout.html")?;
        let layout_txt = read("layout.txt")?;
        for (name, body) in [("layout.html", &layout_html), ("layout.txt", &layout_txt)] {
            anyhow::ensure!(body.contains("{{ content }}"), "letterhead: {name} lacks {{{{ content }}}}");
        }
        let css = fs::read_to_string(dir.join("style.css")).unwrap_or_default();
        let logo_png = fs::read(dir.join("logo.png")).ok();
        Ok(Self { layout_html, layout_txt, css, logo_png })
    }
}

/// Both parts of one mail, already framed and (HTML) inlined.
pub struct Rendered {
    pub html: String,
    pub text: String,
}

/// Escape every ASCII punctuation character with a backslash (CommonMark allows
/// this for all of them), so a value can never introduce Markdown or HTML syntax.
pub fn escape_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        // A line break inside a value must not start a new block.
        out.push(if c == '\n' || c == '\r' { ' ' } else { c });
    }
    out
}

/// Step 1: fill the Markdown template; every value is Markdown-escaped.
pub fn fill_template(template: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_formatter(|out, _state, value| {
        let s = if value.is_safe() { value.to_string() } else { escape_markdown(&value.to_string()) };
        out.write_str(&s)
            .map_err(|_| minijinja::Error::new(ErrorKind::WriteFailure, "write failed"))
    });
    env.add_template("message.md", template)?;
    Ok(env.get_template("message.md")?.render(values)?)
}

fn markdown_options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.autolink = true;
    o.extension.strikethrough = true;
    o.extension.table = true;
    o.render.r#unsafe = false; // raw HTML in the template is dropped
    o
}

/// Step 2: HTML part and text part from the same parsed Markdown.
pub fn markdown_to_parts(md: &str) -> Result<(String, String)> {
    let arena = Arena::new();
    let options = markdown_options();
    let root = parse_document(&arena, md, &options);
    let mut html = String::new();
    format_html(root, &options, &mut html)?;
    let mut text = String::new();
    TextWriter::default().block(root, &mut text);
    Ok((html, text.trim_end().to_string()))
}

/// Plain-text rendering of the Markdown AST: no markup characters, links as
/// "label <url>", headings underlined, lists with "- " / "1. ".
#[derive(Default)]
struct TextWriter {
    indent: usize,
}

impl TextWriter {
    fn block<'a>(&mut self, node: &'a comrak::nodes::AstNode<'a>, out: &mut String) {
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::Document => {
                for child in node.children() {
                    self.block(child, out);
                }
            }
            NodeValue::Heading(h) => {
                let line = inline_text(node);
                out.push_str(&line);
                out.push('\n');
                let ch = if h.level == 1 { '=' } else { '-' };
                out.push_str(&ch.to_string().repeat(line.chars().count()));
                out.push_str("\n\n");
            }
            NodeValue::Paragraph => {
                let pad = " ".repeat(self.indent);
                for (i, line) in inline_text(node).lines().enumerate() {
                    if i > 0 {
                        out.push_str(&pad);
                    }
                    out.push_str(line);
                    out.push('\n');
                }
                out.push('\n');
            }
            NodeValue::List(list) => {
                for (i, item) in node.children().enumerate() {
                    let marker = match list.list_type {
                        ListType::Bullet => "- ".to_string(),
                        ListType::Ordered => format!("{}. ", list.start + i),
                    };
                    out.push_str(&" ".repeat(self.indent));
                    out.push_str(&marker);
                    self.indent += marker.len();
                    let mut item_text = String::new();
                    for child in item.children() {
                        self.block(child, &mut item_text);
                    }
                    self.indent -= marker.len();
                    // tight lists: drop the blank line paragraphs add
                    out.push_str(item_text.trim_end());
                    out.push('\n');
                }
                out.push('\n');
            }
            NodeValue::BlockQuote => {
                let mut inner = String::new();
                for child in node.children() {
                    self.block(child, &mut inner);
                }
                for line in inner.trim_end().lines() {
                    let _ = writeln!(out, "> {line}");
                }
                out.push('\n');
            }
            NodeValue::CodeBlock(cb) => {
                for line in cb.literal.lines() {
                    let _ = writeln!(out, "    {line}");
                }
                out.push('\n');
            }
            NodeValue::ThematicBreak => out.push_str("----------\n\n"),
            NodeValue::HtmlBlock(_) => {}
            _ => {
                out.push_str(&inline_text(node));
                out.push_str("\n\n");
            }
        }
    }
}

fn inline_text<'a>(node: &'a comrak::nodes::AstNode<'a>) -> String {
    let mut s = String::new();
    for child in node.children() {
        inline_into(child, &mut s);
    }
    s
}

fn inline_into<'a>(node: &'a comrak::nodes::AstNode<'a>, s: &mut String) {
    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Text(t) => s.push_str(&t),
        NodeValue::Code(c) => s.push_str(&c.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => s.push('\n'),
        NodeValue::Link(link) => {
            let label = inline_text(node);
            let url = link.url.strip_prefix("mailto:").unwrap_or(&link.url);
            if label == url || label == link.url {
                s.push_str(&label);
            } else {
                let _ = write!(s, "{label} <{url}>");
            }
        }
        NodeValue::Image(img) => {
            let _ = write!(s, "[{}]", if img.title.is_empty() { inline_text(node) } else { img.title.clone() });
        }
        NodeValue::HtmlInline(_) => {}
        _ => {
            for child in node.children() {
                inline_into(child, s);
            }
        }
    }
}

/// Steps 3 and 4: insert into the frames and inline the CSS.
pub fn frame(html_body: &str, text_body: &str, lh: &Letterhead) -> Result<Rendered> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    // ".html" enables HTML auto-escaping; the body is passed as safe.
    env.add_template("layout.html", &lh.layout_html)?;
    env.add_template("layout.txt", &lh.layout_txt)?;
    let html = env
        .get_template("layout.html")?
        .render(minijinja::context! { content => Value::from_safe_string(html_body.to_string()) })?;
    let text = env.get_template("layout.txt")?.render(minijinja::context! { content => text_body })?;

    let inliner = css_inline::CSSInliner::options()
        .extra_css(Some(lh.css.clone().into()))
        .build();
    let html = inliner.inline(&html).context("css inlining failed")?;
    Ok(Rendered { html, text })
}

/// Steps 1–4 for one recipient.
pub fn render(template: &str, values: &BTreeMap<String, String>, lh: &Letterhead) -> Result<Rendered> {
    let md = fill_template(template, values)?;
    let (html, text) = markdown_to_parts(&md)?;
    frame(&html, &text, lh)
}

/// Addresses of one mail.
#[derive(Default)]
pub struct Addresses {
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    pub bcc: Vec<Mailbox>,
}

/// Split a cell holding several addresses separated by `;`.
pub fn parse_address_cell(cell: &str) -> Result<Vec<Mailbox>> {
    cell.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<Mailbox>().with_context(|| format!("invalid address: {s}")))
        .collect()
}

/// Step 5: MIME message.
pub fn build_message(from: Mailbox, addr: &Addresses, subject: &str, r: &Rendered, lh: &Letterhead) -> Result<Message> {
    anyhow::ensure!(!addr.to.is_empty(), "row has no To address");
    let mut b = Message::builder().from(from).subject(subject);
    for m in &addr.to {
        b = b.to(m.clone());
    }
    for m in &addr.cc {
        b = b.cc(m.clone());
    }
    for m in &addr.bcc {
        b = b.bcc(m.clone());
    }
    let html_part = SinglePart::html(r.html.clone());
    let html = match &lh.logo_png {
        Some(png) => MultiPart::related().singlepart(html_part).singlepart(
            Attachment::new_inline(LOGO_CID.to_string()).body(png.clone(), ContentType::parse("image/png")?),
        ),
        None => MultiPart::related().singlepart(html_part),
    };
    let body = MultiPart::alternative()
        .singlepart(SinglePart::plain(r.text.clone()))
        .multipart(html);
    Ok(b.multipart(body)?)
}
