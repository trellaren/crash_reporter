# crash_reporter
A lightweight cli app in rust for logging system information, thermal data, and crash reports

By default, output files are NDJSON. If `--output` points to a `.log` or `.txt` file,
the report is written as plain text instead. Snapshots include USB device and monitor data.
