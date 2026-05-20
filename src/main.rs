use crash_reporter::collector::{collect_snapshot, init_system, refresh_system};
use crash_reporter::logger::{
    file_output_format, open_log_file, print_pretty, write_snapshot, LogOutput,
};

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use clap::Parser;

/// crash_reporter — log running applications and system metrics at a regular interval.
///
/// Metrics captured each tick:
///   • Running processes (name, PID, CPU %, RAM)
///   • CPU usage (global and per-core), brand, frequency
///   • Memory / swap usage
///   • Thermal sensors (temperatures, high/critical thresholds)
///   • Disk usage
///   • Network interface counters
///
/// Output is written as newline-delimited JSON (NDJSON), except files ending in
/// `.log` or `.txt`, which receive plain-text summaries.
/// Pass --pretty to also print a human-readable summary to the terminal.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Sampling interval in seconds (default: 10)
    #[arg(short, long, default_value_t = 10, value_name = "SECS")]
    interval: u64,

    /// Write log entries to this file (in addition to stdout when --pretty is set)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Print a human-readable summary to stdout (structured JSON is still
    /// written to the output file when --output is given)
    #[arg(short, long)]
    pretty: bool,

    /// Stop after collecting this many samples (0 = run forever)
    #[arg(short = 'n', long, default_value_t = 0, value_name = "COUNT")]
    count: u64,
}

fn main() {
    let args = Args::parse();

    if args.interval == 0 {
        eprintln!("error: --interval must be at least 1 second");
        std::process::exit(1);
    }

    // Build the output sink.
    let mut log_output = match &args.output {
        Some(path) => match open_log_file(path) {
            Ok(writer) => {
                let format = file_output_format(path);
                if args.pretty {
                    LogOutput::Both { writer, format }
                } else {
                    LogOutput::File { writer, format }
                }
            }
            Err(e) => {
                eprintln!("error: cannot open output file '{}': {e}", path.display());
                std::process::exit(1);
            }
        },
        None => LogOutput::Stdout,
    };

    // Initialise sysinfo.
    let mut sys = init_system();

    eprintln!(
        "crash_reporter: sampling every {} second(s){}",
        args.interval,
        if args.count > 0 {
            format!(", {} sample(s) total", args.count)
        } else {
            String::from(", press Ctrl-C to stop")
        }
    );

    let mut collected: u64 = 0;

    loop {
        // Refresh all subsystems before collecting.
        refresh_system(&mut sys);

        let snapshot = collect_snapshot(&sys);

        if args.pretty {
            print_pretty(&snapshot);
        }

        if let Err(e) = write_snapshot(&mut log_output, &snapshot) {
            eprintln!("error: failed to write log entry: {e}");
        }

        collected += 1;
        if args.count > 0 && collected >= args.count {
            break;
        }

        thread::sleep(Duration::from_secs(args.interval));
    }
}
