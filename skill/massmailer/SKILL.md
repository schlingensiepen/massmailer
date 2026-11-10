---
name: massmailer
description: Send personalised bulk email (formal notices, invitations, press releases) to a known recipient list with the massmailer CLI, via SMTP such as Amazon SES. Use when a user wants to prepare, test, send or audit a mailing from a CSV/XLSX/ODS/JSON list and a Markdown/HTML/text body, or to collect bounces of a past run.
---

# massmailer

`massmailer` is a command-line tool: one body file (Markdown, HTML or
plain text) with placeholders + one recipient list = one mail per row,
wrapped in a letterhead, sent via SMTP at a limited rate and recorded in
a run folder. This skill matches massmailer **0.2.1** (`massmailer
--version`); the `bounces` command, the `X-SES-MESSAGE-TAGS` header and
parallel SMTP connections (`sending.connections`) exist from 0.2.0 on,
`send --bounces-before/--bounces-after/--purge-bounces` and
`bounces --purge` from 0.2.1.

## When to use, when not

Use it for mail that every person on a list must receive: statutory
notices, invitations, press releases, announcements to members. Do not
use it for newsletters or marketing: there is no unsubscribe link, no
List-Unsubscribe header and no opt-out handling, by design. If the user
needs opt-out, point to a newsletter service instead.

## Hard rules for the agent

1. **Never run a real send without the user's explicit go for that
   run.** "Go" means: the user has seen the mail count, the list, the
   subject and the sender, and says send. `send` without `--dry-run`
   asks "Send N mail(s) now? [y/N]" on stdin; `--yes` suppresses that
   question, so only pass `--yes` after the user said go, never to make
   a command "work" non-interactively.
2. **Always dry-run first, then a test send** with `--limit 1 --redirect-to
   <user's own address>`, then wait for the user to confirm the received
   mail looks right. Only then the real send.
3. **Never write credentials** (SMTP password, AWS keys, bearer tokens)
   into config files inside a repository, into chat or into shell
   history you control. The three acceptable places: a separate config
   file outside any repository with restrictive permissions (passed as
   the last `-c`), environment variables (`MASSMAILER_SMTP_PASSWORD`
   etc.), or CLI flags typed by the user (`--smtp-password`,
   `--aws-secret-access-key`; visible in the process list, so prefer
   file or environment). `-v` masks secrets; still do not paste its
   output into shared places.
4. **Do not print recipient data** (addresses, names, rows) unless the
   user asks for it. `columns` shows only column names; `check` shows
   only counts. Quote a row only when an error names it and the user
   needs to fix it.
5. Nothing is sent if one mail cannot be built; use that: a failing
   `check` is a fix-the-input step, not a reason to work around it.
6. Keep `sending.rate_per_second` below the provider's limit (see
   `providers/amazon-ses.md`).

## Standard workflow

```sh
massmailer columns -c mailing.json                       # 1 column names
massmailer check   -c mailing.json                       # 2 every mail builds?
massmailer render  -c mailing.json                       # 3 look at .html/.txt/.eml
massmailer send --dry-run -c account.json -c mailing.json            # 4 full run, no SMTP
massmailer send --limit 1 --redirect-to me@example.net \
                -c account.json -c mailing.json          # 5 real mail to the user
massmailer send -c account.json -c mailing.json          # 6 real send, after the go
massmailer bounces -c account.json                       # 7 next day: bounces
                                                         #   (or send --bounces-after 5)
```

`-c` may be given several times; the later file wins. Keep account
(SMTP host, rate, `run.state_dir`, `aws.*`) and mailing (sender, body,
list) in separate files; credentials in a third file outside any
repository or in the environment. Every step 2-6 creates a run folder
(default `massmailer-runs/`) with a copy of all inputs, every mail as
`.eml` and, from step 4 on, a `log.jsonl`. Step 7 must run with the
**same `run.state_dir`** as the send, because it matches events to the
run folders there.

## Installing this skill

Installing is the user's choice. Offer two options and let them pick:
install the skill permanently (copy `skill/massmailer/` to wherever
their agent harness keeps skills) or just read these files for the
current session. Do not assume a path or a harness.

## Reference files

Setup

- `setup/onboarding.md` — read when the user has no working setup yet
  (no binary, no config, no SMTP access, no bounce pipeline); the
  setup entry point: local and AWS halves, order of steps, what to ask
  the user, what the user must do himself, checkpoints.
- `setup/local.md` — read when creating the folders and files for a
  mailing or placing credentials; folder layout, the credentials file
  outside any repository, the first tests up to one real test mail.
- `setup/aws-ses.md` — read when the AWS side (domain verification,
  production access, SMTP access, configuration set, SNS/SQS/IAM for
  bounces) has to be created; what to look for in the console, one
  copy-ready prompt per step for the console's AI assistant, policies,
  and how to verify each step.
- `setup/install.md` — read when `massmailer` is not on the PATH, is
  outdated or the user asks how to install or update; install scripts
  for Windows/Linux/macOS, release assets, version check, uninstall,
  build from source.
- `setup/configuration.md` — read before writing or editing a config
  file and whenever a setting is in doubt; the **only complete settings
  reference**, precedence (files < environment < command line), how
  relative paths resolve, how to keep secrets out of files.

Input

- `input/recipients.md` — read before pointing at a list or when
  `check` complains about a row; CSV/XLSX/ODS/JSON rules, address
  columns, several addresses per cell, skip column, how to find and
  report bad rows without dumping the list.
- `input/content.md` — read before writing a body, letterhead or
  subject; Markdown/HTML/text bodies, MiniJinja placeholders and
  escaping, letterhead `layout.html`/`layout.txt`, CSS, images, SVG,
  preheader, attachments, size limit, headers massmailer sets.

Modes

- `modes/inspect.md` — read first for any new list or config; what
  `columns` and `check` do, what they print and what they write.
- `modes/render.md` — read when the user wants to see the mails before
  sending; `render` writes `.eml`, `.html` and `.txt` per mail.
- `modes/dry-run.md` — read before the first send of a mailing; `send
  --dry-run` runs everything except SMTP and what it does *not* test.
- `modes/test-send.md` — read before the first real send; how `--limit`
  and `--redirect-to` deliver real mail to one test address, how the
  test subject looks and what to check in the received mail.
- `modes/send.md` — read right before a real send; prerequisites, the
  confirmation prompt and `--yes`, rate, what happens on failures, how
  to resend only failed mails.

Results

- `results/run-folder.md` — read when auditing a run or answering "what
  was sent to whom"; folder layout, `manifest.json`, `mails/NNNN.eml`,
  `log.jsonl` fields and statuses.
- `results/bounces.md` — read the day after a send or when the user asks
  about undeliverable mail; the `bounces` command, the files it writes
  and what to do with permanent and transient bounces.

Providers

- `providers/amazon-ses.md` — read when the SMTP provider is Amazon SES;
  configuration set, replaced Message-ID, message tags, rate vs account
  limit, mailbox-simulator test addresses, SNS/SQS setup for bounces,
  SMTP endpoints and credentials.

Troubleshooting

- `troubleshooting.md` — read when a command prints `error:` or
  `warning:`; message → cause → fix for every message the tool emits.
