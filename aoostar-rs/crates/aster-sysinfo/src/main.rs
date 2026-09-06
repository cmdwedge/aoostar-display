// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use clap::Parser;
use env_logger::Env;
use itertools::Itertools;
use log::{debug, error, info};
use regex::Regex;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs;
use std::io::{BufWriter, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};
use std::thread::sleep;
use std::time::{Duration, Instant};
use sysinfo::{Components, DiskKind, Disks, Networks, System};
use tempfile::Builder;

/// Proof of concept sensor value collection for the asterctl screen control tool.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Output sensor file.
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Temporary directory for preparing the output sensor file.
    ///
    /// The system temp directory is used if not specified.
    /// The temp directory must be on the same file system for atomic rename operation!
    #[arg(short, long)]
    temp_dir: Option<PathBuf>,

    /// Print values in console
    #[arg(long)]
    console: bool,

    /// System sensor refresh interval in seconds
    #[arg(short, long)]
    refresh: Option<u16>,

    /// Enable individual disk refresh logic as used in AOOSTAR-X. Refresh interval in seconds.
    #[arg(long)]
    disk_refresh: Option<u16>,

    /// Retrieve drive temperature if `disk-update` option is enabled.
    ///
    /// Requires smartctl and password-less sudo!
    #[cfg(target_os = "linux")]
    #[arg(long)]
    smartctl: bool,

    /// Optional settings file path (e.g. /config/settings.json)
    #[arg(long)]
    settings: Option<PathBuf>,

    /// Override primary network interface (e.g. eth1, br0)
    #[arg(long)]
    network_interface: Option<String>,

    /// Temperature unit: C or F
    #[arg(long)]
    temp_unit: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = Args::parse();
    #[cfg(target_os = "linux")]
    let use_smartctl = args.smartctl;
    #[cfg(not(target_os = "linux"))]
    let use_smartctl = false;

    if let Some(out_file) = &args.out
        && let Some(parent) = out_file.parent()
    {
        fs::create_dir_all(parent)?;
    }
    let mut sensors = HashMap::with_capacity(64);
    let mut sysinfo_source = SysinfoSource::new(args.settings, args.network_interface, args.temp_unit);
    let is_f = sysinfo_source.temp_unit_is_f;

    let refresh = Duration::from_secs(args.refresh.unwrap_or_default() as u64);

    let disk_refresh = Duration::from_secs(args.disk_refresh.unwrap_or_default() as u64);
    let mut disk_refresh_time = Instant::now();
    if !disk_refresh.is_zero() {
        update_linux_storage_sensors(&mut sensors, use_smartctl, is_f)?;
    }

    if !refresh.is_zero() {
        info!(
            "Starting aster-sysinfo with refresh={}ms (temp_unit={})",
            refresh.as_millis(),
            if is_f { "F" } else { "C" }
        );
    }

    loop {
        let upd_start_time = Instant::now();

        sysinfo_source.refresh();
        sysinfo_source.update_sensors(&mut sensors)?;

        if !disk_refresh.is_zero() && disk_refresh_time.elapsed() > disk_refresh {
            debug!("Refreshing individual disks");
            update_linux_storage_sensors(&mut sensors, use_smartctl, is_f)?;
            disk_refresh_time = Instant::now();
        }

        if let Some(out_file) = &args.out {
            write_sensor_file(out_file, args.temp_dir.as_deref(), &sensors)?;
        }

        if args.console {
            // pretty print console output with sorted keys
            for (label, value) in sensors.iter().sorted() {
                println!("{}: {}", label, value);
            }
            println!();
        }

        if refresh.is_zero() {
            break;
        }

        let elapsed = upd_start_time.elapsed();
        if refresh > elapsed {
            sleep(refresh - elapsed);
        }
    }

    Ok(())
}

fn write_sensor_file(
    out_file: &Path,
    temp_dir: Option<&Path>,
    sensors: &HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if out_file.is_dir() {
        error!("Output cannot be a directory: {}", out_file.display());
        exit(1);
    }

    // make sure our sensor file can be read by everyone on unix
    #[allow(unused_mut)]
    let mut builder = Builder::new();
    #[cfg(unix)]
    {
        let all_read_perm = fs::Permissions::from_mode(0o664);
        builder.permissions(all_read_perm);
    }
    let tmp_file = if let Some(temp_path) = temp_dir {
        fs::create_dir_all(temp_path)?;

        debug!("Creating a new named temp file in {temp_path:?}");
        builder.tempfile_in(temp_path)?
    } else {
        debug!("Creating a new named temp file");
        builder.tempfile()?
    };

    debug!("Writing sensor temp file...");
    let mut stream = BufWriter::new(&tmp_file);

    for (label, value) in sensors.iter() {
        writeln!(stream, "{label}: {value}")?;
    }

    stream.flush()?;
    drop(stream);
    debug!("Renaming temp file to: {out_file:?}");
    tmp_file.persist(out_file)?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct NetDevStats {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

pub fn read_proc_net_dev(path: &Path) -> Vec<NetDevStats> {
    let mut stats = Vec::new();
    let Ok(content) = fs::read_to_string(path) else {
        return stats;
    };

    for line in content.lines() {
        let line = line.trim();
        if !line.contains(':') {
            continue;
        }
        let Some((iface, data)) = line.split_once(':') else {
            continue;
        };
        let iface = iface.trim().to_string();
        let cols: Vec<&str> = data.split_whitespace().collect();
        if cols.len() >= 9 {
            let rx_bytes = cols[0].parse::<u64>().unwrap_or(0);
            let tx_bytes = cols[8].parse::<u64>().unwrap_or(0);
            stats.push(NetDevStats {
                name: iface,
                rx_bytes,
                tx_bytes,
            });
        }
    }

    stats
}

pub fn get_host_unraid_ip() -> Option<String> {
    let var_ini = Path::new("/var/local/emhttp/var.ini");
    if let Ok(content) = fs::read_to_string(var_ini) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("IPADDR=") || line.starts_with("IPADDR:0=") {
                if let Some((_, val)) = line.split_once('=') {
                    let ip = val.trim().trim_matches('"').trim();
                    if !ip.is_empty() && ip != "0.0.0.0" {
                        return Some(ip.to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn read_setting_from_file(path: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    for line in content.lines() {
        if line.contains(key) {
            if let Some((_, val)) = line.split_once(':') {
                let trimmed = val.trim().trim_matches(',').trim_matches('"').trim();
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn format_temperature(celsius: f32, is_fahrenheit: bool) -> String {
    if is_fahrenheit {
        format!("{:.0}", (celsius * 1.8 + 32.0).round())
    } else {
        format!("{:.0}", celsius.round())
    }
}

pub fn is_temp_unit_fahrenheit() -> bool {
    if let Ok(unit) = std::env::var("TEMP_UNIT") {
        if unit.to_uppercase().starts_with('F') {
            return true;
        }
    }
    if let Some(val) = read_setting_from_file(Path::new("/config/settings.json"), "tempUnit") {
        if val.to_uppercase().starts_with('F') {
            return true;
        }
    }
    false
}

pub fn get_configured_network_interface() -> Option<String> {
    if let Ok(iface) = std::env::var("NETWORK_INTERFACE") {
        let trimmed = iface.trim();
        if !trimmed.is_empty() && trimmed != "auto" {
            return Some(trimmed.to_string());
        }
    }
    if let Some(val) = read_setting_from_file(Path::new("/config/settings.json"), "interface") {
        let trimmed = val.trim();
        if !trimmed.is_empty() && trimmed != "auto" {
            return Some(trimmed.to_string());
        }
    }
    None
}

pub struct SysinfoSource {
    sys: System,
    disks: Disks,
    components: Components,
    networks: Networks,
    last_refresh: Option<Instant>,
    refresh_duration: Option<Duration>,
    prev_host_traffic: HashMap<String, (u64, u64, Instant)>,
    pub temp_unit_is_f: bool,
    pub configured_interface: Option<String>,
    last_reported_iface: Option<String>,
}

impl Default for SysinfoSource {
    fn default() -> Self {
        Self::new(None, None, None)
    }
}

impl SysinfoSource {
    pub fn new(
        settings_path: Option<PathBuf>,
        forced_network_iface: Option<String>,
        temp_unit: Option<String>,
    ) -> Self {
        let effective_settings = settings_path.unwrap_or_else(|| PathBuf::from("/config/settings.json"));
        let temp_unit_is_f = if let Some(u) = temp_unit {
            u.to_uppercase().starts_with('F')
        } else if let Ok(u) = std::env::var("TEMP_UNIT") {
            u.to_uppercase().starts_with('F')
        } else if let Some(u) = read_setting_from_file(&effective_settings, "tempUnit") {
            u.to_uppercase().starts_with('F')
        } else {
            false
        };

        let configured_interface = forced_network_iface.or_else(|| {
            if let Ok(i) = std::env::var("NETWORK_INTERFACE") {
                let t = i.trim();
                if !t.is_empty() && t != "auto" {
                    return Some(t.to_string());
                }
            }
            if let Some(i) = read_setting_from_file(&effective_settings, "interface") {
                let t = i.trim();
                if !t.is_empty() && t != "auto" {
                    return Some(t.to_string());
                }
            }
            None
        });

        Self {
            sys: System::new_all(),
            disks: Disks::new(),
            components: Components::new(),
            networks: Networks::new(),
            last_refresh: None,
            refresh_duration: None,
            prev_host_traffic: HashMap::new(),
            temp_unit_is_f,
            configured_interface,
            last_reported_iface: None,
        }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_all();
        debug!("Refreshing disks, components, networks");
        self.disks.refresh(false);
        self.components.refresh(false);
        self.networks.refresh(false);

        if let Some(last_refresh) = self.last_refresh {
            self.refresh_duration = Some(last_refresh.elapsed());
        }
        self.last_refresh = Some(Instant::now());
    }

    fn update_sensors(
        &mut self,
        sensors: &mut HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Refreshing sensors");
        for cpu in self.sys.cpus() {
            add_sensor(
                sensors,
                format!("cpu_{}_frequency", cpu.name()),
                cpu.frequency(),
            );
            add_sensor(
                sensors,
                format!("cpu_{}_usage", cpu.name()),
                format!("{:.2}", cpu.cpu_usage()),
            );
        }

        add_sensor(
            sensors,
            "cpu_usage_percent".to_string(),
            format!("{:.2}", self.sys.global_cpu_usage()),
        );

        let load_avg = System::load_average();
        add_sensor(sensors, "load_avg_one", format!("{:.2}", load_avg.one));
        add_sensor(sensors, "load_avg_five", format!("{:.2}", load_avg.five));
        add_sensor(
            sensors,
            "load_avg_fifteen",
            format!("{:.2}", load_avg.fifteen),
        );

        // RAM and swap information:
        add_sensor(sensors, "mem_free_bytes", self.sys.free_memory());
        add_sensor(sensors, "mem_free", format_bytes(self.sys.free_memory()));
        add_sensor(sensors, "mem_total_bytes", self.sys.total_memory());
        add_sensor(sensors, "mem_total", format_bytes(self.sys.total_memory()));
        add_sensor(sensors, "mem_used_bytes", self.sys.used_memory());
        add_sensor(sensors, "mem_used", format_bytes(self.sys.used_memory()));
        add_sensor(
            sensors,
            "mem_usage_percent",
            format!(
                "{:.1}",
                (self.sys.used_memory() * 100) as f64 / self.sys.total_memory() as f64
            ),
        );

        add_sensor(sensors, "swap_free_bytes", self.sys.free_swap());
        add_sensor(sensors, "swap_free", format_bytes(self.sys.free_swap()));
        add_sensor(sensors, "swap_total_bytes", self.sys.total_swap());
        add_sensor(sensors, "swap_total", format_bytes(self.sys.total_swap()));
        add_sensor(sensors, "swap_used_bytes", self.sys.used_swap());
        add_sensor(sensors, "swap_used", format_bytes(self.sys.used_swap()));
        add_sensor(
            sensors,
            "swap_usage_percent",
            format!(
                "{:.1}",
                (self.sys.used_swap() * 100) as f64 / self.sys.total_swap() as f64
            ),
        );

        // System information:
        let up_secs = System::uptime();
        let up_days = up_secs / 86400;
        let up_hours = (up_secs - (up_days * 86400)) / 3600;
        let up_mins = (up_secs - (up_days * 86400) - (up_hours * 3600)) / 60;
        add_sensor(sensors, "system_uptime_sec", up_secs);
        /*
        Time to look into ftl for i18n
        The coreutils project did a lot of work that could be used:
        https://github.com/uutils/coreutils/blob/main/src/uucore/src/lib/mods/locale.rs
        Then this would be the easy way to format the time, just uses a lot of setup code:

        uptime-format = { $days ->
            [0] { $time }
            [one] { $days } day, { $time }
           *[other] { $days } days { $time }
        }

        translate!(
            "uptime-format",
            "days" => up_days,
            "time" => format!("{up_hours:02}:{up_mins:02}")
        )
         */
        let day_string = match up_days {
            0 => "",
            1 => "1 day, ",
            n => &format!("{n} days "),
        };
        add_sensor(
            sensors,
            "system_uptime",
            format!("{day_string}{up_hours:02}:{up_mins:02}"),
        );

        if let Some(name) = System::name() {
            add_sensor(sensors, "system_name", name);
        }
        if let Some(kernel_version) = System::kernel_version() {
            add_sensor(sensors, "system_kernel_version", kernel_version);
        }
        if let Some(os_version) = System::os_version() {
            add_sensor(sensors, "system_os_version", os_version);
        }
        if let Some(host_name) = System::host_name() {
            add_sensor(sensors, "system_hostname", host_name);
        }

        add_sensor(sensors, "cpu_count", self.sys.cpus().len());
        add_sensor(sensors, "total_processes", self.sys.processes().len());

        // disks' information:
        let mut ssd_idx = 0;
        let mut hdd_idx = 0;
        for disk in &self.disks {
            let label;
            match disk.kind() {
                DiskKind::SSD => {
                    label = format!("storage_ssd[{}]", ssd_idx);
                    ssd_idx += 1;
                }
                DiskKind::HDD => {
                    label = format!("storage_hdd[{}]", hdd_idx);
                    hdd_idx += 1;
                }
                _ => continue,
            }
            // special label for AOOSTAR-X system panel
            add_sensor(
                sensors,
                format!("{label}_usage_percent"),
                (disk.total_space() - disk.available_space()) * 100 / disk.total_space(),
            );

            // using similar labels as AOOSTAR-X, but combining `{label2}_{label}`
            let device = disk.name().to_string_lossy().replace(' ', "_");
            add_sensor(
                sensors,
                format!("disk_{device}_total_bytes"),
                disk.total_space(),
            );
            add_sensor(
                sensors,
                format!("disk_{device}_total"),
                format_bytes(disk.total_space()),
            );
            let used = disk.total_space() - disk.available_space();
            add_sensor(sensors, format!("disk_{device}_used_bytes"), used);
            add_sensor(sensors, format!("disk_{device}_used"), format_bytes(used));
            add_sensor(
                sensors,
                format!("disk_{device}_free_bytes"),
                disk.available_space(),
            );
            add_sensor(
                sensors,
                format!("disk_{device}_free"),
                format_bytes(disk.available_space()),
            );
            add_sensor(
                sensors,
                format!("disk_{device}_usage_percent"),
                format!(
                    "{:.1}",
                    (disk.total_space() - disk.available_space()) as f64 * 100.0
                        / disk.total_space() as f64
                ),
            );
        }

        // Components temperature:
        let mut nvme_component_idx = 0;
        for component in &self.components {
            if let Some(temperature) = component.temperature() {
                let label;
                let c_label = component.label();
                if c_label.contains("spd5118") {
                    label = "temperature_memory".to_string();
                } else if c_label.contains("amdgpu") {
                    label = "temperature_gpu".to_string();
                } else if c_label.contains("Tctl")
                    || c_label.contains("Tdie")
                    || c_label.contains("Package id 0")
                    || c_label.contains("Core 0")
                    || c_label.contains("cpu")
                    || c_label.contains("CPU")
                {
                    label = "temperature_cpu".to_string();
                } else if c_label.contains("Composite") && !c_label.contains("nvme") {
                    label = "temperature_motherboard".to_string();
                } else if c_label.contains("Motherboard")
                    || c_label.contains("System")
                    || c_label.contains("temp1")
                {
                    label = "temperature_motherboard".to_string();
                } else {
                    label = format!("temperature_{}", c_label.replace(' ', "_"));
                }

                let unit_str = if self.temp_unit_is_f { "°F" } else { "°C" };
                add_sensor(sensors, format!("{label}#unit"), unit_str);
                add_sensor(sensors, label.clone(), format_temperature(temperature, self.temp_unit_is_f));

                let disp_temp = format_temperature(temperature, self.temp_unit_is_f);
                // Direct convenience aliases matching Aoostar standard names
                if label == "temperature_cpu" {
                    add_sensor(sensors, "cpu_temperature", &disp_temp);
                } else if label == "temperature_gpu" {
                    add_sensor(sensors, "gpu_temperature", &disp_temp);
                } else if label == "temperature_memory" {
                    add_sensor(sensors, "memory_Temperature", &disp_temp);
                } else if label == "temperature_motherboard" {
                    add_sensor(sensors, "motherboard_temperature", &disp_temp);
                }

                // If this is an NVMe temperature component, take only the primary Composite sensor per physical drive
                if c_label.contains("Composite") && c_label.to_lowercase().contains("nvme") {
                    add_sensor(sensors, format!("storage_ssd[{nvme_component_idx}]_temperature"), &disp_temp);
                    add_sensor(sensors, format!("storage_ssd[{nvme_component_idx}]['temperature']"), &disp_temp);
                    nvme_component_idx += 1;
                }
            }
        }

        add_sensor(sensors, "temperature_unit", if self.temp_unit_is_f { "℉" } else { "℃" });

        // Fallback for motherboard temperature if not discovered above
        if !sensors.contains_key("motherboard_temperature") {
            if let Some(mb_temp) = get_fallback_motherboard_temp() {
                let disp_mb = format_temperature(mb_temp, self.temp_unit_is_f);
                add_sensor(sensors, "motherboard_temperature", &disp_mb);
                add_sensor(sensors, "temperature_motherboard", &disp_mb);
            } else if let Some(cpu_temp) = sensors.get("cpu_temperature").cloned() {
                add_sensor(sensors, "motherboard_temperature", &cpu_temp);
                add_sensor(sensors, "temperature_motherboard", &cpu_temp);
            }
        }

        // Memory usage in GB and fallback for memory_Temperature if no SPD temp sensor
        // Memory usage in GB (e.g. 5.8G/64G) for RAM sub-bubble
        let used_gb = self.sys.used_memory() as f64 / 1_073_741_824.0;
        let total_gb = self.sys.total_memory() as f64 / 1_073_741_824.0;
        let mem_summary = format!("{:.1}G/{:.0}G", used_gb, total_gb);
        add_sensor(sensors, "mem_used_gb", format!("{:.1}G", used_gb));
        add_sensor(sensors, "mem_used_summary", &mem_summary);
        add_sensor(sensors, "memory_Temperature", &mem_summary);
        if !sensors.contains_key("temperature_memory") {
            add_sensor(sensors, "temperature_memory", &mem_summary);
        }

        // GPU load percentage from /sys/class/drm
        let gpu_load = get_gpu_busy_percent().unwrap_or(0);
        add_sensor(sensors, "gpu_core", gpu_load);
        add_sensor(sensors, "gpu_usage_percent", gpu_load);

        // Network interfaces & throughput:
        // Support bridge mode by checking if host procfs is mounted at /host/proc/net/dev
        let host_proc_net = Path::new("/host/proc/net/dev");
        let host_devs = if host_proc_net.exists() {
            read_proc_net_dev(host_proc_net)
        } else {
            Vec::new()
        };

        let configured_iface = self.configured_interface.clone().or_else(get_configured_network_interface);
        let host_lan_ip = get_host_unraid_ip();

        let mut best_interface: Option<(String, u64, Option<String>, u64)> = None;

        if !host_devs.is_empty() {
            debug!("Reading physical host network statistics from /host/proc/net/dev (bridge network mode)");
            for net_dev in host_devs {
                let if_name = net_dev.name.to_lowercase();
                if if_name == "lo"
                    || if_name.starts_with("docker")
                    || if_name.starts_with("veth")
                    || if_name.starts_with("virbr")
                    || if_name.starts_with("tun")
                    || if_name.starts_with("tap")
                {
                    continue;
                }

                let is_supported = ["eth", "en", "em", "br", "bond", "wlan", "wlp", "wlo", "vlan"]
                    .iter()
                    .any(|prefix| if_name.starts_with(prefix));

                if !is_supported {
                    continue;
                }

                let now = Instant::now();
                let (rx_speed, tx_speed) = if let Some(&(prev_rx, prev_tx, prev_time)) = self.prev_host_traffic.get(&net_dev.name) {
                    let interval_ms = prev_time.elapsed().as_millis() as u64;
                    if interval_ms > 0 {
                        let rx = if net_dev.rx_bytes >= prev_rx { (net_dev.rx_bytes - prev_rx) * 1000 / interval_ms } else { 0 };
                        let tx = if net_dev.tx_bytes >= prev_tx { (net_dev.tx_bytes - prev_tx) * 1000 / interval_ms } else { 0 };
                        (rx, tx)
                    } else {
                        (0, 0)
                    }
                } else {
                    (0, 0)
                };
                self.prev_host_traffic.insert(net_dev.name.clone(), (net_dev.rx_bytes, net_dev.tx_bytes, now));

                add_sensor(
                    sensors,
                    format!("network_{}_download_speed", net_dev.name),
                    format_network_speed(rx_speed),
                );
                add_sensor(
                    sensors,
                    format!("network_{}_upload_speed", net_dev.name),
                    format_network_speed(tx_speed),
                );
                add_sensor(
                    sensors,
                    format!("network_{}_total_received_bytes", net_dev.name),
                    net_dev.rx_bytes,
                );
                add_sensor(
                    sensors,
                    format!("network_{}_total_received", net_dev.name),
                    format_bytes(net_dev.rx_bytes),
                );
                add_sensor(
                    sensors,
                    format!("network_{}_total_transmitted_bytes", net_dev.name),
                    net_dev.tx_bytes,
                );
                add_sensor(
                    sensors,
                    format!("network_{}_total_transmitted", net_dev.name),
                    format_bytes(net_dev.tx_bytes),
                );

                let total_traffic = net_dev.rx_bytes + net_dev.tx_bytes;
                let current_bps = rx_speed + tx_speed;

                let active_bonus: u64 = if current_bps > 0 { 2_000_000_000_000 } else { 0 };
                let score: u64 = if let Some(ref forced) = configured_iface {
                    if net_dev.name == *forced || if_name == forced.to_lowercase() {
                        90_000_000_000_000
                    } else {
                        total_traffic
                    }
                } else if if_name == "br0" {
                    10_000_000_000_000 + active_bonus + total_traffic
                } else if if_name == "bond0" {
                    8_000_000_000_000 + active_bonus + total_traffic
                } else if if_name.starts_with("eth") || if_name.starts_with("en") {
                    6_000_000_000_000 + active_bonus + total_traffic
                } else {
                    1_000_000_000_000 + active_bonus + total_traffic
                };

                match &best_interface {
                    None => {
                        best_interface = Some((net_dev.name.clone(), score, host_lan_ip.clone(), current_bps));
                    }
                    Some((_, best_score, _, _)) if score > *best_score => {
                        best_interface = Some((net_dev.name.clone(), score, host_lan_ip.clone(), current_bps));
                    }
                    _ => {}
                }
            }
        } else {
            // Standard fallback using sysinfo::Networks (when container runs with --net=host)
            for (interface_name, data) in &self.networks {
                let if_name = interface_name.to_lowercase();
                if if_name == "lo"
                    || if_name.starts_with("docker")
                    || if_name.starts_with("veth")
                    || if_name.starts_with("virbr")
                    || if_name.starts_with("tun")
                    || if_name.starts_with("tap")
                {
                    continue;
                }

                let is_supported = ["eth", "en", "em", "br", "bond", "wlan", "wlp", "wlo", "vlan"]
                    .iter()
                    .any(|prefix| if_name.starts_with(prefix));

                if !is_supported {
                    continue;
                }

                let mut first_ip: Option<String> = host_lan_ip.clone();
                for (idx, addr) in data
                    .ip_networks()
                    .iter()
                    .map(|net| net.addr)
                    .sorted()
                    .enumerate()
                {
                    if first_ip.is_none() && !addr.is_loopback() && !addr.is_unspecified() {
                        first_ip = Some(addr.to_string());
                    }
                    add_sensor(
                        sensors,
                        format!("network_{interface_name}_address{idx}"),
                        addr,
                    );
                }

                let total_traffic = data.total_received() + data.total_transmitted();
                let mut current_bps = 0;

                if let Some(refresh) = self.refresh_duration {
                    let interval = refresh.as_millis() as u64;
                    if interval > 0 {
                        let rx = 1000 * data.received() / interval;
                        let tx = 1000 * data.transmitted() / interval;
                        current_bps = rx + tx;
                        add_sensor(
                            sensors,
                            format!("network_{interface_name}_download_speed"),
                            format_network_speed(rx),
                        );
                        add_sensor(
                            sensors,
                            format!("network_{interface_name}_upload_speed"),
                            format_network_speed(tx),
                        );
                    }
                }

                add_sensor(
                    sensors,
                    format!("network_{interface_name}_total_received_bytes"),
                    data.total_received(),
                );
                add_sensor(
                    sensors,
                    format!("network_{interface_name}_total_received"),
                    format_bytes(data.total_received()),
                );
                add_sensor(
                    sensors,
                    format!("network_{interface_name}_total_transmitted_bytes"),
                    data.total_transmitted(),
                );
                add_sensor(
                    sensors,
                    format!("network_{interface_name}_total_transmitted"),
                    format_bytes(data.total_transmitted()),
                );

                let has_ip = first_ip.is_some();
                let active_bonus: u64 = if current_bps > 0 { 2_000_000_000_000 } else { 0 };

                let score: u64 = if let Some(ref forced) = configured_iface {
                    if interface_name == forced || if_name == forced.to_lowercase() {
                        90_000_000_000_000
                    } else {
                        total_traffic
                    }
                } else if !has_ip {
                    // Interfaces without an IP address should never beat an interface with an IP address!
                    total_traffic
                } else if if_name == "br0" {
                    10_000_000_000_000 + active_bonus + total_traffic
                } else if if_name == "bond0" {
                    8_000_000_000_000 + active_bonus + total_traffic
                } else if if_name.starts_with("eth") || if_name.starts_with("en") {
                    6_000_000_000_000 + active_bonus + total_traffic
                } else {
                    1_000_000_000_000 + active_bonus + total_traffic
                };

                match &best_interface {
                    None => {
                        best_interface = Some((interface_name.clone(), score, first_ip, current_bps));
                    }
                    Some((_, best_score, _, _)) if score > *best_score => {
                        best_interface = Some((interface_name.clone(), score, first_ip, current_bps));
                    }
                    _ => {}
                }
            }
        }

        // Export primary / default network interface aliases
        if let Some((best_if_name, _, best_ip, current_bps)) = best_interface {
            let iface_changed = match &self.last_reported_iface {
                Some(prev) => prev != &best_if_name,
                None => true,
            };
            if iface_changed {
                info!(
                    "Active network monitor: interface='{best_if_name}', IP='{}'",
                    best_ip.as_deref().unwrap_or("none")
                );
                self.last_reported_iface = Some(best_if_name.clone());
            }

            add_sensor(sensors, "net_interface", best_if_name.clone());
            if let Some(ip) = best_ip {
                add_sensor(sensors, "net_ip_address", ip.clone());
                add_sensor(sensors, "net_default_ip_address", ip);
            }
            if let Some(down_speed) = sensors.get(&format!("network_{best_if_name}_download_speed")).cloned() {
                add_sensor(sensors, "net_download_speed", down_speed.clone());
                add_sensor(sensors, "net_default_download_speed", down_speed);
            }
            if let Some(up_speed) = sensors.get(&format!("network_{best_if_name}_upload_speed")).cloned() {
                add_sensor(sensors, "net_upload_speed", up_speed.clone());
                add_sensor(sensors, "net_default_upload_speed", up_speed);
            }
            add_sensor(sensors, "net_default_bytes_per_sec", current_bps);
        }

        Ok(())
    }
}

fn add_sensor(
    sensors: &mut HashMap<String, String>,
    label: impl Into<String>,
    value: impl Display,
) {
    sensors.insert(label.into(), value.to_string());
}

fn get_gpu_busy_percent() -> Option<u32> {
    for card in 0..4 {
        let path = format!("/sys/class/drm/card{card}/device/gpu_busy_percent");
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(val) = content.trim().parse::<u32>() {
                return Some(val);
            }
        }
    }
    None
}

fn get_fallback_motherboard_temp() -> Option<f32> {
    for zone in 0..10 {
        let type_path = format!("/sys/class/thermal/thermal_zone{zone}/type");
        let temp_path = format!("/sys/class/thermal/thermal_zone{zone}/temp");
        if let Ok(type_str) = fs::read_to_string(&type_path) {
            let t = type_str.to_lowercase();
            if t.contains("acpi") || t.contains("soc") || t.contains("board") || t.contains("x86_pkg") || t.contains("pch") {
                if let Ok(temp_str) = fs::read_to_string(&temp_path) {
                    if let Ok(raw_temp) = temp_str.trim().parse::<f32>() {
                        let temp = if raw_temp > 1000.0 { raw_temp / 1000.0 } else { raw_temp };
                        if temp > 15.0 && temp < 105.0 {
                            return Some(temp);
                        }
                    }
                }
            }
        }
    }
    // General thermal zone fallback if no specific board zone matched
    for zone in 0..10 {
        let temp_path = format!("/sys/class/thermal/thermal_zone{zone}/temp");
        if let Ok(temp_str) = fs::read_to_string(&temp_path) {
            if let Ok(raw_temp) = temp_str.trim().parse::<f32>() {
                let temp = if raw_temp > 1000.0 { raw_temp / 1000.0 } else { raw_temp };
                if temp > 15.0 && temp < 105.0 {
                    return Some(temp);
                }
            }
        }
    }
    None
}

/// Format disk usage compactly to fit within sensor pill widths (e.g. "11.2T/16T" or "226G/2T")
pub fn format_disk_summary(used_bytes: u64, size_bytes: u64) -> String {
    const TB: f64 = 1_099_511_627_776.0;
    const GB: f64 = 1_073_741_824.0;

    let total_tb = size_bytes as f64 / TB;
    let used_tb = used_bytes as f64 / TB;
    let total_gb = size_bytes as f64 / GB;
    let used_gb = used_bytes as f64 / GB;

    if total_tb >= 1.0 {
        if used_tb >= 1.0 {
            format!("{:.1}T/{:.0}T", used_tb, total_tb.round())
        } else {
            format!("{:.0}G/{:.0}T", used_gb.round(), total_tb.round())
        }
    } else if total_gb >= 1.0 {
        format!("{:.0}G/{:.0}G", used_gb.round(), total_gb.round())
    } else {
        format!("{:.0} MB", used_bytes as f64 / 1_048_576.0)
    }
}

fn update_unraid_storage_sensors(sensors: &mut HashMap<String, String>, is_f: bool) -> bool {
    let disks_ini_path = Path::new("/var/local/emhttp/disks.ini");
    if !disks_ini_path.exists() {
        return false;
    }

    let Ok(content) = fs::read_to_string(disks_ini_path) else {
        return false;
    };

    let mut current_section = String::new();
    let mut disk_props: HashMap<String, String> = HashMap::new();
    let mut disks: HashMap<String, HashMap<String, String>> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if !current_section.is_empty() {
                disks.insert(current_section.clone(), disk_props.clone());
                disk_props.clear();
            }
            current_section = line[1..line.len() - 1].replace('"', "").to_string();
        } else if let Some((key, val)) = line.split_once('=') {
            let key = key.trim().to_string();
            let val = val.trim().trim_matches('"').to_string();
            disk_props.insert(key, val);
        }
    }
    if !current_section.is_empty() {
        disks.insert(current_section, disk_props);
    }

    // Unraid Array Disks: map parity to hdd[0], and disk1..disk5 to hdd[1..5]
    let array_disk_keys = ["parity", "disk1", "disk2", "disk3", "disk4", "disk5"];
    let mut array_total_size_bytes: u64 = 0;
    let mut array_total_used_bytes: u64 = 0;
    let mut array_total_free_bytes: u64 = 0;

    let mut total_array_disks = 0;
    let mut active_array_disks = 0;

    for (idx, &disk_key) in array_disk_keys.iter().enumerate() {
        if let Some(props) = disks.get(disk_key) {
            total_array_disks += 1;
            let temp = props.get("temp").map(|s| s.as_str()).unwrap_or("*");
            let spindown = props.get("spindown").map(|s| s.as_str()).unwrap_or("0");
            let device = props.get("device").map(|s| s.as_str()).unwrap_or("");

            let is_standby = temp == "*" || spindown == "1";
            if !is_standby {
                active_array_disks += 1;
            }

            let mut final_temp: Option<String> = None;

            if temp != "*" && !temp.is_empty() {
                final_temp = Some(temp.to_string());
            } else if spindown == "0" && !device.is_empty() {
                // Disk is active in Unraid, query smartctl safely (won't wake if standby)
                if let Ok(Some(t)) = get_smartctl_disk_temperature(device) {
                    final_temp = Some(t.to_string());
                }
            }

            let display_temp = match final_temp {
                Some(t) => {
                    if is_f {
                        if let Ok(c) = t.parse::<f32>() {
                            format!("{:.0}", (c * 1.8 + 32.0).round())
                        } else {
                            t
                        }
                    } else {
                        t
                    }
                }
                None => "Standby".to_string(),
            };

            add_sensor(sensors, format!("storage_hdd[{idx}]_temperature"), &display_temp);
            add_sensor(sensors, format!("storage_hdd[{idx}]['temperature']"), &display_temp);

            // fsSize, fsFree, fsUsed in disks.ini are in KiB (1024-byte blocks)
            let fs_size_kb = props.get("fsSize").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let fs_free_kb = props.get("fsFree").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let fs_used_kb = if let Some(used) = props.get("fsUsed").and_then(|s| s.parse::<u64>().ok()) {
                used
            } else if fs_size_kb >= fs_free_kb && fs_size_kb > 0 {
                fs_size_kb - fs_free_kb
            } else {
                0
            };

            let raw_size_kb = props.get("size").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);

            let (size_bytes, used_bytes, usage_percent) = if fs_size_kb > 0 {
                let size_b = fs_size_kb * 1024;
                let used_b = fs_used_kb * 1024;
                let pct = ((fs_used_kb as f64 / fs_size_kb as f64) * 100.0).round();
                (size_b, used_b, pct)
            } else if raw_size_kb > 0 {
                // Parity disk or unformatted raw array disk
                let size_b = raw_size_kb * 1024;
                (size_b, size_b, 100.0)
            } else {
                (0, 0, 0.0)
            };

            if disk_key != "parity" && fs_size_kb > 0 {
                array_total_size_bytes += size_bytes;
                array_total_used_bytes += used_bytes;
                array_total_free_bytes += fs_free_kb * 1024;
            }

            add_sensor(sensors, format!("storage_hdd[{idx}]_usage_percent"), usage_percent);
            add_sensor(sensors, format!("storage_hdd[{idx}]['used']"), usage_percent);
            add_sensor(sensors, format!("storage_hdd[{idx}]_total_size_bytes"), size_bytes);
            add_sensor(sensors, format!("storage_hdd[{idx}]_total_size"), format_bytes(size_bytes));
            add_sensor(sensors, format!("storage_hdd[{idx}]_total_used_bytes"), used_bytes);
            add_sensor(sensors, format!("storage_hdd[{idx}]_total_used"), format_bytes(used_bytes));

            let summary_str = if disk_key == "parity" {
                format_bytes(size_bytes)
            } else if size_bytes > 0 {
                format_disk_summary(used_bytes, size_bytes)
            } else {
                "".to_string()
            };
            add_sensor(sensors, format!("storage_hdd[{idx}]_summary"), &summary_str);
            add_sensor(sensors, format!("storage_hdd[{idx}]_used_str"), format_bytes(used_bytes));
        }
    }

    // Export array standby status for screen power management
    let all_disks_standby = total_array_disks > 0 && active_array_disks == 0;
    add_sensor(sensors, "unraid_all_disks_standby", all_disks_standby);
    add_sensor(sensors, "unraid_active_disks", active_array_disks);
    add_sensor(sensors, "unraid_total_disks", total_array_disks);

    // Unraid Array Aggregate Metrics for the MOBO slot on Page 3
    if array_total_size_bytes > 0 {
        let used_tb = array_total_used_bytes as f64 / 1_099_511_627_776.0;
        let total_tb = array_total_size_bytes as f64 / 1_099_511_627_776.0;
        let array_summary = format!("{:.1}T/{:.0}T", used_tb, total_tb);
        let array_pct = ((array_total_used_bytes as f64 / array_total_size_bytes as f64) * 100.0).round();
        add_sensor(sensors, "unraid_array_summary", &array_summary);
        add_sensor(sensors, "storage_mobo_summary", &array_summary);
        add_sensor(sensors, "unraid_array_total_size", format_bytes(array_total_size_bytes));
        add_sensor(sensors, "unraid_array_total_used", format_bytes(array_total_used_bytes));
        add_sensor(sensors, "unraid_array_free", format_bytes(array_total_free_bytes));
        add_sensor(sensors, "unraid_array_usage_percent", array_pct);
    }

    // Unraid Cache / Pool Disks: map to storage_ssd[x]
    let mut pool_disks: Vec<&HashMap<String, String>> = Vec::new();
    for (name, props) in &disks {
        if name.starts_with("cache") || props.get("type").map(|s| s.as_str()) == Some("Pool") {
            pool_disks.push(props);
        }
    }
    pool_disks.sort_by_key(|p| p.get("name").cloned().unwrap_or_default());

    for (idx, props) in pool_disks.iter().take(5).enumerate() {
        if let Some(temp) = props.get("temp") {
            if temp != "*" && !temp.is_empty() {
                let disp_temp = if is_f {
                    if let Ok(c) = temp.parse::<f32>() {
                        format!("{:.0}", (c * 1.8 + 32.0).round())
                    } else {
                        temp.to_string()
                    }
                } else {
                    temp.to_string()
                };
                add_sensor(sensors, format!("storage_ssd[{idx}]_temperature"), &disp_temp);
                add_sensor(sensors, format!("storage_ssd[{idx}]['temperature']"), &disp_temp);
            }
        }
        let fs_size_kb = props.get("fsSize").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let fs_free_kb = props.get("fsFree").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let fs_used_kb = if let Some(used) = props.get("fsUsed").and_then(|s| s.parse::<u64>().ok()) {
            used
        } else if fs_size_kb >= fs_free_kb && fs_size_kb > 0 {
            fs_size_kb - fs_free_kb
        } else {
            0
        };

        let raw_size_kb = props.get("size").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let (size_bytes, used_bytes, usage_percent) = if fs_size_kb > 0 {
            let size_b = fs_size_kb * 1024;
            let used_b = fs_used_kb * 1024;
            let pct = ((fs_used_kb as f64 / fs_size_kb as f64) * 100.0).round();
            (size_b, used_b, pct)
        } else if raw_size_kb > 0 {
            let size_b = raw_size_kb * 1024;
            (size_b, 0, 0.0)
        } else {
            (0, 0, 0.0)
        };

        add_sensor(sensors, format!("storage_ssd[{idx}]_usage_percent"), usage_percent);
        add_sensor(sensors, format!("storage_ssd[{idx}]['used']"), usage_percent);
        add_sensor(sensors, format!("storage_ssd[{idx}]_total_size_bytes"), size_bytes);
        add_sensor(sensors, format!("storage_ssd[{idx}]_total_size"), format_bytes(size_bytes));
        add_sensor(sensors, format!("storage_ssd[{idx}]_total_used_bytes"), used_bytes);
        add_sensor(sensors, format!("storage_ssd[{idx}]_total_used"), format_bytes(used_bytes));

        let summary_str = if size_bytes > 0 {
            format_disk_summary(used_bytes, size_bytes)
        } else {
            "".to_string()
        };
        add_sensor(sensors, format!("storage_ssd[{idx}]_summary"), &summary_str);
        add_sensor(sensors, format!("storage_ssd[{idx}]_used_str"), format_bytes(used_bytes));
    }

    true
}

fn update_linux_storage_sensors(
    sensors: &mut HashMap<String, String>,
    use_smartctl: bool,
    is_f: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // If Unraid /var/local/emhttp/disks.ini exists, use it for native array & pool status
    if update_unraid_storage_sensors(sensors, is_f) {
        return Ok(());
    }

    // Generic Linux fallback:
    if let Ok(hdd_devices) = get_storage_devices(StorageDevice::HddOrSsd) {
        debug!("HDD devices : {:?}", hdd_devices);
        for (idx, device) in hdd_devices.iter().enumerate() {
            let usage = get_disk_usage(device)?;
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_total_size_bytes"),
                usage.total_size,
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_total_size"),
                format_bytes(usage.total_size),
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_total_used_bytes"),
                usage.total_used,
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_total_used"),
                format_bytes(usage.total_used),
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_usage_percent"),
                usage.usage_percent,
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]['used']"),
                usage.usage_percent,
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_summary"),
                format_disk_summary(usage.total_used, usage.total_size),
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_used_str"),
                format_bytes(usage.total_used),
            );

            if use_smartctl && let Some(temperature) = get_smartctl_disk_temperature(device)? {
                let disp_temp = if is_f {
                    format!("{:.0}", (temperature as f32 * 1.8 + 32.0).round())
                } else {
                    format!("{temperature}")
                };
                add_sensor(
                    sensors,
                    format!("storage_hdd[{idx}]_temperature"),
                    &disp_temp,
                );
                add_sensor(
                    sensors,
                    format!("storage_hdd[{idx}]['temperature']"),
                    &disp_temp,
                );
            }
        }
    }

    // AOOSTAR-X: ssd == nvme
    if let Ok(nvme_devices) = get_storage_devices(StorageDevice::Nvme) {
        debug!("NVME devices: {:?}", nvme_devices);
        for (idx, device) in nvme_devices.iter().enumerate() {
            let usage = get_disk_usage(device)?;
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_total_size_bytes"),
                usage.total_size,
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_total_size"),
                format_bytes(usage.total_size),
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_total_used_bytes"),
                usage.total_used,
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_total_used"),
                format_bytes(usage.total_used),
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_usage_percent"),
                usage.usage_percent,
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]['used']"),
                usage.usage_percent,
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_summary"),
                format_disk_summary(usage.total_used, usage.total_size),
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_used_str"),
                format_bytes(usage.total_used),
            );

            if use_smartctl && let Some(temperature) = get_smartctl_disk_temperature(device)? {
                let disp_temp = if is_f {
                    format!("{:.0}", (temperature as f32 * 1.8 + 32.0).round())
                } else {
                    format!("{temperature}")
                };
                add_sensor(
                    sensors,
                    format!("storage_ssd[{idx}]_temperature"),
                    &disp_temp,
                );
                add_sensor(
                    sensors,
                    format!("storage_ssd[{idx}]['temperature']"),
                    &disp_temp,
                );
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct DiskInfo {
    pub device: String,
    pub temperature: i32,
    pub used: f64,
    pub total_used: u64,
    pub total_size: u64,
}

#[derive(Debug)]
pub struct DiskUsage {
    pub usage_percent: f64,
    pub total_used: u64,
    pub total_size: u64,
}

#[derive(Debug, PartialEq)]
pub enum StorageDevice {
    All,
    Hdd,
    Ssd,
    HddOrSsd,
    Nvme,
}

pub type DiskResult = Result<Vec<DiskInfo>, Box<dyn std::error::Error>>;

/// Get storage devices of the given type: NVME, SSD or HD
///
/// Storage devices are identified from /sys/block attributes.
/// Removable devices are excluded.
///
/// # Arguments
///
/// * `kind`: type of storage device
///
/// returns: sorted list of found device names (`sd*` and `nvme*`)
pub fn get_storage_devices(kind: StorageDevice) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut devices = Vec::new();
    let sys_block = Path::new("/sys/block");

    if !sys_block.exists() {
        info!("No storage device found");
        return Ok(devices);
    }

    let device_regex = Regex::new(r"^sd[a-z]+$")?;
    let nvme_regex = Regex::new(r"^nvme[0-9]+n[0-9]+$")?;

    for entry in fs::read_dir(sys_block)? {
        let entry = entry?;
        let dev_name = entry.file_name();
        let dev_str = dev_name.to_string_lossy();

        // filter out all non sd* and nvme* devices
        let is_nvme = nvme_regex.is_match(&dev_str);
        let is_storage = device_regex.is_match(&dev_str);
        if !(is_nvme || is_storage) {
            continue;
        }

        match kind {
            StorageDevice::All => {}
            StorageDevice::Hdd | StorageDevice::Ssd | StorageDevice::HddOrSsd => {
                if !is_storage {
                    continue;
                }
            }
            StorageDevice::Nvme => {
                if !is_nvme {
                    continue;
                }
            }
        };

        if is_nvme {
            let dev_name = entry.file_name();
            let dev_str = dev_name.to_string_lossy();
            devices.push(dev_str.to_string());
            continue;
        }

        let rotational_path = sys_block.join(dev_str.as_ref()).join("queue/rotational");
        let removable_path = sys_block.join(dev_str.as_ref()).join("removable");

        match (
            fs::read_to_string(&rotational_path),
            fs::read_to_string(&removable_path),
        ) {
            (Ok(rotational), Ok(removable)) => {
                let rotational = rotational.trim();
                let removable = removable.trim();

                // ignore removable
                if removable == "1" {
                    continue;
                }

                if kind == StorageDevice::Hdd && rotational == "1"
                    || kind == StorageDevice::Ssd && rotational == "0"
                    || kind == StorageDevice::HddOrSsd
                {
                    devices.push(dev_str.to_string());
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                error!("Unable to read device {dev_str} attributes: {e}");
            }
        }
    }

    devices.sort();
    Ok(devices)
}

/// Retrieve temperature from NVMe or SDD/HDD with smartctl safely without waking sleeping disks
pub fn get_smartctl_disk_temperature(dev: &str) -> Result<Option<i32>, Box<dyn std::error::Error>> {
    let temp_regex = Regex::new(
        r"(?:194\s+Temperature_Celsius|190\s+Airflow_Temperature_Cel)\s+\S+\s+\S+\s+\S+\s+\S+\s+\S+\s+\S+\s+-\s+(\d+)",
    )?;
    let generic_temp_regex = Regex::new(r"(?:Temperature|Current Drive Temperature):\s+(\d+)")?;

    let dev_path = if dev.starts_with('/') {
        dev.to_string()
    } else {
        format!("/dev/{}", dev)
    };

    match Command::new("smartctl")
        .arg("-n")
        .arg("standby")
        .arg("-A")
        .arg(&dev_path)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);

            if let Some(temp_captures) = temp_regex
                .captures(&stdout)
                .or_else(|| generic_temp_regex.captures(&stdout))
                && let Some(temp_match) = temp_captures.get(1)
            {
                let temperature = temp_match.as_str().parse::<i32>()?;
                return Ok(Some(temperature));
            }
        }
        Err(e) => {
            debug!("Device {dev_path} smartctl query skipped or failed: {e}");
        }
    }

    Ok(None)
}

/// Calculate actual filesystem usage rate of hard disk (based on df command)
pub fn get_disk_usage(dev: &str) -> Result<DiskUsage, Box<dyn std::error::Error>> {
    let mut tmp = DiskUsage {
        usage_percent: 0.0,
        total_used: 0,
        total_size: 0,
    };

    // Get mounted partitions for this device
    let cmd = format!(
        "df -h --output=source,target,pcent | grep '/dev/{}[0-9]*'",
        dev
    );

    match Command::new("sh").arg("-c").arg(&cmd).output() {
        Ok(output) => {
            if !output.status.success() {
                return Ok(tmp);
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut total_used: u64 = 0;
            let mut total_size: u64 = 0;

            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 3 {
                    continue;
                }

                let mountpoint = parts[1];

                // Get size in bytes
                let size_cmd = format!(
                    "df --block-size=1 {} | awk 'NR==2 {{print $2}}'",
                    mountpoint
                );
                if let Ok(size_output) = Command::new("sh").arg("-c").arg(&size_cmd).output()
                    && let Ok(size_str) = String::from_utf8(size_output.stdout)
                    && let Ok(size) = size_str.trim().parse::<u64>()
                {
                    total_size += size;
                }

                // Get used space in bytes
                let used_cmd = format!(
                    "df --block-size=1 {} | awk 'NR==2 {{print $3}}'",
                    mountpoint
                );
                if let Ok(used_output) = Command::new("sh").arg("-c").arg(&used_cmd).output()
                    && let Ok(used_str) = String::from_utf8(used_output.stdout)
                    && let Ok(used) = used_str.trim().parse::<u64>()
                {
                    total_used += used;
                }
            }

            if total_size != 0 {
                tmp.usage_percent =
                    ((total_used as f64 / total_size as f64) * 100.0 * 100.0).round() / 100.0;
                tmp.total_used = total_used;
                tmp.total_size = total_size;
            }

            Ok(tmp)
        }
        Err(_) => Ok(tmp),
    }
}

/// Format bytes into human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    const THRESHOLD: f64 = 1024.0;

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size /= THRESHOLD;
        unit_index += 1;
    }

    if unit_index > 0 {
        format!("{:.2} {}", size, UNITS[unit_index])
    } else {
        format!("{} {}", size, UNITS[unit_index])
    }
}

/// Format network speed cleanly to avoid overflowing UI boundaries across all speeds (up to 10GbE)
pub fn format_network_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec >= 1_000_000_000 {
        // 10GbE saturation (1.00 GB/s to 1.25 GB/s)
        format!("{:.2} GB/s", bytes_per_sec as f64 / 1_073_741_824.0)
    } else if bytes_per_sec >= 100_000_000 {
        // High speed (100 MB/s to 999 MB/s) - drop decimal for compact fit
        format!("{:.0} MB/s", (bytes_per_sec as f64 / 1_048_576.0).round())
    } else if bytes_per_sec >= 1_000_000 {
        // Moderate speed (1.0 MB/s to 99.9 MB/s)
        format!("{:.1} MB/s", bytes_per_sec as f64 / 1_048_576.0)
    } else if bytes_per_sec >= 1_000 {
        // Normal speed (1 KB/s to 999 KB/s)
        format!("{:.0} KB/s", (bytes_per_sec as f64 / 1_024.0).round())
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_format_network_speed() {
        assert_eq!(format_network_speed(500), "500 B/s");
        assert_eq!(format_network_speed(138_000), "135 KB/s");
        assert_eq!(format_network_speed(24_500_000), "23.4 MB/s");
        assert_eq!(format_network_speed(450_000_000), "429 MB/s");
        assert_eq!(format_network_speed(1_150_000_000), "1.07 GB/s");
    }

    #[test]
    fn test_format_disk_summary() {
        // 11.2 TB on 16 TB drive
        let used_tb = (11.2 * 1_099_511_627_776.0) as u64;
        let size_16t = 16 * 1_099_511_627_776;
        assert_eq!(format_disk_summary(used_tb, size_16t), "11.2T/16T");

        // 76.6 GB on 4 TB drive
        let used_gb = (76.6 * 1_073_741_824.0) as u64;
        let size_4t = 4 * 1_099_511_627_776;
        assert_eq!(format_disk_summary(used_gb, size_4t), "77G/4T");

        // 226 GB on 2 TB drive
        let used_226g = 226 * 1_073_741_824;
        let size_2t = 2 * 1_099_511_627_776;
        assert_eq!(format_disk_summary(used_226g, size_2t), "226G/2T");
    }

    #[test]
    fn test_format_temperature() {
        assert_eq!(format_temperature(40.0, false), "40");
        assert_eq!(format_temperature(40.0, true), "104");
        assert_eq!(format_temperature(0.0, true), "32");
    }

    #[test]
    fn test_proc_net_dev_parsing() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            "Inter-|   Receive                                                |  Transmit\n\
             face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n\
               lo: 400508544 1912958    0    0    0     0          0         0 400508544 1912958    0    0    0     0       0          0\n\
              br0: 1044439050 2004245    0    0    0     0          0         0 827299042 1656860    0    0    0     0       0          0"
        ).unwrap();
        let devs = read_proc_net_dev(tmp.path());
        assert_eq!(devs.len(), 2);
        assert_eq!(devs[0].name, "lo");
        assert_eq!(devs[0].rx_bytes, 400508544);
        assert_eq!(devs[0].tx_bytes, 400508544);
        assert_eq!(devs[1].name, "br0");
        assert_eq!(devs[1].rx_bytes, 1044439050);
        assert_eq!(devs[1].tx_bytes, 827299042);
    }
}
