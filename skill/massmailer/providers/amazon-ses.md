# Amazon SES

**When to read this:** the SMTP provider is Amazon SES; you need the
SMTP settings, the rate, the test addresses, or the AWS setup for
`bounces`.

**What you can do afterwards:** configure massmailer for SES, test the
delivery and bounce pipeline without real recipients, and describe to
the user what has to exist on the AWS side.

## SMTP endpoints

massmailer speaks plain SMTP with STARTTLS or implicit TLS and a
username/password login; it does not use the AWS API for sending. Two
kinds of SES SMTP endpoints fit:

1. **SES SMTP interface** (the usual case):
   `smtp.host = email-smtp.<region>.amazonaws.com`, `smtp.port = 587`
   with `smtp.security = starttls` (or `465` with `tls`). The login is
   a pair of **SMTP credentials** derived from an IAM user with
   `ses:SendRawEmail` permission; they are not the IAM access key and
   secret themselves. They are generated once in the SES console and
   cannot be shown again, only replaced.
2. **Mail Manager SMTP ingress endpoint** with a rule set whose action
   is "Send to internet": the endpoint's host name and port are shown
   after creation, the credentials are those configured in the
   endpoint's authentication settings. Same massmailer settings,
   different host, port and secret. This path was verified in one real
   setup: mails delivered, DKIM and DMARC pass, and the
   `X-SES-CONFIGURATION-SET` and `X-SES-MESSAGE-TAGS` headers take
   effect (bounce and complaint events with the tags arrived in the
   queue). Check the details against the current AWS
   documentation and console; Mail Manager bills per message on top of
   SES.

Either way: `sender.address` (and `sender.on_behalf_of` if used) must
be a verified identity (address or domain) in that region, and the
account must be out of the sandbox to reach unverified recipients.
Credentials go into `MASSMAILER_SMTP_USERNAME` /
`MASSMAILER_SMTP_PASSWORD` or a config file outside any repository.

## Configuration set

Set `ses.configuration_set` to the name of a configuration set in the
sending region. massmailer then adds `X-SES-CONFIGURATION-SET: <name>`
to every mail, so SES applies that set's event destinations (bounces,
complaints, deliveries) and, if enabled there, its dedicated IP pool
or custom tracking settings. Without the header, only the account-level
notifications fire.

## Message-ID and message tags

SES **replaces the `Message-ID`** with its own
(`<…@<region>.amazonses.com>`). The `message_id` in `log.jsonl` is
therefore not what the recipient sees and not what appears in bounce
events. For matching, massmailer sets `X-SES-MESSAGE-TAGS:
massmailer-run=<run folder name>, massmailer-mail=<NNNN>` (from
0.2.0); SES strips this header and delivers the tags in the event
JSON as `mail.tags`, which `bounces` uses. The header is not visible to
recipients.

## Rate and limits

- The account has a **maximum send rate** (mails per second) and a
  **daily quota**, both shown in the SES console. Keep
  `sending.rate_per_second` clearly below the rate (e.g. 3 when the
  limit is 14) and the row count below the remaining daily quota. SES
  answers `454 Throttling failure` when the rate is exceeded; such
  mails are logged as `failed` and are not retried.
- `sending.connections` (default 3) parallel SMTP connections share
  that rate: the dispatcher releases at most `rate_per_second` mails
  per second in total, whatever the number of connections. For an
  account limited to 1 mail/s set `sending.rate_per_second` to 1; the
  connections may stay at the default. Several connections only help
  when single sends are slow (large attachments, high latency).
- Message size: SES accepts up to 40 MB after encoding, which is why
  `message.max_mail_size_mb` is capped at 40; the default 5 MB is a
  sensible value for many mailboxes.
- Recipients per mail count individually against the quota; a mail
  with two `Cc` is three recipients.

## Mailbox simulator

SES provides addresses that never reach a mailbox and do not count
against the bounce/complaint statistics:

| Address | Result |
|---|---|
| `success@simulator.amazonses.com` | accepted and "delivered" (Delivery event) |
| `bounce@simulator.amazonses.com` | hard bounce (Bounce, Permanent) |
| `ooto@simulator.amazonses.com` | accepted, auto-reply (out of office) |
| `complaint@simulator.amazonses.com` | accepted, then Complaint event |
| `suppressionlist@simulator.amazonses.com` | rejected as on the suppression list (Bounce, Permanent, OnAccountSuppressionList) |

Combine with the test flags to exercise one real mail and the event
pipeline without touching the list:

```sh
massmailer send --limit 1 --redirect-to bounce@simulator.amazonses.com -c account.json -c mailing.json
# a minute later
massmailer bounces -c account.json --keep
```

The resulting `bounces.csv` in that test run's folder lists the
`massmailer-mail=0001` entry with the row's `line` and the simulator
address as `address`.

## Bounces: what has to exist on the AWS side

massmailer only reads an SQS queue; the chain that fills it is the
user's setup, all in the sending region:

1. **Configuration set** with an **event destination** of type
   Amazon SNS for the event types Bounce, Complaint, Reject and (if
   wanted) Delivery and Delivery delay. In the console the Bounce
   event type is labelled **"Hard bounces"**: it fires when the
   receiving server rejects the mail permanently, and for a temporary
   rejection only once SES has given up retrying (then with
   `bounceType` `Transient`). Temporary problems first appear as
   **Delivery delay** events (`DeliveryDelay`), which `bounces`
   records under that name in `events.jsonl` and not in `bounces.csv`.
   Enable "Delivery delays" if the user wants to see those too. (This
   reflects the SES event publishing documentation as known at writing
   time; check the event-type descriptions in the console.)
2. **SNS topic** that this destination publishes to.
3. **SQS queue** subscribed to the topic (standard queue; message
   retention 14 days, the maximum, so events survive until the user
   runs `bounces`; raw message delivery off (the default), so the SNS
   `MessageId` serves as the event id for de-duplication; raw delivery
   is read too). The queue policy must allow the topic to
   `sqs:SendMessage`.
4. **IAM user** for massmailer with a policy that allows only
   `sqs:ReceiveMessage` and `sqs:DeleteMessage` on that queue's ARN
   (these two are all `bounces` calls). Its long-lived access key pair
   goes into a separate config file outside any repository or into
   `MASSMAILER_AWS_ACCESS_KEY_ID` / `MASSMAILER_AWS_SECRET_ACCESS_KEY`;
   temporary credentials with a session token are not supported.
5. massmailer settings: `aws.sqs_queue_url` (the full
   `https://sqs.<region>.amazonaws.com/<account>/<queue>` URL; the
   region is taken from it, `aws.region` only for other host forms),
   `ses.configuration_set` on the sending side.

Identity-level notifications (SES identity → SNS topic, `notificationType`
JSON) can feed the same queue; `bounces` reads both shapes, with or
without the SNS envelope (raw message delivery works too, but then the
SQS message id is used as the event id for de-duplication).

## Deliverability checklist for the sender domain

Before a real send to a large list: SPF includes `amazonses.com` (or the
custom MAIL FROM domain is set up), DKIM via Easy DKIM is enabled and
verified, DMARC exists with a policy the user intends, and the test
send (`../modes/test-send.md`) lands in the inbox with
`spf=pass dkim=pass dmarc=pass` in the `Authentication-Results` header.
