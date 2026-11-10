# Real send

**When to read this:** right before a real send, and when a send ended
with failures.

**What you can do afterwards:** run the send with the user's go, read
its progress and log, and handle failed mails.

## Prerequisites (all of them)

- `check` clean, `send --dry-run` clean, test send received and
  approved by the user (`test-send.md`).
- No `sending.limit` or `sending.redirect_to` left in any config file
  or environment variable: `massmailer send --dry-run -v …` and look
  for both keys in the settings list.
- `smtp.host`, `smtp.username`, `smtp.password` available; credentials
  via environment or flag, never in a repository file.
- `sending.rate_per_second` below the provider's limit
  (`../providers/amazon-ses.md`).
- Enough time: N mails at rate r take about N / r seconds (1,900 mails
  at 3/s ≈ 11 minutes; at 1/s ≈ 32 minutes). The process must stay
  alive for that long; run it in a terminal that is not going to close.
- **The user's explicit go for this run**, after seeing the mail count
  from the dry run, the subject and the sender.

## Command

```sh
massmailer send -c account.json -c mailing.json
```

Sequence:

1. Run folder `<run.state_dir>/<timestamp>-send/` is created.
2. All mails are built and written as `mails/NNNN.eml`. If a single one
   fails, the run aborts here and **nothing has been sent**.
3. Prompt on stdin: `Send N mail(s) now? [y/N]`. Accepted: `y`, `yes`,
   `j`, `ja`. Anything else, including an empty line or a closed stdin,
   aborts with `error: aborted, nothing sent`. `-y`/`--yes` skips the
   prompt; use it only when the user has given the go and cannot type
   into the prompt themselves.
4. SMTP connection and login are tested once (`SMTP connection or login
   failed` aborts, still nothing sent).
5. Mails go out one by one with a gap of `1 / sending.rate_per_second`
   seconds measured from the start of the previous send. Each mail
   prints `   i/N sent recipient@example.org` (or `failed`) and appends
   a line to `log.jsonl`.
6. Summary `done: N ok, F failed; log: …`. Exit code 1 if F > 0.

## Failures during sending

A failed SMTP delivery of one mail (server rejected it, connection
dropped) does **not** stop the run; the mail is logged with
`"status": "failed"` and the server's message in `detail`, and the
next mail follows. There is no automatic retry and no resume command.

To resend only the failed ones:

1. Collect the `line` values of entries with `"status": "failed"` from
   `log.jsonl` (`../results/run-folder.md`).
2. Build a new list containing only those rows (or add a skip column
   with `no` for the others) and run the workflow again from `check`.
   The new run gets its own folder.

Do not simply rerun the full send: everybody with `"status": "sent"`
would receive the mail twice.

## Interrupting

Ctrl-C stops the process between mails; `log.jsonl` has every mail
that was handed to the server up to that point (the entry is written
right after each attempt). Resume as described above, treating all
rows not in the log as not sent.

## After the send

- Keep the run folder; it is the record of what was sent to whom
  (`../results/run-folder.md`).
- Next day: `massmailer bounces` (`../results/bounces.md`).
