# Configuration

**When to read this:** before writing or editing a config file, when a
command says `missing setting` or `unknown setting`, or when you need to
know where a value comes from. This is the only complete list of
settings; the other files link here.

**What you can do afterwards:** write a correct `account.json` and
`mailing.json`, override any setting from the environment or the
command line, keep secrets out of files, and explain why a value won.

## Sources and precedence

1. **Config files**: `-c FILE` (JSON), may be given several times.
   Later files override earlier ones key by key. At least one `-c` is
   required (`error: no --config given`), even when everything else
   comes from `--set`.
2. **Environment**: every key as `MASSMAILER_<KEY>` with dots replaced
   by underscores, upper case: `smtp.password` →
   `MASSMAILER_SMTP_PASSWORD`, `recipients.to_columns` →
   `MASSMAILER_RECIPIENTS_TO_COLUMNS`. Environment wins over files.
3. **Command line**: `--set key=value` (repeatable), plus
   `--smtp-username` and `--smtp-password`; `send` also has `--limit`
   (= `sending.limit`) and `--redirect-to` (= `sending.redirect_to`).
   The command line wins over everything.

`-v` / `--verbose` prints every effective setting with its origin
(`config <file>`, `env <NAME>` or `command line`) on stderr; secret
values appear as `***`.

## File format

A JSON object; nested objects are flattened to dotted keys, so these
are the same:

```json
{ "smtp": { "host": "smtp.example.net", "port": 587 } }
{ "smtp.host": "smtp.example.net", "smtp.port": 587 }
```

Unknown keys are an error (`unknown setting`), which catches typos.
Lists are JSON arrays (`"to_columns": ["email", "email2"]`); a single
string is accepted where a list is expected.

Environment and `--set` values are parsed as JSON when they look like
JSON (`5`, `true`, `["a","b"]`), otherwise taken as a string. So
`--set recipients.to_columns='["email","email2"]'` sets a list and
`--set sending.limit=5` a number.

**Unsetting:** an empty string counts as "not set". `--set
letterhead.dir=` or `--set message.preheader=` removes a value that a
config file provides.

Booleans accept `true`/`yes`/`ja`/`1` and `false`/`no`/`nein`/`0`.

## Relative paths

Path settings in a config file are resolved relative to the directory
of **that file**, not to the working directory. URLs and absolute paths
are left alone. This applies to `message.body`, `message.text`,
`message.attachments`, `message.attachment_dir`, `letterhead.dir`,
`recipients.source` and `run.state_dir`. Paths given via environment or
`--set` are taken literally (relative to the working directory).

## Secrets

Secret keys: `smtp.username`, `smtp.password`,
`recipients.url.bearer_token`, and for `bounces` `aws.access_key_id`,
`aws.secret_access_key`. Supply them as environment variables or flags
(`--smtp-username`, `--smtp-password`, `--aws-access-key-id`,
`--aws-secret-access-key`), or as a separate config file outside any
repository with permissions only the user can read, passed as an extra
`-c`. Never put them in a file next to the mailing or in a repository.

## Recommended split

`account.json` (provider, reusable): `smtp.*`, `sending.rate_per_second`,
`ses.configuration_set`, `aws.*`, `run.state_dir`.
`mailing.json` (one per mailing): `sender.*`, `letterhead.dir`,
`message.*`, `recipients.*`. The example
`examples/pc-freunde-fulda/` in the repository shows both.

## All settings

Defaults apply when the key is absent or empty.

### SMTP

| Key | Meaning | Default |
|---|---|---|
| `smtp.host` | SMTP server host name; required for a real send | – |
| `smtp.port` | port | 587 for `starttls`, 465 for `tls` |
| `smtp.security` | `starttls` or `tls` (implicit TLS) | `starttls` |
| `smtp.username` | login (secret); required for a real send | – |
| `smtp.password` | password (secret); required for a real send | – |
| `smtp.timeout_seconds` | connection and command timeout | 30 |

SMTP settings are only read for a real send. `check`, `render` and
`--dry-run` neither need nor validate them.

### Sending

| Key | Meaning | Default |
|---|---|---|
| `sending.rate_per_second` | mails per second; the gap between two sends is 1/rate, measured from the start of the previous send; minimum 0.01 | 1 |
| `sending.limit` | build and send only the first N rows (after skipping); `--limit N` | all |
| `sending.redirect_to` | deliver every mail to this one address, with the test subject prefix; `--redirect-to ADDR` | – |
| `ses.configuration_set` | value of the `X-SES-CONFIGURATION-SET` header (Amazon SES) | – |

### Sender

| Key | Meaning | Default |
|---|---|---|
| `sender.address` | `From` address; required. The `Message-ID` is generated in this domain | – |
| `sender.name` | display name for `From` | – |
| `sender.reply_to` | `Reply-To` | – |
| `sender.on_behalf_of` | if set, `From` becomes this address and `Sender` is `sender.address` | – |

### Letterhead and message

| Key | Meaning | Default |
|---|---|---|
| `letterhead.dir` | folder or URL with `layout.html` (required) and optional `layout.txt`, both containing `{{ content }}` | none: HTML bodies are sent as they are, text bodies as plain mail |
| `message.body` | body file or URL: `.md`, `.html`/`.htm` or `.txt`; required | – |
| `message.format` | `md`, `html` or `txt` when the extension does not say | from extension |
| `message.subject` | subject with placeholders; required | – |
| `message.preheader` | hidden preview text (placeholders allowed) | – |
| `message.text` | own plain-text file instead of the derived text part | derived from HTML |
| `message.text_width` | line width of the derived text part | 78 |
| `message.placeholders` | `false` sends the body and `message.text` literally (the subject is still filled) | `true` |
| `message.images` | `embed` images as inline parts, or `link` to leave `src` as it is | `embed` |
| `message.parts` | `both` (text + HTML) or `text` only | `both` |
| `message.to`, `message.cc`, `message.bcc` | fixed addresses added to every mail (string or list) | – |
| `message.attachments` | file(s) or URL(s) attached to every mail | – |
| `message.attachment_column` | column whose cell names one extra attachment per row; empty cell = none | – |
| `message.attachment_dir` | folder or URL that relative cell values of the attachment column are resolved against | working directory |
| `message.max_mail_size_mb` | a built mail above this size aborts the run; at most 40 | 5 |

### Recipients

| Key | Meaning | Default |
|---|---|---|
| `recipients.source` | list file or URL: `.csv`, `.xlsx`/`.xls`/`.xlsm`, `.ods`, `.json`; required | – |
| `recipients.format` | `csv`, `xlsx`, `ods` or `json` when the extension does not say | from extension / content type |
| `recipients.to_columns` | column(s) with `To` addresses; required unless `message.to` is set | – |
| `recipients.cc_columns`, `recipients.bcc_columns` | column(s) with `Cc`/`Bcc` addresses | – |
| `recipients.skip_column` | rows whose cell here is `nein`, `no`, `false` or `0` are skipped | – |
| `recipients.sheet` | sheet name for spreadsheets | first sheet |
| `recipients.csv.delimiter` | one character | detected from the header line (`;` or `,`) |
| `recipients.url.bearer_token` | `Authorization: Bearer` for a list URL (secret) | – |

### Run folder and downloads

| Key | Meaning | Default |
|---|---|---|
| `run.state_dir` | where run folders are created; `bounces` also searches here | `massmailer-runs` |
| `fetch.timeout_seconds` | timeout for URL inputs | 30 |
| `fetch.max_size_mb` | maximum size of one input file or download | 20 |

### Bounces (Amazon SES via SNS and SQS)

| Key | Meaning | Default |
|---|---|---|
| `aws.region` | region of the SQS queue, e.g. `eu-north-1` | – |
| `aws.sqs_queue_url` | URL of the queue fed by the configuration set's SNS topic | – |
| `aws.access_key_id` | IAM key (secret); `MASSMAILER_AWS_ACCESS_KEY_ID` or `--aws-access-key-id` | – |
| `aws.secret_access_key` | IAM secret (secret); `MASSMAILER_AWS_SECRET_ACCESS_KEY` or `--aws-secret-access-key` | – |

## Minimal working pair

`account.json`:

```json
{
  "smtp": { "host": "smtp.example.net", "port": 587, "security": "starttls" },
  "sending": { "rate_per_second": 1 },
  "run": { "state_dir": "runs" }
}
```

`mailing.json` (next to `message.md`, `letterhead/`, `members.csv`):

```json
{
  "sender": { "address": "board@example.org", "name": "PC-Freunde-Fulda" },
  "letterhead": { "dir": "letterhead" },
  "message": { "body": "message.md", "subject": "Invitation, {{ first_name }}" },
  "recipients": { "source": "members.csv", "to_columns": ["email"] }
}
```

Credentials: `MASSMAILER_SMTP_USERNAME` and `MASSMAILER_SMTP_PASSWORD`
in the environment of the shell that runs the send.
