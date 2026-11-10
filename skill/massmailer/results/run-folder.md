# Run folder

**When to read this:** auditing a run, answering "what went to whom and
when", finding failed mails, or locating the inputs a run used.

**What you can do afterwards:** navigate a run folder, read
`manifest.json` and `log.jsonl`, and prove which file versions a run
was built from.

## Location and name

`<run.state_dir>/<YYYYMMDD-HHMMSS>-<mode>/`, where `run.state_dir`
defaults to `massmailer-runs` (relative to the config file that sets
it, or to the working directory if unset) and `<mode>` is `check`,
`render`, `dry-run` or `send`. A second run in the same second gets
`-2`, `-3`, … appended. The folder name is also the value of the
`massmailer-run` message tag used by `bounces`.

Every mode creates a folder, so a long project accumulates them;
`check`/`render`/`dry-run` folders can be deleted, `send` folders are
the record and should be kept with the mailing.

## Layout

```
20261110-155012-send/
  inputs/
    001-message.md          every loaded input, numbered in load order,
    002-layout.html         file name sanitised to [A-Za-z0-9._-]
    003-layout.txt
    004-recipients.csv
    005-style.css
    006-logo.png
  manifest.json             list of inputs with source, size, SHA-256
  mails/
    0001.eml                complete MIME message per mail
    0002.eml
    0001.html, 0001.txt     render mode only
  log.jsonl                 dry-run and send only; one line per mail
  events.jsonl              written by `bounces`: raw events for this run
  bounces.csv               written by `bounces`: one row per affected recipient
```

The recipient list and all attachments are inside `inputs/`, so a
`send` folder contains personal data. Treat it like the list itself.

## manifest.json

A JSON array, one object per input, in load order:

```json
{
  "source": "/path/to/message.md",      // path or URL as resolved
  "file": "inputs/001-message.md",      // copy inside the run folder
  "size": 364,
  "sha256": "990f16…",
  "content_type": null,                 // from the HTTP response for URLs
  "loaded_at": "2026-11-10T15:50:12+00:00"
}
```

Use it to prove which version of the body, letterhead and list a run
used: compare `sha256` with `sha256sum` of the user's current file.

## log.jsonl

One JSON object per line, in sending order:

| Field | Meaning |
|---|---|
| `time` | local time with offset, RFC 3339, when the attempt finished |
| `mail` | `mails/0001.eml`, the file for this entry |
| `line` | row of the recipient list (header = line 1) |
| `to` | the `To` addresses as sent, comma-separated; on a redirect the test address |
| `message_id` | the `Message-ID` massmailer generated (`<…@sender-domain>`); Amazon SES replaces it in transit, see `../providers/amazon-ses.md` |
| `bytes` | size of the encoded message |
| `attachments` | `[{"file": "…", "sha256": "…"}]`, common and per-row attachments |
| `status` | `dry-run`, `sent` or `failed` |
| `detail` | `dry run, not sent`; the server's acceptance message; or the error text |

Useful queries (any JSON-lines tool; `jq` shown):

```sh
jq -r 'select(.status=="failed") | "\(.line)\t\(.to)\t\(.detail)"' log.jsonl
jq -r '.status' log.jsonl | sort | uniq -c
jq -r 'select(.to | test("example.net")) | .mail' log.jsonl
```

The log contains addresses; quote it to the user by line number and
status, not as a dump, unless asked.

## mails/NNNN.eml

The exact message as handed to SMTP: headers (including
`X-SES-MESSAGE-TAGS: massmailer-run=<folder>, massmailer-mail=NNNN`
from the release after 0.1.0), text part, HTML part, inline images and
attachments. Numbering is the position among non-skipped rows after
the limit, which is also the `mail` field in `log.jsonl` and the
`massmailer-mail` tag.
