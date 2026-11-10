# Real send

**When to read this:** right before a real send, and when a send ended
with failures or was interrupted.

**What you can do afterwards:** run the send with the user's go, read
its progress and log, and handle failed or unlogged mails.

## Prerequisites (all of them)

- `check` clean, `send --dry-run` clean, test send received and
  approved by the user (`test-send.md`).
- No `sending.limit` or `sending.redirect_to` left in any config file
  or environment variable: `massmailer send --dry-run -v …` and look
  for both keys in the settings list.
- `smtp.host`, `smtp.username`, `smtp.password` available; credentials
  from a separate config file outside any repository, the environment,
  or flags typed by the user (`../setup/configuration.md`, "Secrets").
- `sending.rate_per_second` below the provider's limit; `sending.connections`
  (default 3) fits the provider (`../providers/amazon-ses.md`).
- Enough time: N mails at rate r take about N / r seconds (1,900 mails
  at 3/s ≈ 11 minutes; at 1/s ≈ 32 minutes). The process must stay
  alive for that long; run it in a terminal that is not going to close.
- **The user's explicit go for this run**, after seeing the mail count
  from the dry run, the subject and the sender.

## Command

```sh
massmailer send -c account.json -c mailing.json -c ~/mailings-secrets/smtp.json
```

Sequence:

1. Run folder `<run.state_dir>/<timestamp>-send/` is created.
2. All mails are built and written as `mails/NNNN.eml`. If a single one
   fails, the run aborts here and **nothing has been sent**. If no row
   is left after skipping and limiting, the run ends with `nothing to
   send` and an empty `log.jsonl`, without prompting or connecting.
3. Prompt on stdin: `Send N mail(s) now? [y/N]`. Accepted: `y`, `yes`,
   `j`, `ja`. Anything else, including an empty line or a closed stdin,
   aborts with `error: aborted, nothing sent`. `-y`/`--yes` skips the
   prompt; use it only when the user has given the go and cannot type
   into the prompt themselves.
4. SMTP connection and login are tested once (`SMTP connection or login
   failed` aborts, still nothing sent).
5. Sending: a dispatcher releases mails in order, at most one per
   `1 / sending.rate_per_second` seconds (gap measured between
   hand-offs), to `sending.connections` worker threads that each hold
   an SMTP connection from a pool. So up to `connections` mails are in
   flight at any moment, while the start rate never exceeds the
   setting. Each worker prints `   i/N sent recipient@example.org` (or
   `failed`) when its mail is done and the result is appended to
   `log.jsonl` immediately, flushed per line, with the completion time.
   **Log lines are therefore in completion order, not mail order**;
   every line carries `mail` and `line`, so order does not matter for
   reading it.
6. Summary `done: N ok, F failed; log: …`. Exit code 1 if F > 0.

If a log line cannot be written (disk full, permissions), the
dispatcher releases no further mails, the mails already in flight
finish, and the run ends with `cannot write log.jsonl; sending
stopped`. The mails whose lines could not be written were sent; see
"Interrupting" for how to recover.

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

Ctrl-C kills the process at once. `log.jsonl` has every mail whose
result had arrived; but up to `sending.connections` mails were in
flight at that moment, and a mail the server had already accepted may
have no log line. So, before resending:

1. From `log.jsonl`, list the `mail` numbers present; the missing ones
   are the candidates (`mails/NNNN.eml` exists for all of them).
2. The first few missing numbers (at most `sending.connections`) may
   have been delivered. Check them against the provider's sent
   statistics or message search (SES: the sending statistics or a
   Delivery event via `bounces`) before deciding; when in doubt, ask
   the user whether a duplicate to those few recipients is acceptable.
3. Resend the remaining rows as described above.

## After the send

- Keep the run folder; it is the record of what was sent to whom
  (`../results/run-folder.md`).
- Next day: `massmailer bounces` with the same `run.state_dir`
  (`../results/bounces.md`).
