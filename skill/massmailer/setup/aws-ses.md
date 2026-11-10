# AWS setup for Amazon SES

**When to read this:** the user wants to send through Amazon SES and
collect bounces, and the AWS side does not exist yet or is incomplete.

**What you can do afterwards:** tell the user, step by step, what to
look for in the AWS console, give him one self-contained prompt per
step for the AI assistant built into the console, prepare the DNS
records and policies, and verify each step from the outside.

Conventions: `<REGION>` (e.g. `eu-north-1`), `<DOMAIN>` (e.g.
`example.org`), `<ACCOUNT_ID>`, names in angle brackets are chosen by
the user; suggested names in the prompts: configuration set
`<CONFIG_SET>` = `massmailer`, topic `<TOPIC>` = `massmailer-events`,
queue `<QUEUE>` = `massmailer-events`, IAM user `<IAM_USER>` =
`massmailer-bounces`.

**Caveats for the whole file:** the console assistant's answers (and
this file) describe the console as it was known at writing time; menu
names, wizards and defaults change, so the user checks every answer
against what the console shows. Keys and passwords are created by the
user in the console and written by the user into the credentials file
(`local.md`, "Credentials"); they are not pasted into the chat with
the coding agent unless the user decides so.

## 1. Choose the region

All of SES (identity, configuration set, SMTP endpoint), the SNS topic
and the SQS queue must be in **one region**. Pick one close to the
recipients or required by data-location rules, and use it consistently:
`smtp.host = email-smtp.<REGION>.amazonaws.com`, `aws.region = <REGION>`.

What to look for: the region selector in the top bar of the console;
SES, SNS and SQS each show it.

Prompt for the console assistant:

> I want to send transactional email with Amazon SES from region
> <REGION> and receive bounce events in SQS in the same region. Confirm
> that SES, SNS and SQS are available in <REGION>, and tell me what the
> SES SMTP endpoint host name for <REGION> is.

## 2. Verify the sending domain (DKIM, SPF, DMARC, MAIL FROM)

What to look for: SES → Configuration → Identities → Create identity →
Domain. Choose Easy DKIM (2048 bit). SES shows three CNAME records;
the identity turns "Verified" after they resolve (minutes to hours).

DNS records to prepare for the user:

- **DKIM**: the three CNAMEs from the console, exactly as shown.
- **SPF**: a TXT at `<DOMAIN>` containing `include:amazonses.com`. If
  a TXT `v=spf1 …` already exists, add the include to it; a domain may
  have only one SPF record. Note: for SES the SPF check is done on the
  MAIL FROM domain, which is `amazonses.com` by default; the include
  at `<DOMAIN>` matters once a custom MAIL FROM domain is set (next
  point) and for DMARC alignment.
- **DMARC**: a TXT at `_dmarc.<DOMAIN>`, e.g.
  `v=DMARC1; p=none; rua=mailto:dmarc@<DOMAIN>` to start; tighten to
  `quarantine`/`reject` later. If one exists, leave it and tell the
  user what policy it enforces.
- **Custom MAIL FROM domain** (recommended): a subdomain such as
  `mail.<DOMAIN>`; SES shows an MX record and a TXT (SPF) record for
  it. With it, the envelope sender is in the user's domain and SPF
  aligns with `From` for DMARC.

Prompt:

> In Amazon SES, region <REGION>, I want to verify the domain <DOMAIN>
> as a sending identity with Easy DKIM (2048 bit) and set up a custom
> MAIL FROM domain mail.<DOMAIN>. List every DNS record I must create
> (DKIM CNAMEs, MAIL FROM MX and TXT, recommended SPF and DMARC TXT
> records), with record name, type and value, and tell me where in the
> console I can see the verification status.

Verification from the outside (agent): `dig CNAME <selector>._domainkey.<DOMAIN>`,
`dig TXT <DOMAIN>`, `dig TXT _dmarc.<DOMAIN>`, `dig MX mail.<DOMAIN>`.
Checkpoint: identity status "Verified", DKIM "Successful", MAIL FROM
"Successful".

Optional: verify the single sender address as well (Create identity →
Email address, click the link in the confirmation mail). Needed only
if the domain cannot be verified; while in the sandbox, recipients of
test mails must also be verified addresses.

## 3. Request production access (leave the sandbox)

In the sandbox, mail goes only to verified addresses, 200 per day, 1
per second. What to look for: SES → Account dashboard → "Request
production access" (or Get set up). The form asks for mail type, web
site, use-case description, and how bounces/complaints are handled.
Draft for the user, in his words:

> Mail type: transactional. We send formal notices (invitations and
> statutory notifications) to a fixed list of about <N> members and
> contacts of <ORGANISATION>, a few times per year, all within one
> night. Recipients are members who must receive these notices; this is
> not a newsletter and there is no marketing content. The list is
> maintained by hand; addresses that bounce permanently or complain are
> removed from the list before the next sending, based on SES bounce
> and complaint events delivered via SNS/SQS. Expected volume: <N>
> messages per sending, <M> sendings per year.

Prompt:

> Where in the Amazon SES console for region <REGION> do I request
> production access to leave the sandbox, which fields does the form
> have, and how long does the review usually take? Also show me where
> my current sending quota and maximum send rate are displayed.

Checkpoint: account dashboard shows "Production access: Enabled" and a
daily quota above the list size.

## 4. SMTP access

Two options; massmailer needs only host, port, security and a
username/password (`../providers/amazon-ses.md`, "SMTP endpoints").

**Option A: SES SMTP credentials** (standard, no extra charge). What to
look for: SES → SMTP settings → "Create SMTP credentials". This creates
an IAM user and shows an SMTP user name and SMTP password once;
download or copy them straight into the credentials file. Endpoint
`email-smtp.<REGION>.amazonaws.com`, port 587 STARTTLS (or 465 TLS).

Prompt:

> In Amazon SES, region <REGION>, I want to create SMTP credentials for
> a mail-sending program. Walk me through "Create SMTP credentials",
> tell me which IAM permission the created user gets, confirm the SMTP
> endpoint host and the ports for STARTTLS and TLS, and explain why the
> SMTP password differs from the IAM secret access key.

**Option B: Mail Manager SMTP ingress endpoint** (for users who already
use Mail Manager; extra per-message charge). What to look for: SES →
Mail Manager → Ingress endpoints → Create (type Authenticated, with
managed credentials or a secret), a traffic policy that allows the
sender, and a rule set with a rule whose action is "Send to internet"
(outbound) for mail from the user's identity. The endpoint's host name
is shown after creation; port and credentials as configured there.
This path was verified in one real setup (mails delivered, DKIM and
DMARC pass); whether configuration-set events for `bounces` arrive
through it has not been confirmed yet. Check every detail against the
current AWS documentation and the console.

Prompt:

> In Amazon SES Mail Manager, region <REGION>, I want an authenticated
> SMTP ingress endpoint that my program can log in to with a username
> and password and that relays mail from <DOMAIN> to the internet.
> Explain the ingress endpoint, traffic policy and rule set with a
> "Send to internet" action I need, where the endpoint host name and
> port appear, how the credentials are set, and what this costs per
> message compared with the plain SES SMTP interface.

Checkpoint: `account.json` has `smtp.host`, `smtp.port`,
`smtp.security`; the credentials file has `smtp.username` and
`smtp.password`; a test send with `--limit 1 --redirect-to <verified
address>` arrives (`local.md`, step 4).

## 5. Configuration set

What to look for: SES → Configuration → Configuration sets → Create
set, name `<CONFIG_SET>`; defaults are fine (TLS "Optional" or
"Require", reputation metrics on). Put the name into
`ses.configuration_set` in `account.json`.

Prompt:

> Create a configuration set named <CONFIG_SET> in Amazon SES, region
> <REGION>. Explain the options on the create form (TLS policy,
> reputation metrics, sending enabled, suppression list overrides) and
> which defaults are sensible for transactional mail.

## 6. Event destination → SNS topic

What to look for: in the configuration set → Event destinations → Add
destination. Event types: **Hard bounces** (this is the `Bounce` event
massmailer evaluates), **Complaints**, **Deliveries**, **Rejects**,
optionally **Delivery delays** (temporary problems; `bounces` records
them as `DeliveryDelay` in `events.jsonl`, not in `bounces.csv`).
Destination type Amazon SNS → create a new
topic `<TOPIC>` (standard). Not needed: Sends, Opens, Clicks (tracking
is wrong for formal notices), Rendering failures, Subscriptions.

Prompt:

> In Amazon SES, region <REGION>, configuration set <CONFIG_SET>, add an
> event destination of type Amazon SNS that creates a new standard SNS
> topic named <TOPIC>, with the event types Hard bounces, Complaints,
> Deliveries, Rejects and Delivery delays enabled, and Opens and Clicks
> disabled. Walk me through the form and show me the topic ARN
> afterwards.

Checkpoint: the destination shows the topic ARN
`arn:aws:sns:<REGION>:<ACCOUNT_ID>:<TOPIC>` and status enabled.

## 7. SQS queue and subscription

What to look for: SQS → Create queue → **Standard**, name `<QUEUE>`;
Message retention period **14 days** (maximum, so events wait until
`bounces` runs); Access policy "Advanced" with a statement that lets
the topic send; then SNS → topic → Create subscription → protocol SQS,
endpoint = queue ARN, "Enable raw message delivery" **off** (the
default; massmailer then uses the SNS `MessageId` to recognise events it
has already recorded; raw delivery is read as well). SQS subscriptions
in the same account confirm automatically.

Queue access policy to prepare (replace placeholders):

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Sid": "AllowSnsTopicToSend",
    "Effect": "Allow",
    "Principal": { "Service": "sns.amazonaws.com" },
    "Action": "sqs:SendMessage",
    "Resource": "arn:aws:sqs:<REGION>:<ACCOUNT_ID>:<QUEUE>",
    "Condition": { "ArnEquals": { "aws:SourceArn": "arn:aws:sns:<REGION>:<ACCOUNT_ID>:<TOPIC>" } }
  }]
}
```

Prompt:

> In Amazon SQS, region <REGION>, create a standard queue named <QUEUE>
> with a message retention period of 14 days and an access policy that
> allows only the SNS topic arn:aws:sns:<REGION>:<ACCOUNT_ID>:<TOPIC> to
> call sqs:SendMessage on it. Then subscribe this queue to that SNS
> topic with raw message delivery disabled, and show me how to confirm
> that the subscription is confirmed and how to see the queue URL.

Checkpoint: subscription status "Confirmed"; the queue URL
`https://sqs.<REGION>.amazonaws.com/<ACCOUNT_ID>/<QUEUE>` goes into
`aws.sqs_queue_url`; massmailer reads the region from that host, so
`aws.region` is only needed for another host form.

## 8. IAM user that may only read the queue

What to look for: IAM → Users → Create user `<IAM_USER>`, no console
access; attach an inline policy with the JSON below; then Security
credentials → Create access key (use case "Application running outside
AWS"). The key pair is shown once; the user writes it into the
credentials file (`aws.access_key_id`, `aws.secret_access_key`). A
long-lived key is needed: massmailer does not support temporary
credentials with a session token.

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Action": ["sqs:ReceiveMessage", "sqs:DeleteMessage"],
    "Resource": "arn:aws:sqs:<REGION>:<ACCOUNT_ID>:<QUEUE>"
  }]
}
```

Prompt:

> In IAM, create a user named <IAM_USER> without console access and
> with an inline policy that allows only sqs:ReceiveMessage and
> sqs:DeleteMessage on the queue
> arn:aws:sqs:<REGION>:<ACCOUNT_ID>:<QUEUE>, nothing else. Then show me
> how to create an access key for this user for an application running
> outside AWS and where I can download it once.

Checkpoint: `massmailer bounces --keep -c account.json -c <credentials
file>` runs without an access error (queue may be empty).

## 9. Verify end to end

1. Test send to the simulator address that bounces on purpose:

   ```sh
   massmailer send --limit 1 --redirect-to bounce@simulator.amazonses.com -c account.json -c mailing.json -c <credentials file>
   ```

   (`success@simulator.amazonses.com` for a Delivery event,
   `complaint@simulator.amazonses.com` for a Complaint; list in
   `../providers/amazon-ses.md`.)
2. In the console: SQS → `<QUEUE>` → "Send and receive messages" →
   "Poll for messages". Within about a minute a message appears whose
   body is the SNS envelope; its `Message` field holds JSON with
   `"eventType": "Bounce"` and `"mail": {"tags": {"massmailer-run":
   [...], "massmailer-mail": [...]}}`. Polling in the console does not
   delete messages.

   Prompt:

   > Show me how to look at the messages currently in the SQS queue
   > <QUEUE> in region <REGION> from the console without deleting
   > them, and how to read the message body.

3. Locally:

   ```sh
   massmailer bounces --keep -c account.json -c <credentials file>
   ```

   Checkpoint: the summary counts one Bounce, and the test run's folder
   has `events.jsonl` and `bounces.csv` with one `Permanent` row for
   `mail 0001`. Then run once without `--keep` to clear the queue:
   the event is recognised by its id and not written twice (`1 already
   recorded earlier, not written again`). After `--keep` the received
   messages stay invisible for the queue's visibility timeout (default
   30 s), so wait half a minute, or the second run sees an empty queue
   and the message is cleared by a later run.

If nothing arrives: `ses.configuration_set` set and spelled like the
configuration set? Event destination enabled with "Hard bounces"?
Subscription confirmed? Queue policy with the topic ARN? Region the
same everywhere? Did a `--keep` run less than a visibility timeout
(30 s) ago hide the messages? `../troubleshooting.md`, "Bounces".
