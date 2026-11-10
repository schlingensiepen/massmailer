use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use massmailer::bounces;
use massmailer::config::{Settings, parse_set};
use massmailer::run::{self, Mode, Options};

/// Mass mailer: Markdown, HTML or text body plus recipient list → mails in a letterhead → SMTP.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Config file (JSON); may be given several times, the later one wins
    #[arg(short, long = "config", global = true)]
    configs: Vec<PathBuf>,
    /// Set any setting, e.g. --set sending.limit=5 (wins over config and environment)
    #[arg(long = "set", global = true, value_name = "KEY=VALUE")]
    sets: Vec<String>,
    /// SMTP user name (or MASSMAILER_SMTP_USERNAME)
    #[arg(long, global = true)]
    smtp_username: Option<String>,
    /// SMTP password (or MASSMAILER_SMTP_PASSWORD)
    #[arg(long, global = true)]
    smtp_password: Option<String>,
    /// Show every setting with its origin (secrets masked)
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check settings, recipient list and every mail without sending
    Check,
    /// List the column names of the recipient list
    Columns,
    /// Write every mail as .eml, .html and .txt into a run folder
    Render,
    /// Send the mails
    Send {
        /// Do everything except the SMTP delivery
        #[arg(long)]
        dry_run: bool,
        /// Only the first N mails
        #[arg(long)]
        limit: Option<usize>,
        /// Deliver every mail to this address instead (test)
        #[arg(long)]
        redirect_to: Option<String>,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Fetch bounce, complaint and delivery events from the SQS queue and record them in the run folders
    Bounces {
        /// Leave the messages in the queue (for testing)
        #[arg(long)]
        keep: bool,
        /// Long-poll seconds per request (0-20)
        #[arg(long, default_value_t = 5)]
        wait: u32,
        /// AWS access key id (or MASSMAILER_AWS_ACCESS_KEY_ID)
        #[arg(long)]
        aws_access_key_id: Option<String>,
        /// AWS secret access key (or MASSMAILER_AWS_SECRET_ACCESS_KEY)
        #[arg(long)]
        aws_secret_access_key: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match real_main(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn real_main(cli: Cli) -> anyhow::Result<()> {
    let mut overrides = cli
        .sets
        .iter()
        .map(|s| parse_set(s))
        .collect::<anyhow::Result<Vec<_>>>()?;
    if let Some(u) = cli.smtp_username {
        overrides.push(("smtp.username".into(), u));
    }
    if let Some(p) = cli.smtp_password {
        overrides.push(("smtp.password".into(), p));
    }
    if let Command::Bounces {
        aws_access_key_id: Some(k),
        ..
    } = &cli.command
    {
        overrides.push(("aws.access_key_id".into(), k.clone()));
    }
    if let Command::Bounces {
        aws_secret_access_key: Some(k),
        ..
    } = &cli.command
    {
        overrides.push(("aws.secret_access_key".into(), k.clone()));
    }
    let (mode, yes) = match &cli.command {
        Command::Bounces { .. } => (Mode::Check, false),
        Command::Check => (Mode::Check, false),
        Command::Columns => (Mode::Check, false),
        Command::Render => (Mode::Render, false),
        Command::Send {
            dry_run,
            limit,
            redirect_to,
            yes,
        } => {
            if let Some(l) = limit {
                overrides.push(("sending.limit".into(), l.to_string()));
            }
            if let Some(r) = redirect_to {
                overrides.push(("sending.redirect_to".into(), r.clone()));
            }
            (if *dry_run { Mode::DryRun } else { Mode::Send }, *yes)
        }
    };
    if cli.configs.is_empty() {
        anyhow::bail!("no --config given");
    }
    let settings = Settings::load(&cli.configs, &overrides)?;
    if let Command::Columns = cli.command {
        return run::columns(&settings);
    }
    if let Command::Bounces { keep, wait, .. } = cli.command {
        return bounces::execute(&settings, &bounces::Options { keep, wait });
    }
    run::execute(
        &settings,
        &Options {
            mode,
            yes,
            verbose: cli.verbose,
        },
    )
}
