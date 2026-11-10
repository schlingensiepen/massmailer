# Bounces: `massmailer bounces`

**When to read this:** the day after a send, or when the user asks which
addresses did not receive the mail, complained, or need correcting.

**What you can do afterwards:** collect delivery events from the
provider queue, match them to run folders, read `bounces.csv` and tell
the user which list entries to remove or fix.

Requires massmailer 0.2.0 and an Amazon SES setup that publishes events
to an SQS queue (`../providers/amazon-ses.md`, "Bounces"). The AWS side
is the user's responsibility; massmailer only reads the queue.

## Command

```sh
massmailer bounces -c account.json -c ~/mailings-secrets/aws.json
massmailer bounces -c account.json -c ~/mailings-secrets/aws.json --keep      # read without deleting (testing)
massmailer bounces -c account.json -c ~/mailings-secrets/aws.json --wait 20   # long-poll 20 s per receive
```

- `--keep`: never delete queue messages.
- `--purge`: also delete events of runs that are not in this
  `run.state_dir` (saved to `unmatched-events.jsonl` first). Without
  it (0.2.1 and later) such events **stay in the queue**, so several
  mailings with different state dirs can share one queue; the summary
  says `N event(s) of runs not in this run.state_dir left in the
  queue`. Simplest setup: one common `run.state_dir` for all mailings.
- `send --bounces-before` / `--bounces-after N` / `--purge-bounces`
  call this from a send (`../modes/send.md`).
- `--wait N`: long-poll seconds per receive call, 1–20, default 5.
- `--aws-access-key-id`, `--aws-secret-access-key`: credentials as
  flags (visible in the process list; prefer the file or environment).
- `-v`: show the effective settings (secrets masked).

Settings (`../setup/configuration.md`): `aws.sqs_queue_url`,
`aws.region` (optional when the queue URL host is
`sqs.<region>.amazonaws.com`), `aws.access_key_id`,
`aws.secret_access_key` (long-lived IAM key; no session tokens), and
`run.state_dir`: it must be the **same state dir the send used**,
because the run folders there are what events are matched against.

## What it does

1. Receives messages from the SQS queue with long polling until a
   receive returns nothing, then stops.
2. Each message is an SNS envelope whose `Message` field holds the SES
   event JSON; a body that is the SES event itself (raw message
   delivery) is read as well. Both event shapes are understood:
   configuration-set events (`eventType`) and identity notifications
   (`notificationType`).
3. Event types: **Bounce** (with `bounceType`
   `Permanent`/`Transient`/`Undetermined` and `bounceSubType`),
   **Complaint**, **Delivery**, **Reject**, **DeliveryDelay**
   (`delayType`). Every other type (Send, Open, Click, …, if the event
   destination publishes them) is recorded under its SES name; `other`
   only when neither `eventType` nor `notificationType` is present.
   Recipients come from the event's recipient list; when that is
   missing (e.g. a Complaint without `complainedRecipients`),
   `mail.destination` is used.
4. Matching to a run folder: SES replaces the `Message-ID`, so the
   `message_id` in `log.jsonl` is useless for this. massmailer sets
   message tags on every sent mail (`X-SES-MESSAGE-TAGS:
   massmailer-run=<run folder name>, massmailer-mail=<NNNN>`), which
   come back in the event as `mail.tags`. Without usable tags (mail
   sent by 0.1.0, or identity notifications), each recipient address
   is matched case-insensitively against `to`, `cc` and `bcc` of the
   `sent` entries in `log.jsonl` of the **newest** run folder under
   `run.state_dir` that contains it.
5. Output per matched run folder:
   - `events.jsonl`: one line per event, appended, as a wrapper
     `{"received": <local time>, "id": <event id>, "event": <type>,
     "raw": <SES event JSON>}`. `id` is the SNS `MessageId`, or the SQS
     `MessageId` with raw delivery.
   - `bounces.csv`: one row per affected recipient for **Bounce,
     Complaint and Reject** only. Header: `mail, line, address, event,
     type, subtype, diagnostic, time`. Cells that start with `=`, `+`,
     `-` or `@` get a leading `'` so spreadsheets do not evaluate them.
   An event with several recipients may be split over several runs.
6. Events with a recipient that matches no run go to
   `<run.state_dir>/unmatched-events.jsonl` (same wrapper; also events
   partly matched, which then appear both in a run and here). Queue
   messages that are not JSON are written there with `"event":
   "unreadable"` and the body.
7. **No duplicates**: before writing, the ids already present in all
   `events.jsonl` files and in `unmatched-events.jsonl` are loaded;
   an event with a known id is counted as `already recorded earlier,
   not written again`. A message redelivered within the same call is
   skipped (and deleted unless `--keep`).
8. A queue message is deleted only after its event has been written,
   so a crash cannot lose events (it can only cause a duplicate, which
   step 7 absorbs). With `--keep`, nothing is deleted; the call ends
   when the queue returns nothing new or a whole batch was already
   seen in this call. Received messages stay invisible for the queue's
   visibility timeout (default 30 s), so a second run right after
   `--keep` may see an empty queue; wait half a minute.
9. Exit summary on stderr: `N message(s) from the queue`, counts per
   event type, `run <folder>: N event(s)` per run, duplicates, and
   `U unmatched, R unreadable: <path>` when applicable.

Run it repeatedly; bounces trickle in for hours, transient problems for
days. Every run appends, never duplicates.

## Reading bounces.csv

| `event` / `type` | Meaning | Action for the source list |
|---|---|---|
| `Bounce` / `Permanent` | mailbox does not exist, domain gone, blocked (`subtype` General, NoEmail, Suppressed, OnAccountSuppressionList) | remove or correct the address; do not send to it again (repeated permanent bounces hurt the sender reputation) |
| `Bounce` / `Transient` | mailbox full, message too large, content rejected (`subtype` MailboxFull, MessageTooLarge, ContentRejected, AttachmentRejected, General); with configuration-set events this appears only after SES stopped retrying, earlier the problem shows as a `DeliveryDelay` event (in `events.jsonl`, not in the CSV) | retry later for MailboxFull/General; MessageTooLarge/AttachmentRejected → shrink the mail; ContentRejected → look at the `diagnostic` |
| `Bounce` / `Undetermined` | the receiving server gave no usable reason | read `diagnostic`; treat like transient once, like permanent if it repeats |
| `Complaint` | the recipient marked the mail as spam (`type` = feedback type, e.g. abuse) | remove the address from the list; for statutory notices tell the user, since the person still has to be reached by other means |
| `Reject` | SES refused to send (`type` = reason, e.g. virus detected) | the mail never left; fix the content and resend to that row |

`diagnostic` is the receiving server's text (`diagnosticCode`), `time`
the event timestamp, `mail` and `line` point into the run folder and
the recipient list (`line` = row with the header as line 1). `mail` and
`line` are empty when the run was found but the specific mail was not
(no tags and address not in the log).

Report to the user by count and by `line`, e.g. "3 permanent bounces:
lines 17, 240, 1032"; show addresses only when asked. Corrections
belong in the user's source list, not in the run folder.

`DeliveryDelay` events and `Delivery` confirmations are only in
`events.jsonl`; to see delays, filter that file for `"event":
"DeliveryDelay"`.

## Checking the pipeline before relying on it

Send one test mail to a provider sink address that bounces on purpose
(`../providers/amazon-ses.md`, "Mailbox simulator") with
`--redirect-to <sink> --limit 1`, wait a minute, run `bounces --keep`
and verify that `bounces.csv` appears in that test run's folder. A
later run without `--keep` clears the queue and writes nothing twice.
