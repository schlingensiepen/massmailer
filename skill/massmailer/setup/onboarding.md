# Setting massmailer up: the entry point

**When to read this:** the user does not have a working massmailer setup
yet (no binary, no config, no SMTP access, no bounce pipeline), or asks
"how do I get this running".

**What you can do afterwards:** lead the user through the setup in the
right order, know which steps you can do, which the user has to do
himself, what to ask, and how to confirm each step before the next.

Setting up is a different job from using. For using, start at
`../SKILL.md`.

## The two halves

1. **Local** (`local.md`): the CLI, a folder for the mailing, the two
   config files, the letterhead, the list, the credentials file outside
   any repository, and the first dry runs. You can do most of this; the
   user supplies content and credentials.
2. **Provider / AWS** (`aws-ses.md`): a verified sending domain with
   DKIM/SPF/DMARC, production access, SMTP credentials or a Mail Manager
   ingress endpoint, a configuration set, and the event destination →
   SNS → SQS → IAM chain for `bounces`. The user does this in the AWS
   console and at the DNS provider; you prepare, explain and verify.

Another SMTP provider works for sending (steps 1-3 of the AWS half have
equivalents everywhere: domain authentication, account limits,
credentials); `bounces` is Amazon-SES-specific.

## Order of steps and checkpoints

| # | Step | Who | Checkpoint |
|---|---|---|---|
| 1 | Install the CLI (`install.md`) | agent or user | `massmailer --version` prints a version |
| 2 | Create the mailing folder with `mailing.json`, body, letterhead, a **small fictional** list for testing (`local.md`) | agent | `massmailer columns` and `check` succeed |
| 3 | Render and look at the mails (`../modes/render.md`) | agent, user looks | user approves subject, letterhead, text part |
| 4 | Choose the AWS region; verify the domain; DNS records; wait for "Verified" (`aws-ses.md` 1-2) | user, agent prepares the records | SES shows identity and DKIM as verified |
| 5 | Request production access (`aws-ses.md` 3) | user | AWS confirms (hours to a day); until then only verified addresses receive mail |
| 6 | SMTP access: SES SMTP credentials or Mail Manager ingress (`aws-ses.md` 4) | user creates, writes credentials into the credentials file | `account.json` + credentials file exist; `-v` shows `smtp.*` with origin |
| 7 | Dry run with the fictional list, then test send `--limit 1 --redirect-to <user's address>` (`local.md`, `../modes/test-send.md`) | agent runs, user checks inbox | mail in inbox, `Authentication-Results` shows spf/dkim/dmarc pass |
| 8 | Configuration set + event destination + SNS + SQS + IAM reader (`aws-ses.md` 5-8) | user, agent provides prompts and policy text | queue shows a message after a simulator bounce |
| 9 | Verify bounces end to end: `--redirect-to bounce@simulator.amazonses.com --limit 1`, then `massmailer bounces --keep` (`aws-ses.md` 9) | agent | `bounces.csv` appears in the test run's folder |
| 10 | Swap the fictional list for the real one; `columns`, `check`, dry run; done | agent | ready for the workflow in `../SKILL.md` |

Steps 4-5 take calendar time (DNS propagation, AWS review); start them
early and do 2-3 and 6 in parallel.

## What to ask the user up front

- Sending domain and sender address (`board@example.org`), display
  name, Reply-To.
- Whether the domain already sends mail from somewhere else (existing
  SPF record to extend, existing DMARC policy to respect).
- AWS: account exists? Preferred region (data location, and SES and the
  queue must be in the same region)? Who has console access?
- Expected list size and per-send volume (for the production access
  request and the daily quota).
- Which mail client most recipients use (for the test send).
- Where configs and lists may live (a repository? then the credentials
  file goes outside it), and where run folders should be written
  (`run.state_dir`; they contain the list).

## What the user has to do himself

- Everything in the AWS console and at the DNS provider. Prepare the
  exact values (record names, record values, policy JSON, prompts for
  the console assistant from `aws-ses.md`) so the user only pastes and
  clicks.
- Create and store credentials: SES SMTP credentials, IAM access keys.
  Prefer that the user writes them into the credentials file directly;
  they should not be pasted into the chat with you unless the user
  decides so. Never put them into a file inside a repository.
- Answer the production access request questions in his own words
  (you may draft).
- Give the explicit go for each real send (`../modes/send.md`).

## What you should do

- Write `account.json`, `mailing.json`, the letterhead skeleton and a
  fictional test list (PC-Freunde-Fulda style, `example.org` addresses
  or the user's own address for the test send).
- Run every `columns`, `check`, `render`, `--dry-run`, test send (with
  the user's okay) and `bounces --keep`.
- Verify each AWS step from the outside where possible: a DNS lookup
  for the CNAME/TXT records, a test send, the queue contents via
  `bounces --keep`.
- Check answers of the console assistant against what the console
  actually shows; menu names and defaults change.
