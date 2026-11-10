# Bounces: `massmailer bounces`

**When to read this:** the day after a send, or when the user asks which
addresses did not receive the mail, complained, or need correcting.

**What you can do afterwards:** collect delivery events from the
provider queue, match them to run folders, read `bounces.csv` and tell
the user which list entries to remove or fix.

Requires the release after 0.1.0 and an Amazon SES setup that publishes
events to an SQS queue (`../providers/amazon-ses.md`, "Bounces"). The
AWS side is the user's responsibility; massmailer only reads the queue.

## Command

```sh
massmailer bounces -c account.json
massmailer bounces -c account.json --keep            # read without deleting (testing)
massmailer bounces -c account.json --wait 20         # long-poll 20 s per receive
```

Settings (`../setup/configuration.md`): `aws.region`,
`aws.sqs_queue_url`, `aws.access_key_id`, `aws.secret_access_key`
(secrets: `MASSMAILER_AWS_ACCESS_KEY_ID`,
`MASSMAILER_AWS_SECRET_ACCESS_KEY`, or `--aws-access-key-id` /
`--aws-secret-access-key`, or a separate config file outside any
repository), and `run.state_dir`, where the run folders to match
against live.

## What it does

1. Receives messages from the SQS queue with long polling (`--wait`
   seconds per receive, default 5) until the queue is empty, then
   stops.
2. Each message is an SNS envelope; its `Message` field holds the SES
   event JSON. Both shapes are read: configuration-set events
   (`eventType`) and identity notifications (`notificationType`).
3. Event types handled: **Bounce** (with `bounceType`
   `Permanent`/`Transient`/`Undetermined` and `bounceSubType`),
   **Complaint**, **Delivery**, **Reject**. Anything else is recorded
   as `other`.
4. Matching to a run folder: SES replaces the `Message-ID`, so the
   `message_id` in `log.jsonl` is useless for this. massmailer instead
   sets message tags on every sent mail (`X-SES-MESSAGE-TAGS:
   massmailer-run=<run folder name>, massmailer-mail=<NNNN>`), which
   come back in the event as `mail.tags`. If the tags are missing (mail
   sent by an older version, or tags not forwarded), the recipient
   address is matched case-insensitively against `log.jsonl` of the
   **newest** run folder under `run.state_dir` that contains it.
5. Output per matched run folder:
   - `events.jsonl`: every event for that run, raw, appended.
   - `bounces.csv`: one row per affected recipient for Bounce,
     Complaint and Reject events (Delivery events are not in the CSV).
     Columns: `mail, line, address, event, type, subtype, diagnostic,
     time`.
6. Events that match no run go to `<run.state_dir>/unmatched-events.jsonl`.
7. A queue message is deleted only after its event has been written.
   `--keep` never deletes, so the same events can be read again (use
   for testing the setup; the CSV then gets duplicate rows).
8. Exit summary: counts per event type and per run folder.

Run it repeatedly; bounces trickle in for hours, transient ones for
days. Every run appends.

## Reading bounces.csv

| `event` / `type` | Meaning | Action for the source list |
|---|---|---|
| `Bounce` / `Permanent` | mailbox does not exist, domain gone, blocked (`subtype` General, NoEmail, Suppressed, OnAccountSuppressionList) | remove or correct the address; do not send to it again (repeated permanent bounces hurt the sender reputation) |
| `Bounce` / `Transient` | mailbox full, message too large, content rejected (`subtype` MailboxFull, MessageTooLarge, ContentRejected, AttachmentRejected, General); with configuration-set events this appears only after SES stopped retrying, earlier the problem shows as a `DeliveryDelay` event (recorded as `other`, not in the CSV) | retry later for MailboxFull/General; MessageTooLarge/AttachmentRejected → shrink the mail; ContentRejected → look at the `diagnostic` |
| `Bounce` / `Undetermined` | the receiving server gave no usable reason | read `diagnostic`; treat like transient once, like permanent if it repeats |
| `Complaint` | the recipient marked the mail as spam | remove the address from the list; for statutory notices tell the user, since the person still has to be reached by other means |
| `Reject` | SES refused to send (e.g. virus detected in the mail) | the mail never left; fix the content and resend to that row |

`diagnostic` is the receiving server's text (`diagnosticCode`), `time`
the event timestamp, `mail` and `line` point into the run folder and
the recipient list (`line` = row with the header as line 1).

Report to the user by count and by `line`, e.g. "3 permanent bounces:
lines 17, 240, 1032"; show addresses only when asked. Corrections
belong in the user's source list, not in the run folder.

## Checking the pipeline before relying on it

Send one test mail to a provider sink address that bounces on purpose
(`../providers/amazon-ses.md`, "Mailbox simulator") with
`--redirect-to <sink> --limit 1`, wait a minute, run `bounces --keep`
and verify that `bounces.csv` appears in that test run's folder.
