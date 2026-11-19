//! Platform-specific GPU monitoring.
//!
//! Supported sources (all optional):
//!   * NVIDIA  - `nvidia-smi` (utilisation, clock, temperature, power)
//!   * AMD/Intel DRM - `gpu_busy_percent` and device hwmon nodes
//!   * Broadcom V3D (Raspberry Pi) - the `gpu_stats` busy-time counter
//!
//! If no source is available, every field is `None` and the UI shows `N/A`.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GpuHardware {
    pub name: String,
    pub busy_path: Option<PathBuf>,
    pub freq_path: Option<PathBuf>,
    pub temp_path: Option<PathBuf>,
    pub power_path: Option<PathBuf>,
    pub nvidia: bool,
    pub v3d: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GpuRaw {
    pub usage_percent: Option<f64>,
    pub frequency_mhz: Option<f64>,
    pub temperature_c: Option<f64>,
    pub power_w: Option<f64>,
    /// Monotonic busy-time counter (nanoseconds) for V3D GPUs.
    pub v3d_runtime_ns: Option<u64>,
}

impl GpuHardware {
    /// Detect the first usable GPU on the system.
    pub fn detect() -> Option<Self> {
        if let Some(gpu) = Self::detect_nvidia() {
            return Some(gpu);
        }
        Self::detect_drm()
    }

    fn detect_nvidia() -> Option<Self> {
        if !Path::new("/dev/nvidia0").exists() {
            return None;
        }
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=name",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let name = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        Some(Self {
            name,
            busy_path: None,
            freq_path: None,
            temp_path: None,
            power_path: None,
            nvidia: true,
            v3d: false,
        })
    }

    fn detect_drm() -> Option<Self> {
        let entries = std::fs::read_dir("/sys/class/drm").ok()?;
        let mut cards: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("card") && n[4..].chars().all(|c| c.is_ascii_digit()))
            })
            .collect();
        cards.sort();

        for card in cards {
            let device = card.join("device");
            if !device.exists() {
                continue;
            }
            let driver = crate::util::read_sysfs(&device.join("uevent"))
                .unwrap_or_default()
                .lines()
                .find_map(|l: &str| l.strip_prefix("DRIVER="))
                .unwrap_or("")
                .to_string();
            if driver.is_empty() || driver == "vc4-drm" {
                // vc4-drm is the display stack; the render node (v3d) carries
                // the actual GPU counters.
                if has_v3d_stats(&device) {
                    return Some(Self::from_v3d(device));
                }
                continue;
            }

            let busy_path = path_if_exists(device.join("gpu_busy_percent"));
            let v3d = has_v3d_stats(&device);
            let (freq_path, temp_path, power_path) = find_hwmon_files(&device);
            if busy_path.is_none() && !v3d && freq_path.is_none() && temp_path.is_none() {
                continue;
            }

            return Some(Self {
                name: friendly_driver_name(&driver),
                busy_path,
                freq_path,
                temp_path,
                power_path,
                nvidia: false,
                v3d,
            });
        }
        None
    }

    fn from_v3d(device: PathBuf) -> Self {
        Self {
            name: "Broadcom V3D (Raspberry Pi)".to_string(),
            busy_path: None,
            freq_path: find_hwmon_files(&device).0,
            temp_path: find_hwmon_files(&device).1,
            power_path: find_hwmon_files(&device).2,
            nvidia: false,
            v3d: true,
        }
    }

    /// Read the current raw GPU counters.
    pub fn read(&self) -> GpuRaw {
        if self.nvidia {
            return self.read_nvidia();
        }
        let mut raw = GpuRaw::default();
        if let Some(path) = &self.busy_path {
            raw.usage_percent = read_sysfs_f64(path);
        }
        if self.v3d {
            raw.v3d_runtime_ns = super::linux::read_v3d_runtime().map(|(_, r)| r);
        }
        if let Some(path) = &self.freq_path {
            raw.frequency_mhz = read_sysfs_f64(path).map(|hz| hz / 1_000_000.0);
        }
        if let Some(path) = &self.temp_path {
            raw.temperature_c = read_sysfs_f64(path).map(|milli| milli / 1_000.0);
        }
        if let Some(path) = &self.power_path {
            raw.power_w = read_sysfs_f64(path).map(|micro| micro / 1_000_000.0);
        }
        raw
    }

    fn read_nvidia(&self) -> GpuRaw {
        let mut raw = GpuRaw::default();
        let Ok(output) = Command::new("nvidia-smi")
            .args([
                "--query-gpu=utilization.gpu,clocks.current.graphics,temperature.gpu,power.draw",
                "--format=csv,noheader,nounits",
            ])
            .output()
        else {
            return raw;
        };
        if !output.status.success() {
            return raw;
        }
        let line = String::from_utf8_lossy(&output.stdout);
        let line = line.lines().next().unwrap_or("");
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        raw.usage_percent = fields.first().and_then(|v| v.parse().ok());
        raw.frequency_mhz = fields.get(1).and_then(|v| v.parse().ok());
        raw.temperature_c = fields.get(2).and_then(|v| v.parse().ok());
        raw.power_w = fields.get(3).and_then(|v| v.parse().ok());
        raw
    }
}

fn has_v3d_stats(device: &Path) -> bool {
    let driver = crate::util::read_sysfs(&device.join("uevent"))
        .unwrap_or_default()
        .lines()
        .find_map(|l: &str| l.strip_prefix("DRIVER="))
        .unwrap_or("")
        .to_string();
    driver == "v3d" && device.join("gpu_stats").exists()
}

fn path_if_exists(path: PathBuf) -> Option<PathBuf> {
    path.exists().then_some(path)
}

/// Find (frequency, temperature, power) input files on a DRM device's hwmon
/// interfaces. Values are raw sysfs units (Hz, millidegrees, microwatts).
fn find_hwmon_files(device: &Path) -> (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) {
    let base = device.join("hwmon");
    let readers = match std::fs::read_dir(&base) {
        Ok(r) => r,
        Err(_) => return (None, None, None),
    };
    let mut freq = None;
    let mut temp = None;
    let mut power = None;
    for hwmon in readers.flatten() {
        let dir = hwmon.path();
        let files = match std::fs::read_dir(&dir) {
            Ok(f) => f,
            Err(_) => continue,
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().to_string();
            if freq.is_none() && is_input(&name, "freq") {
                freq = Some(file.path());
            }
            if temp.is_none() && is_input(&name, "temp") {
                temp = Some(file.path());
            }
            if power.is_none() && (is_input(&name, "power") || name.starts_with("power1_average")) {
                power = Some(file.path());
            }
        }
    }
    (freq, temp, power)
}

fn is_input(name: &str, prefix: &str) -> bool {
    name.starts_with(prefix) && name.ends_with("_input")
}

fn friendly_driver_name(driver: &str) -> String {
    match driver {
        "amdgpu" => "AMD Radeon (amdgpu)".to_string(),
        "radeon" => "AMD Radeon (radeon)".to_string(),
        "i915" | "xe" => "Intel Graphics".to_string(),
        "nouveau" => "NVIDIA Graphics (nouveau)".to_string(),
        "panfrost" => "ARM Mali (Panfrost)".to_string(),
        "v3d" => "Broadcom V3D (Raspberry Pi)".to_string(),
        "etnaviv" => "Vivante GPU (etnaviv)".to_string(),
        other => format!("{} Graphics", other),
    }
}

fn read_sysfs_f64(path: &Path) -> Option<f64> {
    crate::util::read_sysfs(path)?.parse().ok()
}
