# Local setup

**When to read this:** setting up massmailer on the user's machine for
the first time, or creating the folder for a new mailing.

**What you can do afterwards:** install the CLI, lay out a mailing
folder, keep credentials in a safe place, and run the first tests up
to one real test mail.

## 1. Install the CLI

See `install.md`. Checkpoint: `massmailer --version`.

## 2. Folder layout

Keep per-mailing content together and credentials elsewhere. A layout
that works, with a repository for the mailing material and a sibling
folder outside it for secrets:

```
~/mailings/                        (may be a git repository)
  account.json                     SMTP host/port/security, rate, state dir, ses/aws names
  2027-annual-meeting/
    mailing.json                   sender, body, subject, letterhead, list, columns
    message.md
    letterhead/
      layout.html
      layout.txt
      style.css
      logo.png
    members.csv                    the real list; add to .gitignore if the folder is a repo
    test-list.csv                  2-3 fictional rows, example.org addresses
  runs/                            run.state_dir; contains lists and mails, .gitignore it

~/mailings-secrets/                OUTSIDE every repository, mode 700
  smtp.json                        mode 600: {"smtp": {"username": "…", "password": "…"}}
  aws.json                         mode 600: {"aws": {"access_key_id": "…", "secret_access_key": "…"}}
```

Rules:

- Relative paths in a config file resolve against that file, so
  `mailing.json` can say `"body": "message.md"`,
  `"letterhead": {"dir": "letterhead"}`, `"source": "members.csv"`
  and `account.json` can say `"run": {"state_dir": "runs"}`.
- Settings reference: `configuration.md`. Example of both files:
  `examples/pc-freunde-fulda/` in the repository.
- The real list and the run folders hold personal data; they do not
  belong in a repository. Run folders are the record of what was sent;
  keep `-send` folders with the mailing's other records.

## 3. Credentials

Three acceptable sources, in order of preference; nothing else:

1. **Separate config file outside any repository**, passed as the
   **last** `-c` so it wins:

   ```sh
   mkdir -m 700 ~/mailings-secrets
   # the user creates the files and sets permissions:
   chmod 600 ~/mailings-secrets/smtp.json ~/mailings-secrets/aws.json
   massmailer send -c account.json -c 2027-annual-meeting/mailing.json -c ~/mailings-secrets/smtp.json
   ```

   On Windows, a folder under the user's profile with default
   permissions (only the user and administrators) serves the same
   purpose.

2. **Environment variables** in the shell that runs the command:
   `MASSMAILER_SMTP_USERNAME`, `MASSMAILER_SMTP_PASSWORD`,
   `MASSMAILER_AWS_ACCESS_KEY_ID`, `MASSMAILER_AWS_SECRET_ACCESS_KEY`.
   Set them in a way that does not land in a committed file or in
   shell history (`read -s`, a secret manager, the user's profile).

3. **CLI flags typed by the user** (`--smtp-username`,
   `--smtp-password`, `--aws-access-key-id`, `--aws-secret-access-key`).
   They are visible in the process list while the command runs and
   tend to end up in shell history; use them only for a one-off and
   prefer 1 or 2.

The user should write the values into the file himself. If the user
pastes a secret into the chat anyway, do not repeat it, do not put it
anywhere but the credentials file, and suggest rotating it. `-v` shows
`smtp.username = ***  (config …/smtp.json)`, which is enough to confirm
the source without revealing anything.

## 4. First tests, in this order

With `test-list.csv` as `recipients.source` (fictional rows; for the
test send the address does not matter because of `--redirect-to`):

```sh
cd ~/mailings
massmailer columns -c 2027-annual-meeting/mailing.json
massmailer check   -c 2027-annual-meeting/mailing.json
massmailer render  -c 2027-annual-meeting/mailing.json          # user looks at runs/<…>-render/mails/
massmailer send --dry-run -c account.json -c 2027-annual-meeting/mailing.json
```

When SMTP access exists (`aws-ses.md` step 4) and the user agrees to a
test mail to his own address:

```sh
massmailer send --limit 1 --redirect-to me@example.net \
  -c account.json -c 2027-annual-meeting/mailing.json -c ~/mailings-secrets/smtp.json
```

What to check in the received mail: `../modes/test-send.md`. While the
SES account is in the sandbox, `me@example.net` must be a verified
identity, otherwise the send fails with `554 Message rejected: Email
address is not verified`.

Then the provider's sink addresses, which exercise the bounce pipeline
without a real mailbox (`../providers/amazon-ses.md`, "Mailbox
simulator"):

```sh
massmailer send --limit 1 --redirect-to success@simulator.amazonses.com -c … -c ~/mailings-secrets/smtp.json
massmailer send --limit 1 --redirect-to bounce@simulator.amazonses.com  -c … -c ~/mailings-secrets/smtp.json
massmailer bounces --keep -c account.json -c ~/mailings-secrets/aws.json
```

Checkpoint: the bounce test run's folder contains `bounces.csv` with
one `Permanent` row; the `success@` run has a `Delivery` event in its
`events.jsonl` and no `bounces.csv`. A later `bounces` without
`--keep` clears the queue and writes nothing twice (events are
de-duplicated by id); after `--keep` wait for the queue's visibility
timeout (default 30 s) before that run, or it sees an empty queue.
Then switch `recipients.source` to the real list,
run `columns`, `check` and `--dry-run` again, and hand over to the
workflow in `../SKILL.md`.
