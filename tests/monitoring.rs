//! Integration tests for real sensor parsing and snapshot sanity.

use amo_monitor::monitor::linux;
use amo_monitor::monitor::Monitor;

#[test]
fn parses_real_proc_diskstats_shape() {
    let sample = "   8       0 sda 12000 44 960000 800 9000 12 720000 600 0 500 700\n\
                   8       1 sda1 11000 40 900000 700 8000 10 700000 500 0 400 600\n\
                 179       0 mmcblk0 200 0 16000 20 100 0 8000 10 0 0 0\n\
                   7       0 loop0 99 0 792 9 0 0 0 0 0 0 0\n";
    let (read, write) = linux::parse_diskstats(sample).expect("whole disks present");
    assert_eq!(read, (960_000 + 16_000) * 512);
    assert_eq!(write, (720_000 + 8_000) * 512);
}

#[test]
fn missing_or_invalid_sensor_data_is_not_fabricated() {
    assert_eq!(linux::parse_diskstats("garbage"), None);
    assert_eq!(linux::parse_v3d_runtime("no header here"), None);
    // A valid V3D counter must be monotonic; a single reading only tells us
    // whether the platform exposes one, and both outcomes are acceptable.
    let _ = linux::read_v3d_runtime();
}

#[test]
fn v3d_busy_counter_parses_nanoseconds() {
    let sample = "queue\ttimestamp\tjobs\truntime\n\
                  bin\t10\t0\t5000\n\
                  render\t10\t3\t123456789\n";
    assert_eq!(linux::parse_v3d_runtime(sample), Some(123_456_789));
}

#[test]
fn live_snapshot_contains_only_plausible_values() {
    let monitor = Monitor::new();
    let s = &monitor.snapshot;
    assert!(s.cpu.cores > 0, "CPU count must be discovered from the system");
    assert!(!s.cpu.model.is_empty(), "CPU model must be read from the system");
    for pct in [
        s.cpu.usage_percent,
        s.gpu.usage_percent,
        s.memory.percent,
        s.swap.percent,
    ] {
        if let Some(v) = pct {
            assert!(v.is_finite() && (0.0..=100.0).contains(&v), "invalid percentage: {v}");
        }
    }
    for rate in [
        s.network.rx_bytes_per_sec,
        s.network.tx_bytes_per_sec,
        s.disk.read_bytes_per_sec,
        s.disk.write_bytes_per_sec,
        s.cpu.power_w,
        s.gpu.power_w,
    ] {
        if let Some(v) = rate {
            assert!(v.is_finite() && v >= 0.0, "invalid rate: {v}");
        }
    }
    assert!(s.system.uptime_secs.unwrap_or(0) > 0, "uptime must be real");
    // No GPU sensor is available on the CI host, so GPU usage must be None
    // rather than a made-up number.
    if s.gpu.name.is_empty() {
        assert!(s.gpu.usage_percent.is_none());
    }
}

#[test]
fn uptime_is_read_from_proc() {
    let uptime = linux::read_uptime().expect("/proc/uptime is available on Linux");
    assert!(uptime > 0);
}
