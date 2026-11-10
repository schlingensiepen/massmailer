//! Render pipeline (three inputs, one common post-processing):
//!
//! ```text
//! md   → placeholders (Markdown-escaped) → comrak ─┐
//! html → placeholders (HTML-escaped) ──────────────┼→ letterhead → HTML post-processing → MIME
//! txt  → placeholders → text as HTML ──────────────┘
//! text part: own text, original text (txt) or derived from the HTML (html2text)
//! ```
//!
//! HTML post-processing: remove scripts and active content, load linked
//! stylesheets and inline them (keeping `<style>` blocks), embed images as
//! CID (SVG rasterized to PNG), Outlook table attributes, preheader.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::rc::Rc;

use anyhow::{Context, Result, anyhow, bail};
use comrak::{Options, markdown_to_html};
use lol_html::html_content::ContentType;
use lol_html::{
    ElementContentHandlers, LocalHandlerTypes, RewriteStrSettings, Selector, element, rewrite_str,
};
use minijinja::{Environment, ErrorKind, UndefinedBehavior, Value};
use std::borrow::Cow;

use crate::config::is_url;
use crate::fetch::{Base, Fetcher};
use crate::recipients::Row;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Md,
    Html,
    Txt,
}

impl Format {
    pub fn detect(source: &str, explicit: Option<&str>) -> Result<Format> {
        let s = explicit.map(str::to_ascii_lowercase).unwrap_or_else(|| {
            let path = source.split(['?', '#']).next().unwrap_or("");
            path.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
        });
        match s.as_str() {
            "md" | "markdown" => Ok(Format::Md),
            "html" | "htm" => Ok(Format::Html),
            "txt" | "text" => Ok(Format::Txt),
            other => bail!(
                "cannot tell the body format from `{other}`; set message.format to md, html or txt"
            ),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Images {
    Embed,
    Link,
}

/// Convert CRLF and lone CR to LF (templates edited on Windows).
pub fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Escape every ASCII punctuation character with a backslash (CommonMark
/// allows this for all of them), so a value cannot introduce Markdown or HTML.
pub fn escape_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(if c == '\n' || c == '\r' { ' ' } else { c });
    }
    out
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Esc {
    Markdown,
    Html,
    None,
}

/// Fill placeholders; missing fields are an error.
fn fill(
    template: &str,
    name: &str,
    values: &Row,
    esc: Esc,
    extra: Option<(&str, Value)>,
) -> Result<String> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_formatter(move |out, _state, value| {
        let raw = value.to_string();
        let s = if value.is_safe() {
            raw
        } else {
            match esc {
                Esc::Markdown => escape_markdown(&raw),
                Esc::Html => escape_html(&raw),
                Esc::None => raw,
            }
        };
        out.write_str(&s)
            .map_err(|_| minijinja::Error::new(ErrorKind::WriteFailure, "write failed"))
    });
    env.add_template(name, template)
        .with_context(|| format!("{name}: template syntax"))?;
    let mut ctx: HashMap<String, Value> = values
        .iter()
        .map(|(k, v)| (k.clone(), Value::from(v.clone())))
        .collect();
    if let Some((k, v)) = extra {
        ctx.insert(k.to_string(), v);
    }
    env.get_template(name)?
        .render(ctx)
        .with_context(|| format!("{name}: placeholder"))
}

fn md_to_html(md: &str) -> String {
    let mut o = Options::default();
    o.extension.autolink = true;
    o.extension.strikethrough = true;
    o.extension.table = true;
    o.render.r#unsafe = false;
    markdown_to_html(md, &o)
}

/// Plain text as HTML: escaped, blank line = paragraph, line break = `<br>`,
/// URLs and mail addresses as links.
pub fn text_to_html(text: &str) -> String {
    let mut out = String::new();
    for para in text.split("\n\n") {
        let para = para.trim_matches('\n');
        if para.trim().is_empty() {
            continue;
        }
        out.push_str("<p>");
        for (i, line) in para.lines().enumerate() {
            if i > 0 {
                out.push_str("<br>\n");
            }
            out.push_str(&linkify(line));
        }
        out.push_str("</p>\n");
    }
    out
}

fn linkify(line: &str) -> String {
    let mut out = String::new();
    for (i, word) in line.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let core = word.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '“', '"']);
        let tail = &word[core.len()..];
        if is_url(core) {
            let _ = write!(
                out,
                "<a href=\"{0}\">{0}</a>{1}",
                escape_html(core),
                escape_html(tail)
            );
        } else if core.contains('@') && core.parse::<lettre::Address>().is_ok() {
            let _ = write!(
                out,
                "<a href=\"mailto:{0}\">{0}</a>{1}",
                escape_html(core),
                escape_html(tail)
            );
        } else {
            out.push_str(&escape_html(word));
        }
    }
    out
}

/// html2text marks headings with `#`; underline them instead.
fn underline_headings(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let hashes = line.chars().take_while(|&c| c == '#').count();
        match line.get(hashes..) {
            Some(rest) if hashes > 0 && rest.starts_with(' ') => {
                let title = rest.trim();
                out.push_str(title);
                out.push('\n');
                let ch = if hashes == 1 { "=" } else { "-" };
                out.push_str(&ch.repeat(title.chars().count()));
            }
            _ => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// `{{ content }}` with any spacing.
fn has_content_slot(s: &str) -> bool {
    let mut rest = s;
    while let Some(i) = rest.find("{{") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("}}") else { break };
        if after[..j].trim() == "content" {
            return true;
        }
        rest = &after[j + 2..];
    }
    false
}

pub struct Letterhead {
    pub html: String,
    pub txt: Option<String>,
    pub base: Base,
}

impl Letterhead {
    pub fn load(dir: &str, fetcher: &mut Fetcher) -> Result<Self> {
        let join = |name: &str| {
            if is_url(dir) {
                format!("{}/{name}", dir.trim_end_matches('/'))
            } else {
                Path::new(dir).join(name).to_string_lossy().into_owned()
            }
        };
        let html_src = join("layout.html");
        let txt_src = join("layout.txt");
        let html = normalize_newlines(
            &fetcher
                .get_text(&html_src)
                .context("the letterhead needs layout.html")?,
        );
        let txt = if is_url(dir) || Path::new(&txt_src).exists() {
            Some(normalize_newlines(&fetcher.get_text(&txt_src)?))
        } else {
            None
        };
        if !has_content_slot(&html) {
            bail!("letterhead layout.html lacks {{{{ content }}}}");
        }
        if txt.as_deref().is_some_and(|t| !has_content_slot(t)) {
            bail!("letterhead layout.txt lacks {{{{ content }}}}");
        }
        Ok(Self {
            html,
            txt,
            base: Base::of(&html_src)?,
        })
    }
}

pub struct InlineImage {
    pub cid: String,
    pub filename: String,
    pub bytes: Vec<u8>,
    pub mime: String,
}

pub struct Rendered {
    pub subject: String,
    pub html: Option<String>,
    pub text: String,
    pub images: Vec<Rc<InlineImage>>,
    pub warnings: Vec<String>,
}

pub struct RendererConfig {
    pub format: Format,
    pub body: String,
    pub body_base: Base,
    pub text_override: Option<String>,
    pub placeholders: bool,
    pub subject: String,
    pub preheader: Option<String>,
    pub letterhead: Option<Letterhead>,
    pub images: Images,
    pub text_only: bool,
    pub text_width: usize,
}

pub struct Renderer {
    cfg: RendererConfig,
    /// source + requested size → image and display size
    image_cache: HashMap<String, (Rc<InlineImage>, u32, u32)>,
    css_cache: HashMap<String, String>,
}

type Handler<'h, 's> = (
    Cow<'s, Selector>,
    ElementContentHandlers<'h, LocalHandlerTypes>,
);

fn settings<'h, 's>(handlers: Vec<Handler<'h, 's>>) -> RewriteStrSettings<'h, 's> {
    handlers
        .into_iter()
        .fold(RewriteStrSettings::new(), |s, h| {
            s.append_element_content_handler(h)
        })
}

/// Rewrite relative `src`/`href` references to absolute ones against `base`.
fn absolutize(html: &str, base: &Base) -> Result<String> {
    let links_too = matches!(base, Base::Url(_));
    Ok(rewrite_str(
        html,
        settings(vec![
            element!("img[src], source[src]", |el| {
                let v = el.get_attribute("src").unwrap_or_default();
                el.set_attribute("src", &base.resolve(&v))?;
                Ok(())
            }),
            element!("link[href]", |el| {
                let v = el.get_attribute("href").unwrap_or_default();
                el.set_attribute("href", &base.resolve(&v))?;
                Ok(())
            }),
            element!("a[href]", |el| {
                if links_too {
                    let v = el.get_attribute("href").unwrap_or_default();
                    el.set_attribute("href", &base.resolve(&v))?;
                }
                Ok(())
            }),
        ]),
    )?)
}

/// Body content and head extras (`<style>`, stylesheet links) of an HTML document.
fn split_document(html: &str) -> (String, String) {
    let doc = scraper::Html::parse_document(html);
    let body_sel = scraper::Selector::parse("body").expect("selector");
    let head_sel =
        scraper::Selector::parse("head style, head link[rel~=stylesheet]").expect("selector");
    let body = doc
        .select(&body_sel)
        .next()
        .map(|b| b.inner_html())
        .unwrap_or_else(|| html.to_string());
    let head: Vec<String> = doc.select(&head_sel).map(|e| e.html()).collect();
    (body, head.join("\n"))
}

fn inject_head(doc: &str, extra: &str) -> String {
    if extra.is_empty() {
        return doc.to_string();
    }
    match doc.to_ascii_lowercase().find("</head>") {
        Some(i) => format!("{}{}\n{}", &doc[..i], extra, &doc[i..]),
        None => format!("{extra}\n{doc}"),
    }
}

/// Remove `@import` rules and `@font-face` blocks (web fonts are not embedded).
fn strip_font_rules(css: &str) -> (String, bool) {
    let mut out = String::new();
    let mut removed = false;
    let mut rest = css;
    loop {
        let lower = rest.to_ascii_lowercase();
        let next = [lower.find("@font-face"), lower.find("@import")]
            .into_iter()
            .flatten()
            .min();
        let Some(i) = next else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..i]);
        removed = true;
        let end = if lower[i..].starts_with("@import") {
            rest[i..].find(';').map(|e| i + e + 1)
        } else {
            rest[i..]
                .find('{')
                .and_then(|o| rest[i + o..].find('}').map(|e| i + o + e + 1))
        };
        rest = &rest[end.unwrap_or(rest.len())..];
    }
    (out, removed)
}

fn is_svg(bytes: &[u8], ct: Option<&str>, src: &str) -> bool {
    let path = src
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    ct.is_some_and(|c| c.contains("svg"))
        || path.ends_with(".svg")
        || String::from_utf8_lossy(&bytes[..bytes.len().min(512)])
            .to_ascii_lowercase()
            .contains("<svg")
}

/// SVG to PNG at twice the display size; returns PNG and display size.
fn rasterize_svg(
    bytes: &[u8],
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(Vec<u8>, u32, u32)> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .context("invalid SVG")?;
    let size = tree.size();
    let (sw, sh) = (size.width(), size.height());
    let (dw, dh) = match (width, height) {
        (Some(w), Some(h)) => (w as f32, h as f32),
        (Some(w), None) => (w as f32, w as f32 * sh / sw),
        (None, Some(h)) => (h as f32 * sw / sh, h as f32),
        (None, None) => (sw, sh),
    };
    let pw = (dw * 2.0).round().max(1.0) as u32;
    let ph = (dh * 2.0).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph).context("SVG too large")?;
    let t = resvg::tiny_skia::Transform::from_scale(pw as f32 / sw, ph as f32 / sh);
    resvg::render(&tree, t, &mut pixmap.as_mut());
    let png = pixmap.encode_png().context("PNG encoding")?;
    Ok((png, dw.round() as u32, dh.round() as u32))
}

fn data_uri(src: &str) -> Option<(Vec<u8>, String)> {
    let rest = src.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.split(';').next().unwrap_or("").to_string();
    if meta.ends_with(";base64") {
        Some((base64_decode(data)?, mime))
    } else {
        Some((data.as_bytes().to_vec(), mime))
    }
}

/// Minimal base64 decoder for `data:` URIs.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn load_image(
    src: &str,
    w: Option<u32>,
    h: Option<u32>,
    n: usize,
    fetcher: &mut Fetcher,
) -> Result<(InlineImage, u32, u32)> {
    let (bytes, ct) = match data_uri(src) {
        Some((b, m)) => (b, Some(m)),
        None => {
            let f = fetcher.get(src, None)?;
            (f.bytes, f.content_type)
        }
    };
    if is_svg(&bytes, ct.as_deref(), src) {
        let (png, dw, dh) = rasterize_svg(&bytes, w, h)?;
        let img = InlineImage {
            cid: format!("img{n}"),
            filename: format!("image{n}.png"),
            bytes: png,
            mime: "image/png".into(),
        };
        return Ok((img, dw, dh));
    }
    let size = imagesize::blob_size(&bytes).context("not an image")?;
    let mime = match imagesize::image_type(&bytes) {
        Ok(imagesize::ImageType::Png) => "image/png",
        Ok(imagesize::ImageType::Jpeg) => "image/jpeg",
        Ok(imagesize::ImageType::Gif) => "image/gif",
        Ok(imagesize::ImageType::Webp) => "image/webp",
        _ => bail!("unsupported image type"),
    };
    let ext = if mime == "image/jpeg" {
        "jpg"
    } else {
        &mime[6..]
    };
    let (iw, ih) = (size.width as u32, size.height as u32);
    let (dw, dh) = match (w, h) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) => (a, a * ih / iw.max(1)),
        (None, Some(b)) => (b * iw / ih.max(1), b),
        (None, None) => (iw, ih),
    };
    let img = InlineImage {
        cid: format!("img{n}"),
        filename: format!("image{n}.{ext}"),
        bytes,
        mime: mime.into(),
    };
    Ok((img, dw, dh))
}

fn px(v: Option<String>) -> Option<u32> {
    v.and_then(|v| v.trim().trim_end_matches("px").parse::<u32>().ok())
}

impl Renderer {
    pub fn new(mut cfg: RendererConfig) -> Self {
        cfg.body = normalize_newlines(&cfg.body);
        cfg.text_override = cfg.text_override.map(|t| normalize_newlines(&t));
        Self {
            cfg,
            image_cache: HashMap::new(),
            css_cache: HashMap::new(),
        }
    }

    pub fn render(&mut self, values: &Row, fetcher: &mut Fetcher) -> Result<Rendered> {
        let c = &self.cfg;
        let mut warnings = Vec::new();
        let subject = fill(&c.subject, "subject", values, Esc::None, None)?;
        if subject.chars().any(char::is_control) {
            bail!("subject contains control characters");
        }
        let body = if c.placeholders {
            let esc = match c.format {
                Format::Md => Esc::Markdown,
                Format::Html => Esc::Html,
                Format::Txt => Esc::None,
            };
            fill(&c.body, "body", values, esc, None)?
        } else {
            c.body.clone()
        };

        // 1. content HTML and head extras of an HTML body
        let (content_html, head_extra) = match c.format {
            Format::Md => (md_to_html(&body), String::new()),
            Format::Txt => (text_to_html(&body), String::new()),
            Format::Html => {
                let abs = absolutize(&body, &c.body_base)?;
                if c.letterhead.is_some() {
                    split_document(&abs)
                } else {
                    (abs, String::new())
                }
            }
        };

        // 2. text part
        let text_content = if let Some(t) = &c.text_override {
            if c.placeholders {
                fill(t, "text", values, Esc::None, None)?
            } else {
                t.clone()
            }
        } else if c.format == Format::Txt {
            body.trim_end().to_string()
        } else {
            html2text::config::plain_no_decorate()
                .string_from_read(content_html.as_bytes(), c.text_width)
                .map(|t| underline_headings(&t))
                .map_err(|e| anyhow!("text part: {e}"))?
                .trim_end()
                .to_string()
        };
        let text = match c.letterhead.as_ref().and_then(|l| l.txt.as_deref()) {
            Some(frame) => fill(
                frame,
                "layout.txt",
                values,
                Esc::None,
                Some(("content", Value::from(text_content))),
            )?,
            None => text_content,
        };

        // 3. HTML document
        let plain_only = c.text_only || (c.format == Format::Txt && c.letterhead.is_none());
        let doc = if plain_only {
            None
        } else {
            Some(match &c.letterhead {
                Some(lh) => {
                    let framed = fill(
                        &lh.html,
                        "layout.html",
                        values,
                        Esc::Html,
                        Some(("content", Value::from_safe_string(content_html))),
                    )?;
                    inject_head(&absolutize(&framed, &lh.base)?, &head_extra)
                }
                None if c.format == Format::Html => content_html,
                None => format!(
                    "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
                     <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\
                     </head>\n<body>\n{content_html}</body>\n</html>\n"
                ),
            })
        };

        let (html, images) = match doc {
            None => (None, Vec::new()),
            Some(doc) => {
                let (h, imgs) = self.postprocess(&doc, values, fetcher, &mut warnings)?;
                (Some(h), imgs)
            }
        };
        if let Some(h) = &html
            && h.len() > 102_000
        {
            warnings.push(format!(
                "HTML part is {} KB; Gmail clips messages above about 102 KB",
                h.len() / 1024
            ));
        }
        Ok(Rendered {
            subject,
            html,
            text,
            images,
            warnings,
        })
    }

    fn postprocess(
        &mut self,
        doc: &str,
        values: &Row,
        fetcher: &mut Fetcher,
        warnings: &mut Vec<String>,
    ) -> Result<(String, Vec<Rc<InlineImage>>)> {
        let embed = self.cfg.images == Images::Embed;
        let removed = RefCell::new(Vec::<String>::new());
        let stylesheets = RefCell::new(Vec::<String>::new());

        // pass 1: remove active content, collect stylesheets
        let pass1 = rewrite_str(
            doc,
            settings(vec![
                element!("script, iframe, object, embed, form, base", |el| {
                    removed.borrow_mut().push(format!("<{}>", el.tag_name()));
                    el.remove();
                    Ok(())
                }),
                element!("*", |el| {
                    let on: Vec<String> = el
                        .attributes()
                        .iter()
                        .map(|a| a.name())
                        .filter(|n| n.starts_with("on"))
                        .collect();
                    for n in on {
                        removed.borrow_mut().push(format!("{n}= attribute"));
                        el.remove_attribute(&n);
                    }
                    Ok(())
                }),
                element!("a[href]", |el| {
                    let href = el.get_attribute("href").unwrap_or_default();
                    if href.trim().to_ascii_lowercase().starts_with("javascript:") {
                        removed.borrow_mut().push("javascript: link".into());
                        el.remove_attribute("href");
                    }
                    Ok(())
                }),
                element!("link[rel~=stylesheet][href]", |el| {
                    if embed {
                        stylesheets
                            .borrow_mut()
                            .push(el.get_attribute("href").unwrap_or_default());
                        el.remove();
                    }
                    Ok(())
                }),
                element!("img", |el| {
                    if embed {
                        el.remove_attribute("srcset");
                        el.remove_attribute("sizes");
                    }
                    Ok(())
                }),
                element!("picture > source", |el| {
                    if embed {
                        el.remove();
                    }
                    Ok(())
                }),
                element!("table", |el| {
                    for (a, v) in [("border", "0"), ("cellpadding", "0"), ("cellspacing", "0")] {
                        if !el.has_attribute(a) {
                            el.set_attribute(a, v)?;
                        }
                    }
                    Ok(())
                }),
            ]),
        )?;
        let mut removed = removed.into_inner();
        removed.sort();
        removed.dedup();
        if !removed.is_empty() {
            warnings.push(format!("removed from HTML: {}", removed.join(", ")));
        }

        // linked stylesheets: load once per run, drop web-font rules
        let mut extra_css = String::new();
        for href in stylesheets.into_inner() {
            if !self.css_cache.contains_key(&href) {
                let raw = fetcher
                    .get_text(&href)
                    .with_context(|| format!("stylesheet {href}"))?;
                let (css, stripped) = strip_font_rules(&raw);
                if stripped {
                    warnings.push(format!("web-font rules removed from {href}"));
                }
                self.css_cache.insert(href.clone(), css);
            }
            extra_css.push_str(&self.css_cache[&href]);
            extra_css.push('\n');
        }

        // pass 2: inline CSS, keep <style> blocks for @media and dark mode
        let inliner = css_inline::CSSInliner::options()
            .keep_style_tags(true)
            .load_remote_stylesheets(false)
            .extra_css((!extra_css.is_empty()).then(|| extra_css.into()))
            .build();
        let inlined = inliner.inline(&pass1).context("CSS inlining failed")?;

        // pass 3: images and preheader
        let preheader = match &self.cfg.preheader {
            Some(p) => Some(escape_html(&fill(p, "preheader", values, Esc::None, None)?)),
            None => None,
        };
        let used: RefCell<Vec<Rc<InlineImage>>> = RefCell::new(Vec::new());
        let missing_alt = RefCell::new(0usize);
        let failure: RefCell<Option<anyhow::Error>> = RefCell::new(None);
        let cache = RefCell::new(&mut self.image_cache);
        let fetcher = RefCell::new(fetcher);
        let html = rewrite_str(
            &inlined,
            settings(vec![
                element!("img", |el| {
                    if !el.has_attribute("alt") {
                        *missing_alt.borrow_mut() += 1;
                    }
                    let src = el.get_attribute("src").unwrap_or_default();
                    if !embed || src.is_empty() || src.starts_with("cid:") {
                        return Ok(());
                    }
                    let (w, h) = (
                        px(el.get_attribute("width")),
                        px(el.get_attribute("height")),
                    );
                    let key = format!("{src}|{w:?}|{h:?}");
                    let mut c = cache.borrow_mut();
                    if !c.contains_key(&key) {
                        let n = c.len() + 1;
                        match load_image(&src, w, h, n, &mut fetcher.borrow_mut()) {
                            Ok((img, dw, dh)) => {
                                c.insert(key.clone(), (Rc::new(img), dw, dh));
                            }
                            Err(e) => {
                                *failure.borrow_mut() = Some(e.context(format!("image {src}")));
                                return Ok(());
                            }
                        }
                    }
                    let (img, dw, dh) = c[&key].clone();
                    el.set_attribute("width", &dw.to_string())?;
                    el.set_attribute("height", &dh.to_string())?;
                    el.set_attribute("src", &format!("cid:{}", img.cid))?;
                    let mut u = used.borrow_mut();
                    if !u.iter().any(|x| x.cid == img.cid) {
                        u.push(img);
                    }
                    Ok(())
                }),
                element!("body", |el| {
                    if let Some(p) = &preheader {
                        el.prepend(
                                &format!(
                                    "<div style=\"display:none;font-size:1px;line-height:1px;\
                                     max-height:0;max-width:0;opacity:0;overflow:hidden;mso-hide:all\">{p}</div>"
                                ),
                                ContentType::Html,
                            );
                    }
                    Ok(())
                }),
            ]),
        )?;
        if let Some(e) = failure.into_inner() {
            return Err(e);
        }
        let n = missing_alt.into_inner();
        if n > 0 {
            warnings.push(format!("{n} image(s) without alt text"));
        }
        Ok((html, used.into_inner()))
    }
}
