//! Spike: renders the PC-Freunde-Fulda example for every row and writes
//! `target/spike/<n>.eml` plus the HTML and text parts for inspection.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use massmailer::render::{self, Addresses, Letterhead};

const TO_COLUMNS: &[&str] = &["to"];
const CC_COLUMNS: &[&str] = &["cc"];

#[test]
fn render_example() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/pc-freunde-fulda");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/spike");
    fs::create_dir_all(&out).unwrap();

    let lh = Letterhead::load(&base.join("letterhead")).unwrap();
    let template = fs::read_to_string(base.join("message.md")).unwrap();
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_path(base.join("recipients.csv"))
        .unwrap();
    let headers = rdr.headers().unwrap().clone();

    let mut results = Vec::new();
    for (n, row) in rdr.records().enumerate() {
        let row = row.unwrap();
        let values: BTreeMap<String, String> = headers
            .iter()
            .zip(row.iter())
            .map(|(h, v)| (h.to_string(), v.to_string()))
            .collect();
        let cells = |cols: &[&str]| -> Vec<_> {
            cols.iter()
                .flat_map(|c| render::parse_address_cell(&values[*c]).unwrap())
                .collect()
        };
        let addr = Addresses {
            to: cells(TO_COLUMNS),
            cc: cells(CC_COLUMNS),
            bcc: vec![],
        };

        let r = render::render(&template, &values, &lh).unwrap();
        let from = "PC-Freunde-Fulda <vorstand@example.org>".parse().unwrap();
        let msg =
            render::build_message(from, &addr, "Einladung zum Jahrestreffen", &r, &lh).unwrap();
        fs::write(out.join(format!("{}.eml", n + 1)), msg.formatted()).unwrap();
        fs::write(out.join(format!("{}.html", n + 1)), &r.html).unwrap();
        fs::write(out.join(format!("{}.txt", n + 1)), &r.text).unwrap();
        results.push((r, msg));
    }
    assert_eq!(results.len(), 2);

    // Row 1: name with `_` and `*` stays literal; condition true.
    let (r1, m1) = &results[0];
    assert!(r1.html.contains("Anna_Maria *Stern*"), "html: {}", r1.html);
    assert!(!r1.html.contains("<em>Stern</em>"));
    assert!(
        r1.text.contains("Hallo Anna_Maria *Stern*,"),
        "text: {}",
        r1.text
    );
    assert!(r1.text.contains("Als Kassenwartin bitten"));
    assert!(r1.html.contains("<em>Kassenwartin</em>"));
    assert!(r1.text.starts_with("PC-Freunde-Fulda e.V.\n"));
    assert!(r1.text.contains("Einladung zum Jahrestreffen\n===="));
    assert!(
        r1.html.contains("Viele Grüße<br>\nDer Vorstand"),
        "hard break: {}",
        r1.html
    );
    assert!(r1.text.contains("Viele Grüße\nDer Vorstand"));
    assert!(r1.html.contains("style=\""), "css not inlined");
    assert!(!r1.html.contains("<style"));
    let raw1 = String::from_utf8(m1.formatted()).unwrap();
    assert!(raw1.contains("multipart/alternative"));
    assert!(raw1.contains("multipart/related"));
    assert!(raw1.contains("Content-ID: <logo>"));

    // Row 2: HTML in a value is escaped, condition false, two Cc from one cell.
    let (r2, m2) = &results[1];
    assert!(
        r2.html.contains("&lt;b&gt;Meier&lt;/b&gt;"),
        "html: {}",
        r2.html
    );
    assert!(!r2.html.contains("<b>Meier"));
    assert!(r2.text.contains("<b>Meier</b>"));
    assert!(!r2.text.contains("bitten wir dich"));
    let raw2 = String::from_utf8(m2.formatted()).unwrap();
    assert!(raw2.contains("carla@example.org") && raw2.contains("dora@example.org"));
}
