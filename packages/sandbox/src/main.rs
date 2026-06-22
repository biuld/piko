mod policy;
mod runner;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use policy::{Access, Policy};

#[derive(Parser)]
#[command(name = "piko-sandbox", version)]
struct Cli {
    #[arg(long, value_name = "FILE")]
    policy: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Check {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    Exec {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    CheckPath {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        access: String,
        #[arg(long)]
        allow_missing: bool,
    },
    Read {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        path: PathBuf,
    },
    Write {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        path: PathBuf,
    },
    Canonicalize {
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
}

fn command_text(parts: &[String]) -> String {
    if parts.len() == 1 {
        parts[0].clone()
    } else {
        shell_words::join(parts)
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("piko-sandbox: {error}");
            std::process::exit(126);
        }
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let policy = Policy::load(&cli.policy)?;
    match cli.command {
        Command::Check { cwd, command } => {
            policy.validate_command(&command_text(&command), &cwd)?;
            Ok(0)
        }
        Command::Exec { cwd, command } => {
            let text = command_text(&command);
            policy.validate_command(&text, &cwd)?;
            runner::exec(&policy, &cwd, &text)
        }
        Command::CheckPath {
            cwd,
            path,
            access,
            allow_missing,
        } => {
            let access = match access.as_str() {
                "read" => Access::Read,
                "write" => Access::Write,
                _ => return Err("access must be read or write".into()),
            };
            policy.authorize(&cwd, &path, access, !allow_missing)?;
            Ok(0)
        }
        Command::Read { cwd, path } => {
            let path = policy.authorize(&cwd, &path, Access::Read, true)?;
            std::io::copy(&mut std::fs::File::open(path)?, &mut std::io::stdout())?;
            Ok(0)
        }
        Command::Write { cwd, path } => {
            let path = policy.authorize(&cwd, &path, Access::Write, false)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut std::io::stdin(), &mut std::fs::File::create(path)?)?;
            Ok(0)
        }
        Command::Canonicalize { command } => {
            let text = command_text(&command);
            let canonical = policy.canonicalize_command(&text)?;
            println!("{}", canonical);
            Ok(0)
        }
    }
}
