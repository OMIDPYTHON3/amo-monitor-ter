//! Linux-specific sysfs/proc readers.
//!
//! Everything in here is best-effort: a missing sensor returns `None`, never
//! a fabricated number.

use std::path::Path;

use crate::util::read_sysfs;

/// Return the whole-device read and written byte counters from `/proc/diskstats`.
///
/// Partition and pseudo-device entries are ignored so that counters are not
/// double counted. Each sector is assumed to be 512 bytes (kernel convention).
pub fn parse_diskstats(text: &str) -> Option<(u64, u64)> {
    let mut reads = 0u64;
    let mut writes = 0u64;
    let mut found = false;

    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        let name = fields[2];
        if !is_whole_disk(name) {
            continue;
        }
        let sectors_read: u64 = fields[5].parse().ok()?;
        let sectors_written: u64 = fields[9].parse().ok()?;
        reads = reads.saturating_add(sectors_read.saturating_mul(512));
        writes = writes.saturating_add(sectors_written.saturating_mul(512));
        found = true;
    }

    if found {
        Some((reads, writes))
    } else {
        None
    }
}

/// Whether a `/proc/diskstats` name refers to a whole physical disk rather
/// than a partition, RAM disk, device-mapper volume or loop device.
pub fn is_whole_disk(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let excluded_prefixes = [
        "loop", "ram", "dm-", "md", "fd", "sr", "scm", "zram", "veth", "br-", "docker",
        "overlay", "nvme0n1p",
    ];
    if excluded_prefixes.iter().any(|p| name.starts_with(p)) {
        return false;
    }
    if name.starts_with("nvme") || name.starts_with("mmcblk") {
        // Partitions are suffixed with `p<number>`; whole devices are not.
        return !name.contains('p');
    }
    if name.starts_with("sd")
        || name.starts_with("hd")
        || name.starts_with("vd")
        || name.starts_with("xvd")
        || name.starts_with("ubd")
    {
        return !name.as_bytes().last().is_some_and(u8::is_ascii_digit);
    }
    false
}

/// Read whole-disk I/O counters from `/proc/diskstats`.
pub fn read_disk_totals() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/diskstats").ok()?;
    parse_diskstats(&text)
}

/// Read system uptime (seconds) from `/proc/uptime`.
pub fn read_uptime() -> Option<u64> {
    let text = read_sysfs(Path::new("/proc/uptime"))?;
    text.split_whitespace().next()?.parse::<f64>().ok().map(|v| v as u64)
}

/// Pick the most plausible CPU temperature from Linux thermal zones / hwmon.
///
/// Preference order: explicit CPU/package/SoC labels, then any thermal zone.
pub fn read_cpu_temperature() -> Option<f64> {
    let mut fallback: Option<f64> = None;

    if let Ok(entries) = std::fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("thermal_zone") {
                continue;
            }
            let zone_type = read_sysfs(&entry.path().join("type")).unwrap_or_default();
            let temp = read_sysfs(&entry.path().join("temp"))
                .and_then(|v| v.parse::<f64>().ok())
                .map(|milli| milli / 1000.0);
            if let Some(t) = temp {
                let label = zone_type.to_lowercase();
                if label.contains("cpu") || label.contains("package") || label.contains("soc") {
                    return Some(t);
                }
                if fallback.is_none() {
                    fallback = Some(t);
                }
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            let chip = read_sysfs(&path.join("name")).unwrap_or_default().to_lowercase();
            if !(chip.contains("cpu") || chip.contains("k10temp") || chip.contains("zenpower"))
            {
                continue;
            }
            if let Ok(files) = std::fs::read_dir(&path) {
                for file in files.flatten() {
                    let name = file.file_name().to_string_lossy().to_string();
                    if name.starts_with("temp") && name.ends_with("_input") {
                        if let Some(t) = read_sysfs(&file.path())
                            .and_then(|v| v.parse::<f64>().ok())
                            .map(|m| m / 1000.0)
                        {
                            return Some(t);
                        }
                    }
                }
            }
        }
    }

    fallback
}

/// Sum the current RAPL energy counters (microjoules) across CPU packages.
///
/// Returns `None` when the platform exposes no RAPL interface (for example,
/// most ARM boards). Wrap-around is handled by the caller using the reported
/// maximum energy range.
pub fn read_cpu_energy_uj() -> Option<(u64, u64)> {
    let base = Path::new("/sys/class/powercap");
    let entries = std::fs::read_dir(base).ok()?;
    let mut total = 0u64;
    let mut max_range = 0u64;
    let mut found = false;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.contains("rapl") {
            continue;
        }
        let path = entry.path();
        let label = read_sysfs(&path.join("name")).unwrap_or_default().to_lowercase();
        // Only package-level domains; ignore per-core / uncore sub-domains to
        // avoid counting the same energy multiple times.
        if !label.contains("package") && !label.contains("pkg") {
            continue;
        }
        let energy = read_sysfs(&path.join("energy_uj")).and_then(|v| v.parse::<u64>().ok());
        let range = read_sysfs(&path.join("max_energy_range_uj"))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        if let Some(e) = energy {
            total = total.saturating_add(e);
            max_range = max_range.max(range);
            found = true;
        }
    }

    if found {
        Some((total, max_range))
    } else {
        None
    }
}

/// Parse the V3D `gpu_stats` file used by Raspberry Pi's VideoCore GPU.
///
/// The file has columns `queue timestamp jobs runtime`; `runtime` is a
/// monotonically increasing busy-time counter in nanoseconds. Returns the
/// largest per-queue runtime, which is used to derive real GPU utilisation.
pub fn parse_v3d_runtime(text: &str) -> Option<u64> {
    let mut lines = text.lines();
    let header = lines.next()?;
    if !header.split_whitespace().any(|f| f == "runtime") {
        return None;
    }
    let mut maximum = 0u64;
    let mut found = false;
    for line in lines {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        if let Ok(runtime) = fields[3].parse::<u64>() {
            maximum = maximum.max(runtime);
            found = true;
        }
    }
    if found {
        Some(maximum)
    } else {
        None
    }
}

/// Read the V3D GPU busy-time counter and its path (for delta calculation).
pub fn read_v3d_runtime() -> Option<(String, u64)> {
    let entries = std::fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") {
            continue;
        }
        let device = entry.path().join("device");
        let stats_path = device.join("gpu_stats");
        if !stats_path.exists() {
            continue;
        }
        let driver = read_sysfs(&device.join("uevent"))
            .unwrap_or_default()
            .lines()
            .find_map(|l: &str| l.strip_prefix("DRIVER="))
            .unwrap_or("")
            .to_string();
        if driver != "v3d" {
            continue;
        }
        let text = std::fs::read_to_string(&stats_path).ok()?;
        let runtime = parse_v3d_runtime(&text)?;
        return Some((stats_path.to_string_lossy().to_string(), runtime));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diskstats_parsing() {
        let sample = "   8       0 sda 1000 20 80000 300 500 5 40000 200 0 100 200\n\
                      8       1 sda1 900 20 79000 290 400 5 39000 190 0 90 190\n\
                     179       0 mmcblk0 10 0 800 5 10 0 800 5 0 0 0\n\
                       7       0 loop0 4 0 32 1 0 0 0 0 0 0 0\n";
        let (read, write) = parse_diskstats(sample).unwrap();
        // Only sda and mmcblk0 are whole disks.
        assert_eq!(read, (80_000 + 800) * 512);
        assert_eq!(write, (40_000 + 800) * 512);
    }

    #[test]
    fn whole_disk_filter() {
        assert!(is_whole_disk("sda"));
        assert!(is_whole_disk("nvme0n1"));
        assert!(is_whole_disk("mmcblk0"));
        assert!(!is_whole_disk("sda1"));
        assert!(!is_whole_disk("nvme0n1p1"));
        assert!(!is_whole_disk("mmcblk0p1"));
        assert!(!is_whole_disk("loop0"));
        assert!(!is_whole_disk("dm-0"));
    }

    #[test]
    fn v3d_runtime_parsing() {
        let sample = "queue\ttimestamp\tjobs\truntime\n\
                      bin\t100\t0\t12345\n\
                      render\t100\t2\t67890\n\
                      csd\t100\t0\t111\n";
        assert_eq!(parse_v3d_runtime(sample), Some(67_890));
        assert_eq!(parse_v3d_runtime("nonsense"), None);
    }
}
