
use clap::Parser;
use mp4lint::{ValidationOptions, Validator};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "mp4lint", version, about)]
struct Args {
    file: PathBuf,

    #[arg(short, long)]
    lenient: bool,

    #[arg(short, long, value_name = "FMT", default_value = "text")]
    format: OutputFormat,

    #[arg(short, long)]
    quiet: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let file = match File::open(&args.file) {
        Ok(f) => f,
        Err(e) => {
            if !args.quiet {
                eprintln!("Error opening file: {}", e);
            }
            return ExitCode::from(2);
        }
    };

    let reader = BufReader::new(file);

    let options = ValidationOptions::new()
        .lenient(args.lenient);

    let validator = Validator::new();
    let report = match validator.validate_with_options(reader, options) {
        Ok(r) => r,
        Err(e) => {
            if !args.quiet {
                eprintln!("Error validating file: {e}");
            }
            return ExitCode::from(2);
        }
    };

    if !args.quiet {
        match args.format {
            OutputFormat::Text => {
                print!("{}", report.format_text());
            }
            OutputFormat::Json => match report.format_json() {
                Ok(json) => println!("{}", json),
                Err(e) => {
                    eprintln!("Error formatting JSON: {}", e);
                    return ExitCode::from(2);
                }
            },
        }
    }

    if report.has_errors() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
