//! System monitoring: data model, sampling and rolling history.

pub mod gpu;
pub mod linux;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use sysinfo::{Components, Disks, Networks, System};

use crate::util::clamp;
use gpu::{GpuHardware, GpuRaw};

/// Default chart window shown by widgets, in seconds.
pub const HISTORY_WINDOW_SECS: f64 = 30.0;
/// Maximum history retained so any widget can show 30s, 1m or 5m windows.
pub const HISTORY_MAX_WINDOW_SECS: f64 = 300.0;
/// Hard cap on stored samples (one per second at the normal tick rate).
pub const HISTORY_CAPACITY: usize = 600;

#[derive(Clone, Debug, Default)]
pub struct CpuData {
    pub usage_percent: Option<f64>,
    pub per_core: Vec<f64>,
    pub frequency_mhz: Option<f64>,
    pub temperature_c: Option<f64>,
    pub power_w: Option<f64>,
    pub cores: usize,
    pub model: String,
}

#[derive(Clone, Debug, Default)]
pub struct GpuData {
    pub name: String,
    pub usage_percent: Option<f64>,
    pub frequency_mhz: Option<f64>,
    pub temperature_c: Option<f64>,
    pub power_w: Option<f64>,
    pub available: bool,
}

#[derive(Clone, Debug, Default)]
pub struct MemoryData {
    pub used_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub percent: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct NetworkData {
    pub rx_bytes_per_sec: Option<f64>,
    pub tx_bytes_per_sec: Option<f64>,
    pub total_rx_bytes: Option<u64>,
    pub total_tx_bytes: Option<u64>,
    pub interface: String,
}

#[derive(Clone, Debug, Default)]
pub struct DiskData {
    pub read_bytes_per_sec: Option<f64>,
    pub write_bytes_per_sec: Option<f64>,
    pub device_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct SystemData {
    pub uptime_secs: Option<u64>,
    pub hostname: String,
    pub kernel: String,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub cpu: CpuData,
    pub gpu: GpuData,
    pub memory: MemoryData,
    pub swap: MemoryData,
    pub network: NetworkData,
    pub disk: DiskData,
    pub system: SystemData,
}

/// Identifies an independently chartable metric series.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeriesKey {
    CpuUsage,
    CpuFrequency,
    CpuTemperature,
    CpuPower,
    GpuUsage,
    GpuFrequency,
    GpuTemperature,
    GpuPower,
    MemoryUsed,
    SwapUsed,
    NetworkRx,
    NetworkTx,
    DiskRead,
    DiskWrite,
}

/// A time-windowed ring of samples. Old samples are dropped by timestamp so
/// the chart always covers roughly the last 30 seconds, regardless of tick rate.
#[derive(Clone, Debug)]
pub struct History {
    samples: Vec<(f64, f64)>,
    window: f64,
    capacity: usize,
}

#[allow(dead_code)] // Some accessors are primarily used by tests and extensions.
impl History {
    pub fn new(window: f64, capacity: usize) -> Self {
        Self {
            samples: Vec::with_capacity(capacity.min(256)),
            window,
            capacity,
        }
    }

    pub fn push(&mut self, time: f64, value: f64) {
        if !value.is_finite() {
            return;
        }
        self.samples.push((time, value));
        self.samples.retain(|(t, _)| time - *t < self.window);
        if self.samples.len() > self.capacity {
            let excess = self.samples.len() - self.capacity;
            self.samples.drain(..excess);
        }
    }

    pub fn samples(&self) -> &[(f64, f64)] {
        &self.samples
    }

    pub fn values(&self) -> impl Iterator<Item = f64> + '_ {
        self.samples.iter().map(|(_, v)| *v)
    }

    pub fn latest(&self) -> Option<f64> {
        self.samples.last().map(|(_, v)| *v)
    }

    pub fn max(&self) -> Option<f64> {
        self.values().fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.max(v))))
    }

    pub fn min(&self) -> Option<f64> {
        self.values().fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.min(v))))
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

#[derive(Clone, Debug)]
pub struct HistoryStore {
    series: HashMap<SeriesKey, History>,
    window: f64,
}

impl HistoryStore {
    pub fn new(window: f64) -> Self {
        Self {
            series: HashMap::new(),
            window,
        }
    }

    pub fn get(&self, key: SeriesKey) -> Option<&History> {
        self.series.get(&key)
    }

    pub fn push(&mut self, key: SeriesKey, time: f64, value: Option<f64>) {
        if let Some(v) = value {
            self.series
                .entry(key)
                .or_insert_with(|| History::new(self.window, HISTORY_CAPACITY))
                .push(time, v);
        }
    }

    /// Record every series of a snapshot at the given timestamp.
    pub fn record(&mut self, snapshot: &Snapshot, time: f64) {
        let s = snapshot;
        self.push(SeriesKey::CpuUsage, time, s.cpu.usage_percent);
        self.push(SeriesKey::CpuFrequency, time, s.cpu.frequency_mhz);
        self.push(SeriesKey::CpuTemperature, time, s.cpu.temperature_c);
        self.push(SeriesKey::CpuPower, time, s.cpu.power_w);
        self.push(SeriesKey::GpuUsage, time, s.gpu.usage_percent);
        self.push(SeriesKey::GpuFrequency, time, s.gpu.frequency_mhz);
        self.push(SeriesKey::GpuTemperature, time, s.gpu.temperature_c);
        self.push(SeriesKey::GpuPower, time, s.gpu.power_w);
        self.push(SeriesKey::MemoryUsed, time, s.memory.percent);
        self.push(SeriesKey::SwapUsed, time, s.swap.percent);
        self.push(SeriesKey::NetworkRx, time, s.network.rx_bytes_per_sec);
        self.push(SeriesKey::NetworkTx, time, s.network.tx_bytes_per_sec);
        self.push(SeriesKey::DiskRead, time, s.disk.read_bytes_per_sec);
        self.push(SeriesKey::DiskWrite, time, s.disk.write_bytes_per_sec);
    }
}

/// Owns all monitoring state and produces fresh snapshots.
pub struct Monitor {
    system: System,
    networks: Networks,
    components: Components,
    // `Disks` is kept so the library maintains its mount table, even though
    // I/O rates come from /proc/diskstats for accuracy.
    _disks: Disks,
    gpu_hardware: Option<GpuHardware>,
    start: Instant,
    last_sample: Option<Instant>,
    last_network: Option<(u64, u64)>,
    last_disk: Option<(u64, u64)>,
    last_energy_uj: Option<u64>,
    last_v3d: Option<(String, u64)>,
    pub snapshot: Snapshot,
    pub history: HistoryStore,
}

impl Monitor {
    pub fn new() -> Self {
        let system = System::new_all();
        let networks = Networks::new_with_refreshed_list();
        let components = Components::new_with_refreshed_list();
        let disks = Disks::new_with_refreshed_list();
        let gpu_hardware = GpuHardware::detect();

        let mut monitor = Self {
            system,
            networks,
            components,
            _disks: disks,
            gpu_hardware,
            start: Instant::now(),
            last_sample: None,
            last_network: None,
            last_disk: None,
            last_energy_uj: None,
            last_v3d: None,
            snapshot: Snapshot::default(),
            history: HistoryStore::new(HISTORY_MAX_WINDOW_SECS),
        };

        // Two CPU samples are required before sysinfo can report a usage value.
        monitor.system.refresh_cpu_usage();
        std::thread::sleep(Duration::from_millis(250));
        monitor.sample();
        monitor
    }

    /// Minimum time between samples. Prevents redundant work and respects
    /// sysinfo's CPU refresh interval.
    pub fn min_interval() -> Duration {
        Duration::from_millis(250)
    }

    pub fn sample(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_sample {
            if now.duration_since(last) < Self::min_interval() {
                return;
            }
        }
        let dt = self
            .last_sample
            .map(|last| now.duration_since(last).as_secs_f64())
            .unwrap_or(1.0);
        self.last_sample = Some(now);

        self.system.refresh_cpu_usage();
        self.system.refresh_cpu_frequency();
        self.system.refresh_memory();
        self.networks.refresh(false);
        self.components.refresh(false);

        let mut snapshot = Snapshot::default();
        self.fill_cpu(&mut snapshot.cpu, dt);
        self.fill_gpu(&mut snapshot.gpu, dt);
        self.fill_memory(&mut snapshot.memory);
        self.fill_swap(&mut snapshot.swap);
        self.fill_network(&mut snapshot.network, dt);
        self.fill_disk(&mut snapshot.disk, dt);
        snapshot.system = SystemData {
            uptime_secs: linux::read_uptime().or_else(|| Some(System::uptime())),
            hostname: System::host_name().unwrap_or_default(),
            kernel: System::kernel_version().unwrap_or_default(),
        };

        let elapsed = self.start.elapsed().as_secs_f64();
        self.history.record(&snapshot, elapsed);
        self.snapshot = snapshot;
    }

    fn fill_cpu(&mut self, cpu: &mut CpuData, dt: f64) {
        let cpus = self.system.cpus();
        let cores = cpus.len();
        cpu.cores = cores;
        cpu.model = cpus.first().map(|c| c.brand().to_string()).unwrap_or_default();
        let usage = self.system.global_cpu_usage() as f64;
        cpu.usage_percent = Some(clamp(usage, 0.0, 100.0));
        cpu.per_core = cpus
            .iter()
            .map(|c| clamp(c.cpu_usage() as f64, 0.0, 100.0))
            .collect();
        if cores > 0 {
            let total: u64 = cpus.iter().map(|c| c.frequency()).sum();
            let mhz = total as f64 / cores as f64;
            cpu.frequency_mhz = (mhz > 0.0).then_some(mhz);
        }
        cpu.temperature_c = linux::read_cpu_temperature();
        cpu.power_w = self.read_cpu_power(dt);
    }

    fn read_cpu_power(&mut self, dt: f64) -> Option<f64> {
        let (energy_uj, max_range_uj) = linux::read_cpu_energy_uj()?;
        let Some(last) = self.last_energy_uj else {
            self.last_energy_uj = Some(energy_uj);
            return None;
        };
        let delta = if energy_uj >= last {
            energy_uj.saturating_sub(last)
        } else if max_range_uj > 0 {
            max_range_uj.saturating_sub(last).saturating_add(energy_uj)
        } else {
            return None;
        };
        self.last_energy_uj = Some(energy_uj);
        if dt <= 0.0 {
            return None;
        }
        let watts = delta as f64 / dt / 1_000_000.0;
        (watts.is_finite() && watts >= 0.0).then_some(watts)
    }

    fn fill_gpu(&mut self, gpu: &mut GpuData, dt: f64) {
        let Some(hardware) = &self.gpu_hardware else {
            return;
        };
        let raw: GpuRaw = hardware.read();
        gpu.name = hardware.name.clone();
        gpu.available = true;
        gpu.frequency_mhz = raw.frequency_mhz;
        gpu.temperature_c = raw.temperature_c;
        gpu.power_w = raw.power_w;
        gpu.usage_percent = raw.usage_percent;

        if let Some(runtime) = raw.v3d_runtime_ns {
            let usage = match self.last_v3d.clone() {
                Some((path, last)) if path == hardware.name && runtime >= last => {
                    let delta_ns = runtime - last;
                    Some(clamp(delta_ns as f64 / (dt * 1_000_000_000.0) * 100.0, 0.0, 100.0))
                }
                _ => None,
            };
            self.last_v3d = Some((hardware.name.clone(), runtime));
            gpu.usage_percent = usage;
        }
    }

    fn fill_memory(&self, mem: &mut MemoryData) {
        let total = self.system.total_memory();
        let used = self.system.used_memory();
        mem.total_bytes = (total > 0).then_some(total);
        mem.used_bytes = (total > 0).then_some(used);
        mem.percent = (total > 0).then(|| clamp(used as f64 / total as f64 * 100.0, 0.0, 100.0));
    }

    fn fill_swap(&self, swap: &mut MemoryData) {
        let total = self.system.total_swap();
        let used = self.system.used_swap();
        swap.total_bytes = Some(total);
        swap.used_bytes = Some(used);
        swap.percent = (total > 0).then(|| clamp(used as f64 / total as f64 * 100.0, 0.0, 100.0));
    }

    fn fill_network(&mut self, net: &mut NetworkData, dt: f64) {
        let mut rx = 0u64;
        let mut tx = 0u64;
        let mut iface = String::new();
        for (name, data) in self.networks.iter() {
            let name: &str = name;
            if is_virtual_interface(&name) {
                continue;
            }
            rx = rx.saturating_add(data.total_received());
            tx = tx.saturating_add(data.total_transmitted());
            if iface.is_empty() {
                iface = name.to_string();
            }
        }
        net.interface = iface;
        net.total_rx_bytes = (rx > 0 || !net.interface.is_empty()).then_some(rx);
        net.total_tx_bytes = (tx > 0 || !net.interface.is_empty()).then_some(tx);

        if let Some((last_rx, last_tx)) = self.last_network {
            if rx >= last_rx && tx >= last_tx && dt > 0.0 {
                net.rx_bytes_per_sec = Some((rx - last_rx) as f64 / dt);
                net.tx_bytes_per_sec = Some((tx - last_tx) as f64 / dt);
            }
        }
        self.last_network = Some((rx, tx));
    }

    fn fill_disk(&mut self, disk: &mut DiskData, dt: f64) {
        let Some((read, write)) = linux::read_disk_totals() else {
            return;
        };
        disk.device_count = count_whole_disks();
        if let Some((last_read, last_write)) = self.last_disk {
            if read >= last_read && write >= last_write && dt > 0.0 {
                disk.read_bytes_per_sec = Some((read - last_read) as f64 / dt);
                disk.write_bytes_per_sec = Some((write - last_write) as f64 / dt);
            }
        }
        self.last_disk = Some((read, write));
    }
}

fn is_virtual_interface(name: &str) -> bool {
    name == "lo"
        || name.starts_with("veth")
        || name.starts_with("docker")
        || name.starts_with("br-")
        || name.starts_with("virbr")
        || name.starts_with("tun")
        || name.starts_with("tap")
        || name.starts_with("dummy")
}

fn count_whole_disks() -> usize {
    std::fs::read_to_string("/proc/diskstats")
        .map(|text| {
            text.lines()
                .filter_map(|line| line.split_whitespace().nth(2))
                .filter(|name| linux::is_whole_disk(name))
                .count()
        })
        .unwrap_or(0)
}
