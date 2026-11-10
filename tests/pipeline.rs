//! End-to-end tests on the PC-Freunde-Fulda example (no network needed).

use std::fs;
use std::path::{Path, PathBuf};

use massmailer::config::Settings;
use massmailer::render::{escape_markdown, text_to_html};
use massmailer::run::{self, Mode, Options};

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/pc-freunde-fulda")
}

fn state(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-runs")
        .join(name);
    let _ = fs::remove_dir_all(&p);
    p
}

/// Run with extra settings; returns the run folder.
fn run_with(name: &str, mode: Mode, sets: &[(&str, &str)]) -> PathBuf {
    let st = state(name);
    let mut overrides: Vec<(String, String)> = sets
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    overrides.push(("run.state_dir".into(), st.to_string_lossy().into_owned()));
    let s = Settings::load(&[example().join("mailing.example.json")], &overrides).unwrap();
    run::execute(
        &s,
        &Options {
            mode,
            yes: true,
            verbose: false,
        },
    )
    .unwrap();
    fs::read_dir(&st).unwrap().next().unwrap().unwrap().path()
}

fn read(dir: &Path, f: &str) -> String {
    fs::read_to_string(dir.join("mails").join(f)).unwrap()
}

#[test]
fn markdown_in_letterhead() {
    let dir = run_with("md", Mode::Render, &[]);
    let html = read(&dir, "0001.html");
    let text = read(&dir, "0001.txt");
    let eml = read(&dir, "0001.eml");
    // values stay literal, condition works
    assert!(html.contains("Anna_Maria *Stern*"), "{html}");
    assert!(html.contains("<em>Kassenwartin</em>"));
    assert!(text.contains("Anna_Maria *Stern*"), "{text}");
    // linked stylesheet is inlined and the link removed
    assert!(!html.contains("<link"), "{html}");
    assert!(html.contains("padding: 16px 24px"), "{html}");
    // logo embedded, preheader present
    assert!(html.contains("src=\"cid:img1\""), "{html}");
    assert!(html.contains("Jahrestreffen am 14. März 2027"));
    // text frame and MIME structure
    assert!(text.starts_with("PC-Freunde-Fulda e.V.\n"), "{text}");
    assert!(eml.contains("multipart/alternative") && eml.contains("multipart/related"));
    assert!(eml.contains("Content-ID: <img1>"));
    // Message-ID in the sender's domain, not the local host name
    let mid = eml
        .lines()
        .find(|l| l.starts_with("Message-ID: "))
        .expect("Message-ID header");
    assert!(mid.ends_with("@example.org>"), "{mid}");
    assert!(
        eml.contains("Einladung zum Jahrestreffen, Anna_Maria"),
        "{eml}"
    );
    // row 2: HTML in a value is escaped, two Cc from one cell
    let html2 = read(&dir, "0002.html");
    assert!(html2.contains("&lt;b&gt;Meier&lt;/b&gt;"), "{html2}");
    let eml2 = read(&dir, "0002.eml");
    assert!(eml2.contains("carla@example.org") && eml2.contains("dora@example.org"));
    assert!(dir.join("manifest.json").exists());
}

#[test]
fn text_body_without_letterhead_is_plain() {
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-runs/plain.txt");
    fs::create_dir_all(tmp.parent().unwrap()).unwrap();
    fs::write(
        &tmp,
        "Hallo {{ vorname }},\r\n\r\nsiehe https://example.org.\r\n",
    )
    .unwrap();
    let dir = run_with(
        "txt",
        Mode::Render,
        &[
            ("message.body", tmp.to_str().unwrap()),
            ("letterhead.dir", ""),
            ("message.preheader", ""),
        ],
    );
    let eml = read(&dir, "0001.eml");
    assert!(!eml.contains("text/html"), "{eml}");
    assert!(read(&dir, "0001.txt").contains("Hallo Anna_Maria,\n\nsiehe https://example.org."));
}

#[test]
fn redirect_rewrites_recipients() {
    let dir = run_with(
        "redirect",
        Mode::DryRun,
        &[
            ("sending.redirect_to", "test@example.net"),
            ("sending.limit", "1"),
        ],
    );
    let eml = read(&dir, "0001.eml");
    assert!(eml.contains("To: test@example.net"), "{eml}");
    assert!(!eml.contains("\r\nCc:"));
    assert!(eml.contains("TEST an anna@example.org"), "{eml}");
    assert!(!dir.join("mails/0002.eml").exists());
    assert!(
        fs::read_to_string(dir.join("log.jsonl"))
            .unwrap()
            .contains("\"dry-run\"")
    );
}

#[test]
fn helpers() {
    assert_eq!(escape_markdown("a*b_c"), "a\\*b\\_c");
    let h = text_to_html("Zeile 1\nZeile 2\n\nMail an vorstand@example.org.");
    assert!(h.contains("Zeile 1<br>\nZeile 2"));
    assert!(h.contains("<a href=\"mailto:vorstand@example.org\">vorstand@example.org</a>."));
}
