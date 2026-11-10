# Inspect: `columns` and `check`

**When to read this:** first step with any new list, body or config, and
after every fix to one of them.

**What you can do afterwards:** confirm which columns exist, confirm
that every mail of the mailing can be built, and read the run summary.

## `massmailer columns -c mailing.json`

Loads the recipient list and prints the row count and the column names,
nothing else:

```
2 rows; columns:
  to
  cc
  vorname
  nachname
  funktion
  datum
```

No run folder is created (the list is read via a temporary directory
that is removed again). Use it to match `recipients.to_columns`,
`recipients.skip_column`, `message.attachment_column` and the
placeholders in body, subject and letterhead against the real headers.
Only `recipients.*` and `fetch.*` settings matter here.

## `massmailer check -c mailing.json`

Does everything a send does except connecting to SMTP and writing the
log: loads body, letterhead, stylesheets, images and attachments,
reads the list, applies skip column and limit, renders and builds
**every** mail and checks its size.

Output (stderr):

```
run folder: massmailer-runs/20261110-155012-check
warning: 1 image(s) without alt text          # each distinct warning once
2 mail(s) prepared from 2 row(s), 1 skipped
```

It stops at the first error, e.g. `error: recipient list line 2: to
column `mail` not in recipient list`, with exit code 1. See
`../troubleshooting.md` for the messages.

What it writes: a run folder `<run.state_dir>/<timestamp>-check/` with
`inputs/` (copies of every loaded file), `manifest.json` (their
SHA-256) and `mails/NNNN.eml` for every built mail. No `log.jsonl`.
These folders are safe to delete.

`check` does **not** validate `smtp.*`; only a real send connects. Add
`-v` to see every effective setting with its origin (secrets masked).

Settings for `check`: everything except `smtp.*`; see
`../setup/configuration.md`.

## Typical loop

```sh
massmailer columns -c mailing.json
massmailer check   -c mailing.json        # fix, repeat until clean
massmailer check   -c mailing.json --set sending.limit=3   # quick iteration on a big list
```

With `--set sending.limit=N` the check builds only the first N mails,
which is faster while the body is still being edited; run a full
`check` once before the dry run.
