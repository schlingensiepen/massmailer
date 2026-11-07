# massmailer

A command-line tool for sending personalised bulk email from a
Markdown template and a recipient list (CSV, XLSX, ODS or JSON
from a URL). Each message is rendered into a letterhead (HTML
and plain-text frame), sent via SMTP at a limited rate, and
recorded in a delivery log; bounces can be collected afterwards.

It is meant for formal notices to members of an association
(e.g. invitations to a general meeting), not for newsletters.

**Status:** in development; no release yet.

## Planned commands

- `check` — validate settings, recipients, template and attachments
- `render` — write `.eml` and HTML previews
- `send` — send (with `--dry-run` for a full run without delivery)
- `bounces` — collect bounces and complaints (Amazon SES via SQS)

Settings come from one or more JSON config files, environment
variables prefixed `MASSMAILER_`, and command-line options
(options override environment, environment overrides config).

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
