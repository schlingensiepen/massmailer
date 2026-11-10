# Recipient list

**When to read this:** before pointing `recipients.source` at a file,
when `columns` shows unexpected names, or when `check` stops at a row.

**What you can do afterwards:** choose the right format and settings
for a list, know how cells become addresses and placeholder values,
find a bad row from the error message and tell the user what to fix
without dumping the list.

## Formats

The format comes from the extension of `recipients.source` (or from the
content type of a URL); `recipients.format` overrides it.

| Format | Detection | Notes |
|---|---|---|
| CSV | `.csv` or anything else | UTF-8 (BOM removed); non-UTF-8 files are read as Windows-1252/Latin-1. Delimiter detected from the header line: `;` if it has more semicolons than commas, else `,`; `recipients.csv.delimiter` forces one character. Quoted cells may contain the delimiter and line breaks. |
| XLSX / XLS / XLSM | `.xlsx`, `.xls`, `.xlsm` | first sheet unless `recipients.sheet`; whole numbers appear without decimals (`42`, not `42.0`); dates come as the cell's text/number as the spreadsheet library reports them, so prefer a text column for dates |
| ODS | `.ods` | same as XLSX |
| JSON | `.json` or content type containing `json` | a flat array of objects; columns are the union of all keys in first-seen order; `null` becomes empty; numbers/booleans become their JSON text |

Spreadsheet and CSV rules: the first row is the header, names are
trimmed; completely empty rows are dropped; a column with an empty
header is ignored; a row with more non-empty cells than headers is an
error (`row N: more cells than columns`).

Lists may be URLs (`https://…`); `recipients.url.bearer_token` adds an
`Authorization: Bearer` header. Every input is copied into the run
folder (`inputs/`) with its SHA-256.

## Values

- Column names are the placeholder names: `{{ first_name }}` needs a
  header `first_name`, exact and case-sensitive. Choose headers without
  spaces or special characters: `{{ Vorname }}` works, `{{ E-Mail }}`
  is not a valid placeholder; rename the column (e.g. `email`).
- Every value is trimmed and control characters are removed before use.
- A missing field in the template is an error, not an empty string.

## Address columns

- `recipients.to_columns` (required unless `message.to` is set),
  `recipients.cc_columns`, `recipients.bcc_columns`: lists of column
  names. Every mail needs at least one `To`; a row without one is an
  error (`line N: no To address`).
- A cell may hold several addresses separated by `;`, e.g.
  `carla@example.org; dora@example.org`. In a `;`-delimited CSV such a
  cell must be quoted. Whitespace around addresses is ignored, empty
  entries are dropped.
- `Name <address@example.org>` is accepted; a cell that does not parse
  is an error (`invalid address`).
- `message.to`/`cc`/`bcc` add fixed addresses to every mail on top of
  the columns.

## Skipping rows

`recipients.skip_column` names a column; rows with `nein`, `no`,
`false` or `0` (case-insensitive) in it are skipped and counted
(`… 2 row(s), 1 skipped`). Any other value, including empty, keeps the
row. If the named column does not exist, nothing is skipped and no
error is raised, so verify the name with `columns` first.

`--limit N` applies after skipping: the first N remaining rows.

## Checking a list and reporting bad rows

1. `massmailer columns -c mailing.json` prints `N rows; columns:` and
   the names. Compare with the settings and the placeholders in the
   body and subject. No data is printed.
2. `massmailer check -c mailing.json` builds every mail. It stops at
   the **first** failing row with `recipient list line N: …`. `line`
   counts the header as line 1, so line 2 is the first data row; blank
   rows are dropped before counting, so in a file with blank lines the
   number can be lower than the editor's line number.
3. Report the line number and the reason to the user (e.g. "line 17:
   invalid address" or "line 5: placeholder `first_name` is empty or
   missing"). Quote the offending cell only if the user asks; never the
   whole row set.
4. After the fix, run `check` again; repeat until it ends with
   `N mail(s) prepared from M row(s)`.

Typical causes: a header with trailing spaces or different case, an
address cell with a comma instead of `;`, a stray `<` in a name (it is
escaped, not an error, but looks wrong), a column renamed in the
spreadsheet but not in the config.
