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
        /// After sending, wait this many minutes and then fetch bounces (needs aws.* settings)
        #[arg(long, value_name = "MINUTES")]
        bounces_after: Option<f64>,
    },
    /// Fetch bounce, complaint and delivery events from the SQS queue and record them in the run folders
    Bounces {
        /// Leave the messages in the queue (for testing)
        #[arg(long)]
        keep: bool,
        /// Long-poll seconds per request (1-20)
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=20))]
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
            bounces_after,
        } => {
            if let Some(m) = bounces_after {
                overrides.push(("sending.bounces_after_minutes".into(), m.to_string()));
            }
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
    if cli.verbose {
        for l in settings.describe() {
            eprintln!("  {l}");
        }
    }
    if let Command::Columns = cli.command {
        return run::columns(&settings);
    }
    if let Command::Bounces { keep, wait, .. } = cli.command {
        return bounces::execute(&settings, &bounces::Options { keep, wait });
    }
    let bounces_after = match mode {
        Mode::Send => settings.num("sending.bounces_after_minutes")?,
        _ => None,
    };
    if let Some(m) = bounces_after {
        if !(0.0..=1440.0).contains(&m) {
            anyhow::bail!("sending.bounces_after_minutes must be between 0 and 1440");
        }
        // fail before sending, not after the wait
        bounces::check_settings(&settings)
            .map_err(|e| anyhow::anyhow!("--bounces-after needs the bounces settings: {e:#}"))?;
    }
    let result = run::execute(
        &settings,
        &Options {
            mode,
            yes,
            verbose: cli.verbose,
        },
    );
    if let Some(m) = bounces_after {
        // also after partly failed sends; nothing to wait for if sending was aborted early
        if result.is_ok()
            || result
                .as_ref()
                .is_err_and(|e| e.to_string().ends_with("mail(s) failed"))
        {
            wait_minutes(m);
            bounces::execute(
                &settings,
                &bounces::Options {
                    keep: false,
                    wait: 5,
                },
            )?;
        }
    }
    result
}

/// Waits, printing the remaining time once a minute (Ctrl-C ends the wait;
/// the send is already logged completely at this point).
fn wait_minutes(minutes: f64) {
    let total = std::time::Duration::from_secs_f64(minutes * 60.0);
    let start = std::time::Instant::now();
    eprintln!("waiting {minutes} minute(s) before fetching bounces (Ctrl-C ends the wait)");
    while let Some(rest) = total.checked_sub(start.elapsed()) {
        if rest.is_zero() {
            break;
        }
        let step = rest.min(std::time::Duration::from_secs(60));
        std::thread::sleep(step);
        let left = total.saturating_sub(start.elapsed()).as_secs();
        if left > 0 {
            eprintln!("  {}:{:02} left", left / 60, left % 60);
        }
    }
}
