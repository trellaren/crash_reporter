use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::collector::SystemSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOutputFormat {
    Json,
    PlainText,
}

/// Output destination for log entries.
pub enum LogOutput {
    Stdout,
    File {
        writer: BufWriter<File>,
        format: FileOutputFormat,
    },
    Both {
        writer: BufWriter<File>,
        format: FileOutputFormat,
    },
}

/// Serialise a snapshot to a compact JSON line and write it to the configured output.
///
/// Each call appends exactly one newline-terminated JSON object so the output
/// file is valid [NDJSON](https://ndjson.org/) (newline-delimited JSON).
pub fn write_snapshot(output: &mut LogOutput, snapshot: &SystemSnapshot) -> io::Result<()> {
    match output {
        LogOutput::Stdout => {
            let line = serde_json::to_string(snapshot).map_err(io::Error::other)?;
            println!("{line}");
        }
        LogOutput::File { writer, format } => {
            write_snapshot_to_writer(writer, *format, snapshot)?;
        }
        LogOutput::Both { writer, format } => {
            write_snapshot_to_writer(writer, *format, snapshot)?;
        }
    }

    Ok(())
}

fn write_snapshot_to_writer(
    writer: &mut BufWriter<File>,
    format: FileOutputFormat,
    snapshot: &SystemSnapshot,
) -> io::Result<()> {
    match format {
        FileOutputFormat::Json => {
            let line = serde_json::to_string(snapshot).map_err(io::Error::other)?;
            writeln!(writer, "{line}")?;
            writer.flush()?;
        }
        FileOutputFormat::PlainText => {
            write!(writer, "{}", format_plain_text(snapshot))?;
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
    print!("{}", format_plain_text(snapshot));
}

pub fn file_output_format(path: &Path) -> FileOutputFormat {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());
    match ext.as_deref() {
        Some("txt") | Some("log") => FileOutputFormat::PlainText,
        _ => FileOutputFormat::Json,
    }
}

fn format_plain_text(snapshot: &SystemSnapshot) -> String {
    let mut out = String::new();
    out.push_str("─────────────────────────────────────────────────\n");
    out.push_str(&format!(
        "  Timestamp : {}\n\n",
        snapshot.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
    ));

    out.push_str(&format!(
        "  CPU  : {} ({} cores)\n",
        snapshot.cpu.brand, snapshot.cpu.core_count
    ));
    out.push_str(&format!(
        "         Global usage : {:.1} %\n",
        snapshot.cpu.global_usage_pct
    ));
    out.push_str(&format!(
        "         Frequency    : {} MHz\n",
        snapshot.cpu.frequency_mhz
    ));
    if !snapshot.cpu.per_core_usage_pct.is_empty() {
        let cores: Vec<String> = snapshot
            .cpu
            .per_core_usage_pct
            .iter()
            .enumerate()
            .map(|(i, u)| format!("core{}: {:.1}%", i, u))
            .collect();
        out.push_str(&format!("         Per-core     : {}\n", cores.join("  ")));
    }
    out.push('\n');

    let to_mb = |b: u64| b / 1_048_576;
    out.push_str(&format!(
        "  MEM  : {}/{} MiB used  ({} MiB available)\n",
        to_mb(snapshot.memory.used_bytes),
        to_mb(snapshot.memory.total_bytes),
        to_mb(snapshot.memory.available_bytes),
    ));
    out.push_str(&format!(
        "  SWAP : {}/{} MiB used\n\n",
        to_mb(snapshot.memory.swap_used_bytes),
        to_mb(snapshot.memory.swap_total_bytes),
    ));

    if !snapshot.usb_devices.is_empty() {
        out.push_str("  USB DEVICES:\n");
        for device in &snapshot.usb_devices {
            out.push_str(&format!("    {device}\n"));
        }
        out.push('\n');
    }

    if !snapshot.monitors.is_empty() {
        out.push_str("  MONITORS:\n");
        for monitor in &snapshot.monitors {
            out.push_str(&format!("    {monitor}\n"));
        }
        out.push('\n');
    }

    if !snapshot.thermals.is_empty() {
        out.push_str("  THERMAL:\n");
        for t in &snapshot.thermals {
            let extra = match (t.critical_celsius, t.high_celsius) {
                (Some(c), Some(h)) => format!("  (high: {h:.1}°C  crit: {c:.1}°C)"),
                (Some(c), None) => format!("  (crit: {c:.1}°C)"),
                (None, Some(h)) => format!("  (high: {h:.1}°C)"),
                _ => String::new(),
            };
            out.push_str(&format!(
                "    {:40} {:6.1} °C{}\n",
                t.label, t.temperature_celsius, extra
            ));
        }
        out.push('\n');
    }

    if !snapshot.disks.is_empty() {
        out.push_str("  DISKS:\n");
        for d in &snapshot.disks {
            out.push_str(&format!(
                "    {:30} {:>10} MiB / {:>10} MiB  ({})\n",
                d.mount_point,
                to_mb(d.used_bytes),
                to_mb(d.total_bytes),
                d.file_system,
            ));
        }
        out.push('\n');
    }

    if !snapshot.networks.is_empty() {
        out.push_str("  NETWORK:\n");
        for n in &snapshot.networks {
            out.push_str(&format!(
                "    {:20}  rx: {:>12} B   tx: {:>12} B\n",
                n.interface, n.received_bytes, n.transmitted_bytes,
            ));
        }
        out.push('\n');
    }

    let top: Vec<_> = snapshot.processes.iter().take(10).collect();
    if !top.is_empty() {
        out.push_str("  TOP PROCESSES (by CPU):\n");
        out.push_str(&format!(
            "    {:>7}  {:>7}  {:>12}  NAME\n",
            "PID", "CPU%", "MEM (MiB)"
        ));
        for p in top {
            out.push_str(&format!(
                "    {:>7}  {:>6.1}%  {:>12}  {}\n",
                p.pid,
                p.cpu_usage_pct,
                to_mb(p.memory_bytes),
                p.name,
            ));
        }
        out.push('\n');
    }

    out
}

/// Open (or create) a log file for appending.
pub fn open_log_file(path: &Path) -> io::Result<BufWriter<File>> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok(BufWriter::new(file))
}
