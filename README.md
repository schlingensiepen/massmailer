# massmailer

**Install** (no admin rights required):

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/install.ps1 | iex
```

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/install.sh | sh
```

Both verify the SHA-256 checksum of the download; running them again
updates an existing installation. Uninstall with `deploy/uninstall.ps1`
or `deploy/uninstall.sh` the same way.

---

A command-line tool for sending personalised bulk email. The body is a
Markdown, HTML or plain-text file with placeholders; the recipients come
from a CSV, XLSX, ODS or JSON list. Every message is put into a letterhead
(HTML and plain-text frame), prepared for email clients (CSS inlined,
images embedded, SVG converted to PNG, plain-text part derived), sent via
SMTP at a limited rate (e.g. Amazon SES) and recorded in a run folder.

It is meant for formal notices and press releases to a known list of
recipients, not for newsletters: there is no unsubscribe mechanism.

**Status:** early version (0.1). `check`, `columns`, `render` and `send`
(with `--dry-run`, `--limit`, `--redirect-to`) work; bounce handling is
planned.

## Quick start

```sh
massmailer columns -c mailing.json                 # column names of the list
massmailer render  -c mailing.json                 # .eml/.html/.txt per mail
massmailer send --dry-run -c account.json -c mailing.json
massmailer send --limit 5 --redirect-to me@example.org -c account.json -c mailing.json
massmailer send -c account.json -c mailing.json    # asks before sending
```

Every run creates a folder (`run.state_dir`, default `massmailer-runs/`)
with a copy of all inputs and their SHA-256 (`inputs/`, `manifest.json`),
every mail as `.eml` (`mails/`) and a log (`log.jsonl`). Nothing is sent if
a single mail cannot be built.

See [`examples/pc-freunde-fulda/`](examples/pc-freunde-fulda/) for a
complete example (fictional club).

## Settings

Settings come from one or more JSON files (`-c`, later files win),
environment variables and the command line (`--set key=value`); the
command line wins over the environment, the environment over the files.
Relative paths are resolved against the file that contains them. Every key
can be set as an environment variable: `smtp.password` →
`MASSMAILER_SMTP_PASSWORD`. Keep secrets out of config files.

| Key | Meaning |
|---|---|
| `smtp.host`, `smtp.port`, `smtp.security` | e.g. `email-smtp.eu-central-1.amazonaws.com`, `587`, `starttls` (or `tls`) |
| `smtp.username`, `smtp.password` | credentials (environment or `--smtp-username`/`--smtp-password`) |
| `sending.rate_per_second` | send rate (default 1) |
| `sending.limit`, `sending.redirect_to` | only the first N mails; deliver all to one test address |
| `ses.configuration_set` | sets `X-SES-CONFIGURATION-SET` |
| `sender.address`, `sender.name` | `From` |
| `sender.reply_to` | `Reply-To` |
| `sender.on_behalf_of` | `From` becomes this address, `Sender` is `sender.address` |
| `letterhead.dir` | folder with `layout.html` and optional `layout.txt`, both with `{{ content }}`; without it, HTML is sent as it is and text as plain mail |
| `message.body` | `.md`, `.html` or `.txt` file or URL |
| `message.subject` | subject, with placeholders |
| `message.preheader` | hidden preview text |
| `message.text` | own plain-text version instead of the derived one |
| `message.placeholders` | `false` takes the body literally |
| `message.images` | `embed` (default) or `link` |
| `message.parts` | `both` (default) or `text` |
| `message.to`, `message.cc`, `message.bcc` | fixed addresses for every mail |
| `message.attachments`, `message.attachment_column`, `message.attachment_dir` | attachments for all and per row |
| `message.max_mail_size_mb` | size limit per mail (default 5, at most 40) |
| `recipients.source` | `.csv`, `.xlsx`, `.ods` or `.json` file or URL |
| `recipients.to_columns`, `cc_columns`, `bcc_columns` | address columns; a cell may hold several addresses separated by `;` |
| `recipients.skip_column` | rows with `nein`, `no`, `false` or `0` there are skipped |
| `recipients.sheet`, `recipients.csv.delimiter` | sheet name; CSV delimiter (default: detected) |
| `run.state_dir` | where run folders are created |

Placeholders use [MiniJinja](https://docs.rs/minijinja) syntax
(`{{ Vorname }}`, `{% if … %}`); field names are the column headers. Values
are escaped for the body format. The letterhead and an HTML body include
their CSS with `<link rel="stylesheet">` or `<style>`; it is inlined.

## Build from source

```sh
cargo build --release
```

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
