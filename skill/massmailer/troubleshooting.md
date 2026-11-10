# Troubleshooting

**When to read this:** a command printed `error: …` or `warning: …`.

**What you can do afterwards:** name the cause and the fix for every
message massmailer emits. Errors go to stderr as `error: <message>`,
often with a context chain joined by `: `, e.g. `error: recipient list
line 7: invalid address `anna@`. Exit code 1. Warnings are
`warning: …`, printed once per run per distinct text, and do not stop
the run.

## Command line and config

| Message | Cause | Fix |
|---|---|---|
| `no --config given` | no `-c` | pass at least one config file, even if everything else comes from `--set` |
| `cannot read config X` | path wrong or unreadable | check the path; relative to the working directory |
| `invalid JSON in X` | syntax error, trailing comma, comment | validate the JSON |
| `X: top level must be an object` | file is an array or scalar | wrap in `{ … }` |
| `X: unknown setting `key`` / `unknown setting `key`` | typo, or key from a newer version | compare with `setup/configuration.md`; the second form comes from `--set` or env |
| `--set expects KEY=VALUE, got …` | missing `=` | `--set sending.limit=5` |
| `missing setting `key` (config, MASSMAILER_… or command line)` | required key absent or empty | set it in a file, env or `--set` |
| ``key` must be a number` / `must be true or false` | wrong type | numbers unquoted or numeric strings; booleans `true/false/yes/no/ja/nein/1/0` |
| `cannot create <dir>` | `run.state_dir` not writable | fix permissions or set another `run.state_dir` |

## Inputs and downloads

| Message | Cause | Fix |
|---|---|---|
| `cannot read X` | body, letterhead file, list, attachment or image not found | relative paths in config files resolve against that file's folder; check with `-v` |
| `X: not UTF-8 text` | body, layout, stylesheet or `message.text` in another encoding | save as UTF-8 |
| `X: larger than fetch.max_size_mb` | input above 20 MB (default) | shrink the file or raise `fetch.max_size_mb` |
| `download failed: URL` | network, 4xx/5xx, timeout | check URL, token (`recipients.url.bearer_token`), `fetch.timeout_seconds` |
| `invalid URL X` | malformed URL in `message.body` or letterhead | fix the URL |

## Recipient list

| Message | Cause | Fix |
|---|---|---|
| `recipient list is empty` | no header row | add a header row |
| `row N: more cells than columns` | a data row has more non-empty cells than the header | fix the row or the header; often an unquoted delimiter inside a cell |
| `cannot open spreadsheet X` | corrupt or not actually XLSX/ODS | re-export; or set `recipients.format` |
| `workbook has no sheet` / `sheet `X` not found` | wrong `recipients.sheet` | `columns` shows nothing about sheets; check the name in the spreadsheet |
| `JSON must be a flat array of objects` | nested JSON or an object at the top | export as `[{"col": "value", …}, …]` |
| `empty delimiter` | `recipients.csv.delimiter` is `""` | set one character or remove the key |
| `CSV line N: …` | quoting error in the CSV | fix the quotes on that line |
| `unknown recipients.format `X`` | not `csv`, `xlsx`, `xls`, `ods`, `json` | fix the value |
| `set recipients.to_columns or message.to` | no `To` source at all | set one of them |
| `recipient list line N: to column `X` not in recipient list` (also `cc`/`bcc`) | header name differs | `columns`; exact, case-sensitive, trimmed |
| `recipient list line N: no To address` | all `To` cells of that row empty | fill or skip the row (`recipients.skip_column`) |
| `recipient list line N: invalid address `X`` | malformed address, comma-separated list, trailing characters | use `;` between addresses; fix the cell |
| `attachment column `X` not in recipient list` | `message.attachment_column` misspelled | `columns` |

## Rendering

| Message | Cause | Fix |
|---|---|---|
| `cannot tell the body format from `X`; set message.format to md, html or txt` | unknown extension or URL without one | set `message.format` |
| `body: template syntax: …` (also `subject`, `preheader`, `text`, `layout.html`, `layout.txt`) | MiniJinja syntax error in that template | the message names line and column; typical: unclosed `{% if %}`, `{{` in CSS of the letterhead (write `{{ '{{' }}` or move the CSS) |
| `body: placeholder: … undefined …` | `{{ field }}` with no such column | `columns`; rename the placeholder or the column |
| `subject contains control characters` | line break or tab in `message.subject` or in a value used in it | fix the subject or the cell |
| `the letterhead needs layout.html` | `letterhead.dir` has no `layout.html` | add it, or unset the letterhead with `--set letterhead.dir=` |
| `letterhead layout.html lacks {{ content }}` / `layout.txt lacks {{ content }}` | no insertion point | add `{{ content }}` |
| `message.images must be embed or link` / `message.parts must be both or text` / `smtp.security must be starttls or tls` | wrong enum value | fix the value |
| `stylesheet X: …` | linked CSS file not found or not UTF-8 | fix the `href` (relative to the file containing it) |
| `CSS inlining failed: …` | CSS the inliner cannot parse | simplify the CSS; keep `@media` in `<style>` |
| `image X: cannot read …` / `not an image` / `unsupported image type` | missing file, HTML served instead of an image, BMP/TIFF/AVIF | fix the path; use PNG, JPEG, GIF, WebP or SVG |
| `image X: invalid SVG` / `SVG too large` | SVG does not parse, or the rasterised size is huge | fix the SVG; give the `<img>` a `width`/`height` |
| `text part: …` | HTML-to-text conversion failed | usually broken HTML; validate the body/letterhead |
| `recipient list line N: mail is X MB, above message.max_mail_size_mb (Y MB)` | big images or attachments | shrink them, use `message.images=link`, or raise the limit (max 40) |
| `message.max_mail_size_mb may not exceed 40 (Amazon SES limit after encoding)` | value above 40 | lower it |
| `invalid address `X`` / `invalid sender.on_behalf_of `X`` / `invalid sender.reply_to `X`` / `invalid address` for `sending.redirect_to` | malformed sender or redirect address | fix the setting |

## Sending

| Message | Cause | Fix |
|---|---|---|
| `nothing to send` | no row left after skipping and limiting | expected; check `recipients.skip_column`, `sending.limit` |
| `aborted, nothing sent` | confirmation not answered with `y`/`yes`/`j`/`ja`; also when stdin is closed or empty | run interactively and let the user answer, or `--yes` after the user's explicit go |
| `cannot write log.jsonl; sending stopped: …` | disk full or permissions while sending; no further mails were released, in-flight ones finished | the mails already handed to the server have no log line: see `modes/send.md`, "Interrupting", before resending |
| `SMTP connection or login failed: …` | host/port/security wrong, firewall, wrong credentials, SES SMTP credentials vs IAM keys mixed up | check `smtp.*` with `-v`; try the other port/security pair; regenerate SMTP credentials |
| ``sending.connections`` has no effect / too many connections | value is clamped to 1–10 | set 1–10; the rate limit applies across all connections |
| per-mail `failed` with a `4xx`/`5xx` text, then `N mail(s) failed` | server rejected that mail (throttling `454`, recipient rejected `550`, size `552`, sandbox `554 Message rejected: Email address is not verified`) | read `detail` in `log.jsonl`; lower the rate, verify identities or leave the sandbox, shrink the mail; resend only failed rows (`modes/send.md`) |
| mails arrive with the original `Message-ID` replaced | Amazon SES behaviour | expected; match by tags or address (`providers/amazon-ses.md`) |

## Received mail

| Symptom | Cause | Fix |
|---|---|---|
| Outlook (web) shows "pictures blocked / download pictures" although the logo is embedded | SES open tracking adds an external image (`awstrack.me`) | disable Opens and Clicks in the configuration set's event destinations (`providers/amazon-ses.md`, "Open and click tracking") |
| links point to `….awstrack.me/L0/…` | SES click tracking | same |
| test mail accepted (`Delivery` event, `250 … Queued`) but not in the inbox | the receiving server delivered it to junk/quarantine, or the client threads it into an earlier conversation with the same subject | look in junk/quarantine and the conversation view; the `Delivery` event in `events.jsonl` shows the receiving server's answer |

## Warnings

| Warning | Meaning | Action |
|---|---|---|
| `removed from HTML: <script>, onclick= attribute, …` | active content stripped | expected for pasted web HTML; remove it from the source to silence |
| `web-font rules removed from X` | `@import`/`@font-face` dropped | use a font stack with system fonts in `font-family` |
| `N image(s) without alt text` | accessibility and spam-score issue | add `alt` to every `<img>` (Markdown: `![alt](file.png)`) |
| `HTML part is N KB; Gmail clips messages above about 102 KB` | mail too long for Gmail's view | shorten the body or the inlined CSS |

## Bounces

| Message | Cause | Fix |
|---|---|---|
| `missing setting `aws.sqs_queue_url`` (also `aws.access_key_id`, `aws.secret_access_key`) | not configured | `setup/configuration.md`, "Bounces"; credentials from the file outside the repository, env or flag |
| `aws.sqs_queue_url is not a URL` / `aws.sqs_queue_url has no host` | malformed URL | use the full `https://sqs.<region>.amazonaws.com/<account>/<queue>` URL from the SQS console |
| `aws.region missing` | queue URL host is not `sqs.<region>.amazonaws.com` and `aws.region` unset | set `aws.region` |
| `SQS ReceiveMessage: HTTP 403 … AccessDenied` / `SQS DeleteMessage: HTTP 403 …` | IAM policy lacks `sqs:ReceiveMessage` or `sqs:DeleteMessage` on the queue ARN; wrong key; wrong region; session-token credentials | fix the policy or key; only long-lived IAM keys work |
| `SQS ReceiveMessage: HTTP 400 … InvalidAddress` / `QueueDoesNotExist` | queue URL wrong or in another region | copy the URL from the console |
| `SQS …: request failed` | network, DNS, proxy | retry; check connectivity to `sqs.<region>.amazonaws.com` |
| `cannot write <path>` | `run.state_dir` or a run folder not writable | fix permissions |
| `--wait` rejected | value outside 1–20 | pick 1–20 |
| `0 message(s) from the queue` although mails bounced | configuration set not on the mails (`ses.configuration_set`), event destination missing "Hard bounces"/"Complaints", SNS → SQS subscription missing or unconfirmed, queue policy without the topic ARN, different region; or the messages are still invisible after a `--keep` run (visibility timeout, default 30 s) | `providers/amazon-ses.md`, "Bounces"; wait half a minute after `--keep` |
| `N already recorded earlier, not written again` | events whose id is already in `events.jsonl`/`unmatched-events.jsonl` (earlier `--keep` run, crash between write and delete, SQS redelivery) | expected; the queue is cleaned up, nothing is duplicated |
| `U unmatched, R unreadable: …/unmatched-events.jsonl` | no tags (mail sent by 0.1.0) and address not in `to`/`cc`/`bcc` of any `log.jsonl` under `run.state_dir`; or a different `run.state_dir` than at send time; or a recipient of a multi-recipient event in no run; unreadable = queue message not JSON | run `bounces` with the same `run.state_dir`; match by address manually from the raw event |
| `bounces.csv` has empty `mail`/`line` | run matched by tag, but the mail number is not in the log (interrupted send) or no tags and address not in the log | look the address up in `mails/*.eml` of that run |
