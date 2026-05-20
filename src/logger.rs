use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::collector::SystemSnapshot;

/// Output destination for log entries.
pub enum LogOutput {
    Stdout,
    File(BufWriter<File>),
    Both(BufWriter<File>),
}

/// Serialise a snapshot to a compact JSON line and write it to the configured output.
///
/// Each call appends exactly one newline-terminated JSON object so the output
/// file is valid [NDJSON](https://ndjson.org/) (newline-delimited JSON).
pub fn write_snapshot(output: &mut LogOutput, snapshot: &SystemSnapshot) -> io::Result<()> {
    let line = serde_json::to_string(snapshot)
        .map_err(io::Error::other)?;

    match output {
        LogOutput::Stdout => {
            println!("{line}");
        }
        LogOutput::File(writer) => {
            writeln!(writer, "{line}")?;
            writer.flush()?;
        }
        LogOutput::Both(writer) => {
            writeln!(writer, "{line}")?;
            writer.flush()?;
        }
    }

    Ok(())
}

/// Print a human-readable summary of a snapshot to stdout.
///
/// Used when `--pretty` is passed so operators can read the output directly
/// without piping through a JSON pretty-printer.
pub fn print_pretty(snapshot: &SystemSnapshot) {
    println!("─────────────────────────────────────────────────");
    println!("  Timestamp : {}", snapshot.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
    println!();

    // CPU
    println!("  CPU  : {} ({} cores)", snapshot.cpu.brand, snapshot.cpu.core_count);
    println!("         Global usage : {:.1} %", snapshot.cpu.global_usage_pct);
    println!("         Frequency    : {} MHz", snapshot.cpu.frequency_mhz);
    if !snapshot.cpu.per_core_usage_pct.is_empty() {
        let cores: Vec<String> = snapshot
            .cpu
            .per_core_usage_pct
            .iter()
            .enumerate()
            .map(|(i, u)| format!("core{}: {:.1}%", i, u))
            .collect();
        println!("         Per-core     : {}", cores.join("  "));
    }
    println!();

    // Memory
    let to_mb = |b: u64| b / 1_048_576;
    println!("  MEM  : {}/{} MiB used  ({} MiB available)",
        to_mb(snapshot.memory.used_bytes),
        to_mb(snapshot.memory.total_bytes),
        to_mb(snapshot.memory.available_bytes),
    );
    println!("  SWAP : {}/{} MiB used",
        to_mb(snapshot.memory.swap_used_bytes),
        to_mb(snapshot.memory.swap_total_bytes),
    );
    println!();

    // Thermals
    if !snapshot.thermals.is_empty() {
        println!("  THERMAL:");
        for t in &snapshot.thermals {
            let extra = match (t.critical_celsius, t.high_celsius) {
                (Some(c), Some(h)) => format!("  (high: {h:.1}°C  crit: {c:.1}°C)"),
                (Some(c), None)    => format!("  (crit: {c:.1}°C)"),
                (None, Some(h))    => format!("  (high: {h:.1}°C)"),
                _                  => String::new(),
            };
            println!("    {:40} {:6.1} °C{}", t.label, t.temperature_celsius, extra);
        }
        println!();
    }

    // Disks
    if !snapshot.disks.is_empty() {
        println!("  DISKS:");
        for d in &snapshot.disks {
            println!("    {:30} {:>10} MiB / {:>10} MiB  ({})",
                d.mount_point,
                to_mb(d.used_bytes),
                to_mb(d.total_bytes),
                d.file_system,
            );
        }
        println!();
    }

    // Network
    if !snapshot.networks.is_empty() {
        println!("  NETWORK:");
        for n in &snapshot.networks {
            println!("    {:20}  rx: {:>12} B   tx: {:>12} B",
                n.interface,
                n.received_bytes,
                n.transmitted_bytes,
            );
        }
        println!();
    }

    // Top processes (top 10 by CPU)
    let top: Vec<_> = snapshot.processes.iter().take(10).collect();
    if !top.is_empty() {
        println!("  TOP PROCESSES (by CPU):");
        println!("    {:>7}  {:>7}  {:>12}  NAME",
            "PID", "CPU%", "MEM (MiB)");
        for p in top {
            println!("    {:>7}  {:>6.1}%  {:>12}  {}",
                p.pid,
                p.cpu_usage_pct,
                to_mb(p.memory_bytes),
                p.name,
            );
        }
        println!();
    }
}

/// Open (or create) a log file for appending.
pub fn open_log_file(path: &Path) -> io::Result<BufWriter<File>> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok(BufWriter::new(file))
}
