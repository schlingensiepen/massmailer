# Body, letterhead, subject and attachments

**When to read this:** before writing or changing the body file, the
letterhead, the subject, a preheader or attachments, or when `render`
output does not look as intended.

**What you can do afterwards:** write a body in Markdown, HTML or text
with placeholders that are escaped correctly, build a letterhead,
control CSS and images, add attachments and stay within the size limit.

## Body formats

`message.body` is a file or URL; the format comes from the extension
(`.md`/`.markdown`, `.html`/`.htm`, `.txt`/`.text`) or from
`message.format`. CRLF line endings are normalised, a BOM is removed,
the file must be UTF-8.

| Format | How it becomes HTML | Placeholder escaping |
|---|---|---|
| Markdown | CommonMark via comrak with autolinks, strikethrough and tables; **raw HTML inside Markdown is not rendered** (it is omitted). A trailing backslash makes a hard line break. | every ASCII punctuation character in a value is backslash-escaped and line breaks become spaces, so a value can never inject Markdown or HTML (`*Stern*` stays literal) |
| HTML | relative `src`/`href` are made absolute against the body's location; with a letterhead the `<body>` content is extracted and `<style>`/`<link rel="stylesheet">` from `<head>` are carried into the letterhead's head; without a letterhead the document is sent as it is (after post-processing) | `&`, `<`, `>`, `"`, `'` are HTML-escaped |
| Text | paragraphs on blank lines, line breaks become `<br>`, URLs and mail addresses become links; without a letterhead a text body is sent as **plain text only** (no HTML part) | none |

Pipeline: placeholders → (Markdown/text → HTML) → letterhead →
post-processing → MIME.

## Placeholders (MiniJinja)

Syntax: `{{ field }}`, `{% if field %}…{% endif %}`, `{% for %}`,
filters (`{{ name | upper }}`). Field names are the column headers.

- An **undefined field is an error** (`body: placeholder …`), so a typo
  fails in `check`, not in the inbox. Empty cells are defined and
  empty: `{% if funktion %}` is the idiom for optional text.
- Values are escaped as in the table above. `{{ value | safe }}`
  disables escaping for that value; use it only for values you control.
- Placeholders are also filled in the **subject** (no escaping; control
  characters are an error), the **preheader** (HTML-escaped after
  filling), `message.text` (no escaping) and in the **letterhead**
  (HTML-escaped in `layout.html`, none in `layout.txt`).
- `message.placeholders=false` sends the body and `message.text`
  literally; the subject is still a template.

## Letterhead

`letterhead.dir` is a folder or URL with:

- `layout.html` (required): a complete HTML document; `{{ content }}`
  marks where the rendered body goes (inserted unescaped). Relative
  `src`/`href` resolve against the letterhead folder, so `<img
  src="logo.png">` and `<link rel="stylesheet" href="style.css">` work.
- `layout.txt` (optional; for a URL letterhead it must exist): plain
  text frame with `{{ content }}` for the text part.

Both may use row placeholders. A frame without `{{ content }}` is an
error. The example `examples/pc-freunde-fulda/letterhead/` is a
working table-based layout with logo, stylesheet and footer.

## Text part

Every mail has a plain-text part. Order of preference:

1. `message.text` (own file, placeholders filled, no escaping),
2. a text body itself,
3. derived from the HTML with `html2text` at `message.text_width`
   (default 78); headings are underlined with `=`/`-`.

The result is wrapped in `layout.txt` if present. `message.parts=text`
sends only this part, no HTML.

## HTML post-processing

Applied to the final HTML document of every mail:

- **Removed**, with one `warning: removed from HTML: …` per run:
  `<script>`, `<iframe>`, `<object>`, `<embed>`, `<form>`, `<base>`,
  all `on*` attributes, `javascript:` links.
- **CSS**: `<link rel="stylesheet">` files are loaded once per run
  (local or URL) and the link removed; `@import` and `@font-face` rules
  are dropped (`warning: web-font rules removed from …`); all CSS is
  inlined into `style` attributes; `<style>` blocks are kept for
  `@media` and dark-mode rules. Linked stylesheets are only loaded in
  `message.images=embed` mode; in `link` mode the `<link>` stays.
- **Tables** get `border="0" cellpadding="0" cellspacing="0"` unless
  set (Outlook).
- **Images** (`message.images=embed`, the default): every `<img src>`
  (file, URL or `data:` URI) is loaded once per run, attached as an
  inline `cid:` part and `width`/`height` are set to the display size:
  from the attributes if present (one of them scales proportionally),
  otherwise the intrinsic size. `srcset`, `sizes` and `<picture>`
  sources are removed. Supported: PNG, JPEG, GIF, WebP; **SVG is
  rasterised to PNG** at twice the display size, so give SVGs a
  `width` or `height`. Any other type is an error. `<img>` without
  `alt` → `warning: N image(s) without alt text`.
  `message.images=link` leaves `src` alone: then every image needs an
  absolute `https://` URL that recipients can reach.
- **Preheader**: `message.preheader` is inserted as a hidden `<div>` at
  the start of `<body>`; mail clients show it as the preview text.
- **Size warning**: an HTML part above about 102 KB triggers `warning:
  HTML part is N KB; Gmail clips …`.

## Attachments

- `message.attachments`: one path/URL or a list; attached to every mail.
- `message.attachment_column`: a column whose cell names one file per
  row (empty cell = none); relative names are joined with
  `message.attachment_dir` if set, otherwise resolved against the
  working directory; absolute paths and URLs are used as they are. A
  missing column is an error.
- The attachment file name is the last path segment; the MIME type is
  guessed from the extension (pdf, png, jpg/jpeg, gif, txt, csv, ics,
  docx, xlsx, odt, zip; anything else `application/octet-stream`).
- Each attachment's SHA-256 is recorded per mail in `log.jsonl`.

## Size limit

After encoding, a mail larger than `message.max_mail_size_mb` (default
5, maximum 40) aborts the whole run before anything is sent (`line N:
mail is X MB, above message.max_mail_size_mb`). Embedded images and
attachments count with base64 overhead (about +37 %). Input files
larger than `fetch.max_size_mb` (default 20) are refused when loading.

## Headers massmailer sets

`From` (`sender.name` + `sender.address`; with `sender.on_behalf_of`:
`From` = that address, `Sender` = `sender.address`), `Reply-To`,
`To`/`Cc`/`Bcc`, `Subject`, `Message-ID` as `<random@sender-domain>`,
`Auto-Submitted: auto-generated`, `X-SES-CONFIGURATION-SET` (if
`ses.configuration_set`), `X-SES-MESSAGE-TAGS:
massmailer-run=<run folder name>, massmailer-mail=<NNNN>` (from the
release after 0.1.0; used by `bounces`), and on a redirected test send
`X-Massmailer-Original-Recipients`.

MIME structure: text only; `multipart/alternative` (text, HTML);
`multipart/alternative` (text, `multipart/related` (HTML, images));
with attachments all of that inside `multipart/mixed`.
