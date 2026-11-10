# Dry run: `send --dry-run`

**When to read this:** before the first send of a mailing, and after any
change to inputs or settings once `check` is clean.

**What you can do afterwards:** run the complete send path, including
the log, without delivering anything; and know what a dry run does not
prove.

## Command

```sh
massmailer send --dry-run -c account.json -c mailing.json
massmailer send --dry-run --limit 5 --redirect-to me@example.net -c account.json -c mailing.json
```

`--limit` and `--redirect-to` work exactly as in a real send, so a dry
run of the planned test send shows the test subject and the redirected
`To` in the `.eml` files.

## What happens

1. Everything `check` does: all mails are built; one failure aborts.
2. No confirmation prompt, no SMTP connection, no login.
3. Every mail is "sent" to nowhere: a progress line per mail
   (`   1/2 dry-run anna@example.org`) and a `log.jsonl` entry with
   `"status": "dry-run"` and `"detail": "dry run, not sent"`.
4. Summary `done: N ok, 0 failed; log: …/log.jsonl`.

The run folder is `<run.state_dir>/<timestamp>-dry-run/` with `inputs/`,
`manifest.json`, `mails/NNNN.eml` and `log.jsonl`. This is the file set
of a real run, so it is the right place to verify the `line` → mail
mapping and the recipients per mail (`to` field in the log).

## What a dry run does not test

- **SMTP host, port, security, credentials.** `smtp.*` is not read.
  A wrong password shows up only in the test send
  (`../modes/test-send.md`).
- Deliverability: SPF/DKIM/DMARC, spam scoring, rendering in a real
  client. That is what the redirected test send is for.
- The rate: no pause between dry-run mails.

## Checklist before moving on

- `N mail(s) prepared from M row(s)` matches the user's expectation
  (skipped count, limit).
- No unexpected warnings.
- `log.jsonl` has one line per mail, with the right `to`.
- The user has seen the count, the subject and the sender address.
