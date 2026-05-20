use crash_reporter::collector::{collect_snapshot, init_system, refresh_system};
use crash_reporter::logger::{
    file_output_format, open_log_file, write_snapshot, FileOutputFormat, LogOutput,
};
use tempfile::NamedTempFile;

// ── Collector tests ───────────────────────────────────────────────────────────

#[test]
fn snapshot_has_cpu_info() {
    let mut sys = init_system();
    refresh_system(&mut sys);
    let snap = collect_snapshot(&sys);

    assert!(snap.cpu.core_count > 0, "at least one CPU core expected");
    assert!(
        snap.cpu.global_usage_pct >= 0.0 && snap.cpu.global_usage_pct <= 100.0,
        "global CPU usage must be 0–100 %"
    );
    assert_eq!(
        snap.cpu.per_core_usage_pct.len(),
        snap.cpu.core_count,
        "per-core vec length must equal core_count"
    );
}

#[test]
fn snapshot_memory_is_consistent() {
    let mut sys = init_system();
    refresh_system(&mut sys);
    let snap = collect_snapshot(&sys);

    assert!(snap.memory.total_bytes > 0, "total memory must be non-zero");
    assert!(
        snap.memory.used_bytes <= snap.memory.total_bytes,
        "used memory must not exceed total"
    );
    assert!(
        snap.memory.available_bytes <= snap.memory.total_bytes,
        "available memory must not exceed total"
    );
}

#[test]
fn snapshot_has_processes() {
    let mut sys = init_system();
    refresh_system(&mut sys);
    let snap = collect_snapshot(&sys);

    // There is always at least one process running (the test binary itself).
    assert!(!snap.processes.is_empty(), "process list must not be empty");

    for p in &snap.processes {
        assert!(p.cpu_usage_pct >= 0.0, "cpu_usage_pct must be non-negative");
    }
}

#[test]
fn snapshot_processes_sorted_by_cpu_descending() {
    let mut sys = init_system();
    refresh_system(&mut sys);
    let snap = collect_snapshot(&sys);

    let usages: Vec<f32> = snap.processes.iter().map(|p| p.cpu_usage_pct).collect();
    for w in usages.windows(2) {
        assert!(
            w[0] >= w[1],
            "processes must be sorted by CPU usage descending"
        );
    }
}

#[test]
fn snapshot_timestamp_is_recent() {
    use chrono::Utc;
    let mut sys = init_system();
    refresh_system(&mut sys);
    let before = Utc::now();
    let snap = collect_snapshot(&sys);
    let after = Utc::now();

    assert!(
        snap.timestamp >= before && snap.timestamp <= after,
        "snapshot timestamp must fall within the collection window"
    );
}

// ── Logger tests ─────────────────────────────────────────────────────────────

#[test]
fn write_snapshot_to_file_produces_valid_ndjson() {
    let mut sys = init_system();
    refresh_system(&mut sys);
    let snap = collect_snapshot(&sys);

    let tmp = NamedTempFile::new().expect("temp file");
    let writer = open_log_file(tmp.path()).expect("open log file");
    let mut output = LogOutput::File {
        writer,
        format: FileOutputFormat::Json,
    };

    write_snapshot(&mut output, &snap).expect("write snapshot");

    // File must contain exactly one JSON line.
    let content = std::fs::read_to_string(tmp.path()).expect("read log file");
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1, "one line per snapshot");

    // The line must deserialise back to a snapshot without errors.
    let _: crash_reporter::collector::SystemSnapshot =
        serde_json::from_str(lines[0]).expect("valid JSON snapshot");
}

#[test]
fn write_multiple_snapshots_appends_lines() {
    let mut sys = init_system();
    refresh_system(&mut sys);

    let tmp = NamedTempFile::new().expect("temp file");

    for _ in 0..3 {
        let snap = collect_snapshot(&sys);
        let writer = open_log_file(tmp.path()).expect("open log file");
        let mut output = LogOutput::File {
            writer,
            format: FileOutputFormat::Json,
        };
        write_snapshot(&mut output, &snap).expect("write snapshot");
    }

    let content = std::fs::read_to_string(tmp.path()).expect("read log file");
    assert_eq!(content.lines().count(), 3, "three lines for three snapshots");
}

#[test]
fn snapshot_json_contains_expected_keys() {
    let mut sys = init_system();
    refresh_system(&mut sys);
    let snap = collect_snapshot(&sys);

    let json = serde_json::to_value(&snap).expect("serialise snapshot");
    for key in &[
        "timestamp",
        "cpu",
        "memory",
        "processes",
        "thermals",
        "disks",
        "networks",
        "usb_devices",
        "monitors",
    ] {
        assert!(json.get(key).is_some(), "JSON snapshot must contain key '{key}'");
    }
}

#[test]
fn plain_text_output_contains_human_readable_sections() {
    let mut sys = init_system();
    refresh_system(&mut sys);
    let snap = collect_snapshot(&sys);

    let tmp = NamedTempFile::new().expect("temp file");
    let writer = open_log_file(tmp.path()).expect("open log file");
    let mut output = LogOutput::File {
        writer,
        format: FileOutputFormat::PlainText,
    };

    write_snapshot(&mut output, &snap).expect("write snapshot");
    let content = std::fs::read_to_string(tmp.path()).expect("read log file");

    assert!(content.contains("Timestamp"), "plain text should include timestamp");
    assert!(
        content.contains("TOP PROCESSES"),
        "plain text should include process section"
    );
}

#[test]
fn file_output_format_uses_extension_for_plain_text() {
    assert_eq!(
        file_output_format(std::path::Path::new("report.txt")),
        FileOutputFormat::PlainText
    );
    assert_eq!(
        file_output_format(std::path::Path::new("report.log")),
        FileOutputFormat::PlainText
    );
    assert_eq!(
        file_output_format(std::path::Path::new("report.json")),
        FileOutputFormat::Json
    );
}

#[test]
fn snapshot_usb_and_monitor_entries_are_well_formed() {
    let mut sys = init_system();
    refresh_system(&mut sys);
    let snap = collect_snapshot(&sys);

    for usb in &snap.usb_devices {
        assert!(!usb.is_empty(), "usb entry should not be empty");
        assert!(usb.contains(':'), "usb entry should contain vendor:product");
    }

    for monitor in &snap.monitors {
        assert!(!monitor.is_empty(), "monitor entry should not be empty");
    }
}
