use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use sysinfo::{Components, Disks, Networks, System};

/// A snapshot of all collected system metrics at one point in time.
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub timestamp: DateTime<Utc>,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub processes: Vec<ProcessInfo>,
    pub thermals: Vec<ThermalComponent>,
    pub disks: Vec<DiskInfo>,
    pub networks: Vec<NetworkInfo>,
    pub usb_devices: Vec<String>,
    pub monitors: Vec<String>,
}

/// Overall and per-core CPU usage.
#[derive(Debug, Serialize, Deserialize)]
pub struct CpuInfo {
    /// Logical core count
    pub core_count: usize,
    /// Global CPU usage 0–100 %
    pub global_usage_pct: f32,
    /// Per-core usage 0–100 %
    pub per_core_usage_pct: Vec<f32>,
    /// CPU brand string (first CPU)
    pub brand: String,
    /// CPU frequency in MHz (first CPU)
    pub frequency_mhz: u64,
}

/// RAM and swap information.
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

/// A single running process.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage_pct: f32,
    pub memory_bytes: u64,
    pub status: String,
}

/// Temperature reading from a hardware sensor.
#[derive(Debug, Serialize, Deserialize)]
pub struct ThermalComponent {
    /// Component label (e.g. "CPU Package", "GPU Core")
    pub label: String,
    pub temperature_celsius: f32,
    /// Manufacturer-defined critical threshold, if available
    pub critical_celsius: Option<f32>,
    /// Manufacturer-defined high threshold, if available
    pub high_celsius: Option<f32>,
}

/// Disk usage information.
#[derive(Debug, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub file_system: String,
}

/// Network interface counters.
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub interface: String,
    pub received_bytes: u64,
    pub transmitted_bytes: u64,
}

/// Collect a full system snapshot using the provided `sysinfo::System`.
///
/// `sys` must have been refreshed before calling this function.
pub fn collect_snapshot(sys: &System) -> SystemSnapshot {
    let timestamp = Utc::now();

    // ── CPU ──────────────────────────────────────────────────────────────────
    let cpus = sys.cpus();
    let global_usage_pct = sys.global_cpu_usage();
    let per_core_usage_pct: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
    let (brand, frequency_mhz) = cpus
        .first()
        .map(|c| (c.brand().to_string(), c.frequency()))
        .unwrap_or_default();

    let cpu = CpuInfo {
        core_count: cpus.len(),
        global_usage_pct,
        per_core_usage_pct,
        brand,
        frequency_mhz,
    };

    // ── Memory ───────────────────────────────────────────────────────────────
    let memory = MemoryInfo {
        total_bytes: sys.total_memory(),
        used_bytes: sys.used_memory(),
        available_bytes: sys.available_memory(),
        swap_total_bytes: sys.total_swap(),
        swap_used_bytes: sys.used_swap(),
    };

    // ── Processes ────────────────────────────────────────────────────────────
    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            cpu_usage_pct: p.cpu_usage(),
            memory_bytes: p.memory(),
            status: format!("{:?}", p.status()),
        })
        .collect();
    processes.sort_by(|a, b| {
        b.cpu_usage_pct
            .partial_cmp(&a.cpu_usage_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // ── Thermals ─────────────────────────────────────────────────────────────
    let components = Components::new_with_refreshed_list();
    let thermals: Vec<ThermalComponent> = components
        .iter()
        .map(|c| ThermalComponent {
            label: c.label().to_string(),
            temperature_celsius: c.temperature().unwrap_or(0.0),
            critical_celsius: c.critical(),
            high_celsius: c.max(),
        })
        .collect();

    // ── Disks ────────────────────────────────────────────────────────────────
    let disk_list = Disks::new_with_refreshed_list();
    let disks: Vec<DiskInfo> = disk_list
        .iter()
        .map(|d| {
            let total = d.total_space();
            let avail = d.available_space();
            DiskInfo {
                name: d.name().to_string_lossy().into_owned(),
                mount_point: d.mount_point().to_string_lossy().into_owned(),
                total_bytes: total,
                available_bytes: avail,
                used_bytes: total.saturating_sub(avail),
                file_system: d.file_system().to_string_lossy().into_owned(),
            }
        })
        .collect();

    // ── Networks ─────────────────────────────────────────────────────────────
    let net = Networks::new_with_refreshed_list();
    let networks: Vec<NetworkInfo> = net
        .iter()
        .map(|(name, data)| NetworkInfo {
            interface: name.clone(),
            received_bytes: data.total_received(),
            transmitted_bytes: data.total_transmitted(),
        })
        .collect();

    let usb_devices = collect_usb_devices();
    let monitors = collect_monitors();

    SystemSnapshot {
        timestamp,
        cpu,
        memory,
        processes,
        thermals,
        disks,
        networks,
        usb_devices,
        monitors,
    }
}

/// Initialise a `sysinfo::System` and perform the two-sample refresh needed
/// to get accurate CPU usage percentages.
pub fn init_system() -> System {
    let mut sys = System::new_all();
    sys.refresh_all();
    sys
}

/// Refresh all subsystems in the given `System` to get up-to-date readings.
pub fn refresh_system(sys: &mut System) {
    sys.refresh_all();
}

fn collect_usb_devices() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        let mut devices = Vec::new();
        if let Ok(entries) = fs::read_dir("/sys/bus/usb/devices") {
            for entry in entries.flatten() {
                let path = entry.path();
                let vendor = fs::read_to_string(path.join("idVendor")).ok();
                let product = fs::read_to_string(path.join("idProduct")).ok();
                if let (Some(vendor), Some(product)) = (vendor, product) {
                    let product_name = fs::read_to_string(path.join("product")).ok();
                    let vendor = vendor.trim();
                    let product = product.trim();
                    let product_name = product_name.unwrap_or_default();
                    let product_name = product_name.trim();
                    devices.push(if product_name.is_empty() {
                        format!("{vendor}:{product}")
                    } else {
                        format!("{vendor}:{product} {product_name}")
                    });
                }
            }
        }
        devices.sort();
        devices.dedup();
        devices
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

fn collect_monitors() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        let mut monitors = Vec::new();
        if let Ok(entries) = fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let path = entry.path();
                let status_path = path.join("status");
                let enabled = fs::read_to_string(status_path)
                    .ok()
                    .map(|s| s.trim() == "connected")
                    .unwrap_or(false);

                if enabled
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && name.contains('-')
                {
                    monitors.push(name.to_string());
                }
            }
        }
        monitors.sort();
        monitors.dedup();
        monitors
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}
