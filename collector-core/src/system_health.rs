use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use std::sync::Arc;
use sysinfo::{Components, Disks, System};

#[derive(Serialize)]
pub struct CpuInfo {
    pub cores: usize,
    pub usage_percent: Vec<f64>,
    pub usage_total_percent: f64,
}

#[derive(Serialize)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

#[derive(Serialize)]
pub struct DiskInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
    pub reads_per_sec: f64,
    pub writes_per_sec: f64,
}

#[derive(Serialize)]
pub struct SystemInfo {
    pub uptime_seconds: u64,
    pub uptime_human: String,
}

#[derive(Serialize)]
pub struct ProcessInfo {
    pub rss_bytes: u64,
    pub cpu_percent: f64,
    pub threads: u32,
    pub uptime_seconds: u64,
}

#[derive(Serialize)]
pub struct CollectorInfo {
    pub flows_received: u64,
    pub flows_decoded: u64,
    pub packets_dropped: u64,
    pub template_cache_size: i64,
}

#[derive(Serialize)]
pub struct SystemHealth {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disk: DiskInfo,
    pub system: SystemInfo,
    pub process: ProcessInfo,
    pub collector: CollectorInfo,
}

fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{} dia{}, {}h {}m", days, if days == 1 { "" } else { "s" }, hours, mins)
    } else {
        format!("{}h {}m", hours, mins)
    }
}

fn read_diskstats_iops(device: &str) -> Option<(u64, u64)> {
    let content = std::fs::read_to_string("/proc/diskstats").ok()?;
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 14 { continue; }
        if parts[2] == device {
            let reads:  u64 = parts[3].parse().unwrap_or(0);
            let writes: u64 = parts[7].parse().unwrap_or(0);
            return Some((reads, writes));
        }
    }
    None
}

fn root_device_name() -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "/" {
            let dev = parts[0].trim_start_matches("/dev/");
            return Some(strip_partition_suffix(dev).to_string());
        }
    }
    None
}

fn strip_partition_suffix(dev: &str) -> &str {
    // nvme0n1p1 → nvme0n1
    if let Some(idx) = dev.rfind('p') {
        let suffix = &dev[idx + 1..];
        if suffix.chars().all(|c| c.is_ascii_digit()) && idx > 0 {
            return &dev[..idx];
        }
    }
    // sda1 → sda
    let trimmed = dev.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.len() < dev.len() { trimmed } else { dev }
}

pub async fn get_health_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<SystemHealth>, StatusCode> {
    // Refresh CPU usage — sysinfo needs two calls with a delay between them
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    sys.refresh_all();

    // CPU
    let cpu_usages: Vec<f64> = sys.cpus().iter().map(|c| c.cpu_usage() as f64).collect();
    let cpu_total = if cpu_usages.is_empty() { 0.0 }
        else { cpu_usages.iter().sum::<f64>() / cpu_usages.len() as f64 };

    // Memory
    let total_mem = sys.total_memory();
    let used_mem  = sys.used_memory();
    let avail_mem = sys.available_memory();
    let mem_pct   = if total_mem > 0 { (used_mem as f64 / total_mem as f64) * 100.0 } else { 0.0 };

    // Disk — use sysinfo for capacity, /proc/diskstats for IOPS
    let disks = Disks::new_with_refreshed_list();
    let (disk_total, disk_used, disk_avail) = disks.iter()
        .find(|d| d.mount_point() == std::path::Path::new("/"))
        .map(|d| (d.total_space(), d.total_space() - d.available_space(), d.available_space()))
        .unwrap_or((0, 0, 0));
    let disk_pct = if disk_total > 0 { (disk_used as f64 / disk_total as f64) * 100.0 } else { 0.0 };

    let (reads_ps, writes_ps) = if let Some(dev) = root_device_name() {
        let s0 = read_diskstats_iops(&dev);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let s1 = read_diskstats_iops(&dev);
        match (s0, s1) {
            (Some((r0, w0)), Some((r1, w1))) => (
                (r1.saturating_sub(r0) as f64) * 2.0,
                (w1.saturating_sub(w0) as f64) * 2.0,
            ),
            _ => (0.0, 0.0),
        }
    } else {
        (0.0, 0.0)
    };

    // System uptime
    let uptime_secs = System::uptime();

    // Process info (self)
    let pid = sysinfo::get_current_pid().ok();
    let (rss_bytes, proc_cpu, threads, proc_uptime) = match pid {
        Some(p) => {
            let proc = sys.process(p);
            let rss   = proc.map(|pr| pr.memory()).unwrap_or(0);
            let cpu   = proc.map(|pr| pr.cpu_usage() as f64 / sys.cpus().len().max(1) as f64).unwrap_or(0.0);
            let thr   = proc.map(|_| {
                // sysinfo doesn't expose thread count directly; read from /proc
                std::fs::read_to_string("/proc/self/status")
                    .ok()
                    .and_then(|s| s.lines()
                        .find(|l| l.starts_with("Threads:"))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|n| n.parse::<u32>().ok()))
                    .unwrap_or(0)
            }).unwrap_or(0);
            let start = proc.map(|pr| pr.start_time()).unwrap_or(0);
            let pup   = uptime_secs.saturating_sub(
                if start > 0 {
                    // start_time is UNIX epoch; uptime is relative
                    // compute process uptime from system boot
                    let boot_ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                        .saturating_sub(uptime_secs);
                    start.saturating_sub(boot_ts)
                } else { uptime_secs }
            );
            (rss, cpu, thr, pup)
        }
        None => (0, 0.0, 0, 0),
    };

    // Collector metrics
    let flows_received  = state.metrics.flows_received.get() as u64;
    let flows_decoded   = state.metrics.flows_decoded.get() as u64;
    let packets_dropped = state.metrics.packets_dropped.get() as u64;
    let tmpl_cache_size = state.metrics.template_cache_size.get();

    // Suppress unused variable from sysinfo refresh
    let _ = Components::new_with_refreshed_list();

    Ok(Json(SystemHealth {
        cpu: CpuInfo {
            cores: cpu_usages.len(),
            usage_total_percent: cpu_total,
            usage_percent: cpu_usages,
        },
        memory: MemoryInfo {
            total_bytes: total_mem,
            used_bytes: used_mem,
            available_bytes: avail_mem,
            used_percent: mem_pct,
        },
        disk: DiskInfo {
            total_bytes: disk_total,
            used_bytes: disk_used,
            available_bytes: disk_avail,
            used_percent: disk_pct,
            reads_per_sec: reads_ps,
            writes_per_sec: writes_ps,
        },
        system: SystemInfo {
            uptime_seconds: uptime_secs,
            uptime_human: format_uptime(uptime_secs),
        },
        process: ProcessInfo {
            rss_bytes,
            cpu_percent: cpu_cpu_round(proc_cpu),
            threads,
            uptime_seconds: proc_uptime,
        },
        collector: CollectorInfo {
            flows_received,
            flows_decoded,
            packets_dropped,
            template_cache_size: tmpl_cache_size,
        },
    }))
}

fn cpu_cpu_round(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}
