# Render only

**When to read this:** the user wants to see the mails as they will
arrive, or you need to inspect the HTML, the text part or the raw
message of one row without sending anything.

**What you can do afterwards:** produce `.eml`, `.html` and `.txt` for
every mail, open them, and point the user to the right file.

## Command

```sh
massmailer render -c mailing.json
massmailer render -c mailing.json --set sending.limit=2    # only the first two
```

No SMTP settings are needed or read.

## What it writes

A run folder `<run.state_dir>/<timestamp>-render/` with:

- `inputs/` and `manifest.json`: copies and SHA-256 of every input.
- `mails/0001.eml`, `0002.eml`, …: the complete MIME message, exactly
  what `send` would hand to the SMTP server, with all headers, inline
  images and attachments. Open with a mail client or read the headers
  with a text editor.
- `mails/0001.html`: the final HTML part after letterhead, CSS inlining
  and image handling. Image `src` attributes are `cid:imgN` here, so a
  browser shows them as broken; that is expected. For a browser
  preview, run once with `--set message.images=link` (then local image
  paths stay as file paths).
- `mails/0001.txt`: the plain-text part, including `layout.txt`.

Numbering follows the order of rows after skipping and limiting; the
`line` of each mail is only in `log.jsonl`, which `render` does not
write. To map mail number to row: mail 0001 is the first non-skipped
row, and so on.

## What to look at

- Subject and `From`/`To`/`Cc` in the `.eml` header block.
- Placeholder values: literal, correctly escaped, no leftover `{{`.
- Letterhead, logo, links in the `.html`.
- Readability of the `.txt` (headings underlined, links visible).
- Warnings printed during the run (`removed from HTML`, `web-font rules
  removed`, `image(s) without alt text`, `Gmail clips`).

A `render` run on the real list writes personal data into the run
folder; keep the folder where the user keeps the list and do not paste
mail contents into chat unless asked.
