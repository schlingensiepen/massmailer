//! massmailer — Markdown, HTML or text body plus recipient list → mails in a
//! letterhead → SMTP (e.g. Amazon SES).

pub mod bounces;
pub mod config;
pub mod fetch;
pub mod mail;
pub mod recipients;
pub mod render;
pub mod run;
