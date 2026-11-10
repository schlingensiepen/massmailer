# Test send: `--limit` and `--redirect-to`

**When to read this:** before the first real send of a mailing; after
the dry run is clean.

**What you can do afterwards:** deliver one or a few real mails to the
user's own inbox over the real SMTP path, recognise them, and know what
to check before the real send.

## Command

```sh
massmailer send --limit 1 --redirect-to me@example.net -c account.json -c mailing.json
```

- `--redirect-to ADDR` replaces **all** recipients of every mail
  (To, Cc, Bcc from the columns and `message.to`/`cc`/`bcc`) with this
  single `To`. Nobody on the list receives anything.
- `--limit N` builds only the first N non-skipped rows. `--limit 1` is
  the normal first test; a handful (`--limit 3`) when rows differ a lot
  (optional blocks, attachments per row).
- Both are also settings (`sending.limit`, `sending.redirect_to`), so
  make sure neither is accidentally left in a config file before the
  real send. `-v` shows where each comes from.
- The SMTP connection and login happen for real: credentials must be in
  the environment or passed as flags (`../setup/configuration.md`,
  "Secrets").
- The confirmation prompt appears as in a real send (`Send 1 mail(s)
  now? [y/N]`); the user answers it, or you pass `--yes` after the user
  asked for the test send.

## How the test mail looks

- Subject: `[TEST an anna@example.org] <original subject>`; with
  several original recipients they are listed comma-separated. The
  prefix is fixed (German "an" = "to").
- `To`: the redirect address only. No `Cc`/`Bcc`.
- Header `X-Massmailer-Original-Recipients: anna@example.org, …` lists
  who would have received this mail.
- Body, letterhead, images and attachments are exactly those of the
  real mail for that row.

The summary line says `… prepared from M row(s), redirected`, and the
run folder is a normal `-send` folder with `log.jsonl` (`status:
"sent"`, `to` = redirect address).

## What to check in the inbox

Ask the user to open the mail in the client most recipients use and
confirm:

1. It arrived in the inbox, not in spam. If it is in spam, stop and
   look at authentication (SPF, DKIM, DMARC for the sender domain) and
   the provider's reputation before any real send.
2. `From` name and address, `Reply-To`; whether replying goes to the
   intended mailbox.
3. Subject without the test prefix reads correctly, including the
   placeholder values.
4. Letterhead and logo show; images not blocked or broken; links work.
5. Placeholders in the body are filled and look like plain text (no
   stray backslashes, no `&amp;`).
6. The plain-text view (or a text-only client) is readable.
7. Attachments open.
8. On a phone: layout and font size.

Send one more test with a different row if the body has conditional
blocks (`--limit 2` or a trimmed copy of the list).

## Provider test addresses

Some providers offer sink addresses that accept, bounce or complain
without involving a real mailbox; with `--redirect-to <sink> --limit 1`
they let you test the bounce pipeline end to end. For Amazon SES see
`../providers/amazon-ses.md`.

## After the test

Only after the user says the test mail is right, continue with
`send.md`. If anything was changed after the test (body, letterhead,
list, settings), repeat dry run and test send.
