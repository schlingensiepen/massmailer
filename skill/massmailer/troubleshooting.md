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
| `aborted, nothing sent` | confirmation not answered with `y`/`yes`/`j`/`ja`; also when stdin is closed or empty | run interactively and let the user answer, or `--yes` after the user's explicit go |
| `SMTP connection or login failed: …` | host/port/security wrong, firewall, wrong credentials, SES SMTP credentials vs IAM keys mixed up | check `smtp.*` with `-v`; try the other port/security pair; regenerate SMTP credentials |
| per-mail `failed` with a `4xx`/`5xx` text, then `N mail(s) failed` | server rejected that mail (throttling `454`, recipient rejected `550`, size `552`, sandbox `554 Message rejected: Email address is not verified`) | read `detail` in `log.jsonl`; lower the rate, verify identities or leave the sandbox, shrink the mail; resend only failed rows (`modes/send.md`) |
| mails arrive with the original `Message-ID` replaced | Amazon SES behaviour | expected; match by tags or address (`providers/amazon-ses.md`) |

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
| missing `aws.*` setting | not configured | `setup/configuration.md`, "Bounces" |
| access denied on receive/delete | IAM policy lacks `sqs:ReceiveMessage`/`sqs:DeleteMessage`/`sqs:GetQueueAttributes` on the queue, or wrong region | fix the policy or `aws.region` |
| queue empty, no events although mails bounced | configuration set not set on the mails (`ses.configuration_set`), event destination missing event types, SNS → SQS subscription missing or raw delivery on | `providers/amazon-ses.md`, "Bounces" |
| events in `unmatched-events.jsonl` | no tags (older binary) and address not found in any `log.jsonl` under `run.state_dir`; or a different `run.state_dir` than at send time | run `bounces` with the same `run.state_dir`; match by address manually from the raw event |
