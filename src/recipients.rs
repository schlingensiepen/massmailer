//! Reading the recipient list (CSV, XLSX/ODS, JSON) into rows of named fields.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use calamine::{Data, Reader, open_workbook_auto};
use lettre::message::Mailbox;

use crate::fetch::Fetched;

pub type Row = BTreeMap<String, String>;

pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

fn cell_to_string(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        other => other.to_string(),
    }
}

fn from_grid(grid: Vec<Vec<String>>) -> Result<Table> {
    let mut it = grid.into_iter();
    let header = it.next().context("recipient list is empty")?;
    let columns: Vec<String> = header.iter().map(|h| h.trim().to_string()).collect();
    let mut rows = Vec::new();
    for (i, cells) in it.enumerate() {
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if cells.len() > columns.len()
            && cells[columns.len()..].iter().any(|c| !c.trim().is_empty())
        {
            bail!("row {}: more cells than columns", i + 2);
        }
        let mut row = Row::new();
        for (j, col) in columns.iter().enumerate() {
            if col.is_empty() {
                continue;
            }
            row.insert(col.clone(), cells.get(j).cloned().unwrap_or_default());
        }
        rows.push(row);
    }
    Ok(Table { columns, rows })
}

pub fn read(
    source: &str,
    f: &Fetched,
    format: Option<&str>,
    delimiter: Option<&str>,
    sheet: Option<&str>,
) -> Result<Table> {
    let lower = source.to_ascii_lowercase();
    let fmt = format.map(str::to_ascii_lowercase).unwrap_or_else(|| {
        if lower.ends_with(".xlsx") || lower.ends_with(".xls") || lower.ends_with(".xlsm") {
            "xlsx".into()
        } else if lower.ends_with(".ods") {
            "ods".into()
        } else if lower.ends_with(".json")
            || f.content_type
                .as_deref()
                .is_some_and(|c| c.contains("json"))
        {
            "json".into()
        } else {
            "csv".into()
        }
    });
    match fmt.as_str() {
        "xlsx" | "xls" | "ods" => {
            let mut wb = open_workbook_auto(&f.local)
                .with_context(|| format!("cannot open spreadsheet {source}"))?;
            let name = match sheet {
                Some(s) => s.to_string(),
                None => wb
                    .sheet_names()
                    .first()
                    .cloned()
                    .context("workbook has no sheet")?,
            };
            let range = wb
                .worksheet_range(&name)
                .with_context(|| format!("sheet `{name}` not found"))?;
            let grid = range
                .rows()
                .map(|r| r.iter().map(cell_to_string).collect())
                .collect();
            from_grid(grid)
        }
        "json" => {
            let v: Vec<serde_json::Map<String, serde_json::Value>> =
                serde_json::from_slice(&f.bytes).context("JSON must be a flat array of objects")?;
            let mut columns: Vec<String> = Vec::new();
            for o in &v {
                for k in o.keys() {
                    if !columns.contains(k) {
                        columns.push(k.clone());
                    }
                }
            }
            let rows = v
                .into_iter()
                .map(|o| {
                    o.into_iter()
                        .map(|(k, v)| {
                            (
                                k,
                                match v {
                                    serde_json::Value::String(s) => s,
                                    serde_json::Value::Null => String::new(),
                                    other => other.to_string(),
                                },
                            )
                        })
                        .collect()
                })
                .collect();
            Ok(Table { columns, rows })
        }
        "csv" => {
            let text = String::from_utf8(f.bytes.clone()).or_else(|e| {
                // Windows-1252 fallback for Excel exports
                Ok::<_, anyhow::Error>(e.into_bytes().iter().map(|&b| b as char).collect())
            })?;
            let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_string();
            let delim = match delimiter {
                Some(d) => *d.as_bytes().first().context("empty delimiter")?,
                None => {
                    let first = text.lines().next().unwrap_or("");
                    if first.matches(';').count() > first.matches(',').count() {
                        b';'
                    } else {
                        b','
                    }
                }
            };
            let mut rdr = csv::ReaderBuilder::new()
                .delimiter(delim)
                .has_headers(false)
                .flexible(true)
                .from_reader(text.as_bytes());
            let mut grid = Vec::new();
            for (i, rec) in rdr.records().enumerate() {
                let rec = rec.with_context(|| format!("CSV line {}", i + 1))?;
                grid.push(rec.iter().map(str::to_string).collect());
            }
            from_grid(grid)
        }
        other => bail!("unknown recipients.format `{other}`"),
    }
}

/// Split a cell holding several addresses separated by `;`.
pub fn parse_addresses(cell: &str) -> Result<Vec<Mailbox>> {
    cell.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<Mailbox>()
                .with_context(|| format!("invalid address `{s}`"))
        })
        .collect()
}

pub fn is_skip_value(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "nein" | "no" | "false" | "0"
    )
}

/// Trim values and remove control characters (they must never reach a header or the markup).
pub fn sanitize(row: &Row) -> Row {
    row.iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.trim().chars().filter(|c| !c.is_control()).collect(),
            )
        })
        .collect()
}
