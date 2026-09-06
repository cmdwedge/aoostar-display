// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use asterctl::cfg::{MonitorConfig, Panel, load_custom_panel};
use asterctl::render::PanelRenderer;
use asterctl::sensors::{read_filter_file, read_key_value_file, start_file_slurper};
use asterctl::{cfg, img, web};
use asterctl_lcd::{AooScreen, AooScreenBuilder, DISPLAY_SIZE};

use anyhow::anyhow;
use clap::Parser;
use env_logger::Env;
use log::{debug, error, info};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// AOOSTAR WTR MAX and GEM12+ PRO screen control.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Serial device, for example, "/dev/cu.usbserial-AB0KOHLS". Takes priority over --usb option.
    #[arg(short, long)]
    device: Option<String>,

    /// USB serial UART "vid:pid" in hex notation (lsusb output). Default: 416:90A1
    #[arg(short, long)]
    usb: Option<String>,

    /// Switch display on and exit. This will show the last displayed image.
    #[arg(long)]
    on: bool,

    /// Switch display off and exit.
    #[arg(long)]
    off: bool,

    /// Image to display, other sizes than 960x376 will be scaled.
    #[arg(short, long)]
    image: Option<String>,

    /// AOOSTAR-X json configuration file to parse.
    ///
    /// The configuration file will be loaded from the `config_dir` directory if no full path is
    /// specified.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Include one or more additional custom panels into the base configuration.
    ///
    /// Specify the path to the panel directory containing panel.json and fonts / img subdirectories.
    #[arg(short, long)]
    panels: Option<Vec<PathBuf>>,

    /// Configuration directory containing configuration files and background images
    /// specified in the `config` file.
    #[arg(long, default_value_t = String::from("cfg"))]
    config_dir: String, // default_value_t requires Display trait which PathBuf does not implement

    /// Font directory for fonts specified in the `config` file.
    #[arg(long, default_value_t = String::from("fonts"))]
    font_dir: String,

    /// Single sensor value input file or directory for multiple sensor input files.
    #[arg(long, default_value_t = String::from("cfg/sensors"))]
    sensor_path: String,

    /// Sensor identifier mapping file. Ignored if the file does not exist.
    ///
    /// The configuration file will be loaded from the `config_dir` directory if no full path is
    /// specified.
    #[arg(long, default_value_t = String::from("sensor-mapping.cfg"))]
    sensor_mapping: String,

    /// Switch off display n seconds after loading image or running demo.
    #[arg(short, long)]
    off_after: Option<u32>,

    /// Test mode: only write to the display without checking response.
    #[arg(short, long)]
    write_only: bool,

    /// Test mode: save changed images in ./out folder.
    #[arg(short, long)]
    save: bool,

    /// Simulate serial port for testing and development, `--device` and `--usb` options are ignored.
    #[arg(long)]
    simulate: bool,

    /// Settings file for power management and daemon parameters (e.g. /config/settings.json).
    #[arg(long)]
    settings: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    // initialize display with given UART port parameter
    let mut builder = AooScreenBuilder::new();
    builder.no_init_check(args.write_only);
    let mut screen = if args.simulate {
        builder.simulate()?
    } else if let Some(device) = args.device {
        builder.open_device(&device)?
    } else if let Some(usb) = args.usb {
        builder.open_usb_id(&usb)?
    } else {
        builder.open_default()?
    };

    // process simple commands
    if args.off {
        screen.off()?;
        return Ok(());
    } else if args.on {
        screen.on()?;
        return Ok(());
    }

    // switch on screen for remaining commands
    screen.init()?;

    if let Some(config) = args.config {
        info!("Starting sensor panel mode");
        let img_save_path = if args.save {
            let img_save_path = PathBuf::from("out");
            fs::create_dir_all(&img_save_path)?;
            Some(img_save_path)
        } else {
            None
        };

        let cfg_dir = PathBuf::from(args.config_dir);
        let font_dir = PathBuf::from(args.font_dir);
        let sensor_path = PathBuf::from(args.sensor_path);
        let mapping_cfg = PathBuf::from(args.sensor_mapping);
        let settings_path = args.settings.clone().unwrap_or_else(|| {
            let direct = cfg_dir.join("settings.json");
            if direct.is_file() {
                direct
            } else {
                PathBuf::from("/config/settings.json")
            }
        });
        let cfg = load_configuration(&config, &cfg_dir, args.panels, &mapping_cfg)?;
        run_sensor_panel(
            &mut screen,
            cfg,
            settings_path,
            cfg_dir,
            font_dir,
            sensor_path,
            img_save_path,
        )?;
        return Ok(());
    }

    if let Some(image) = args.image {
        info!("Loading and displaying background image {image}...");
        let rgb_img = img::load_image(&image, Some(DISPLAY_SIZE))?.to_rgb8();
        let timestamp = Instant::now();
        screen.send_image(&rgb_img)?;
        debug!("Image sent in {}ms", timestamp.elapsed().as_millis());
    }

    if let Some(off) = args.off_after {
        info!("Switching off display in {off}s");
        sleep(Duration::from_secs(off as u64));
        screen.off()?;
    }

    info!("Bye bye!");

    Ok(())
}

fn load_configuration<P: AsRef<Path>>(
    config: P,
    config_dir: P,
    panels: Option<Vec<PathBuf>>,
    sensor_mapping: P,
) -> anyhow::Result<MonitorConfig> {
    let config = config.as_ref();
    let config_dir = config_dir.as_ref();

    let mut cfg = if config.is_absolute() {
        cfg::load_cfg(config)?
    } else {
        cfg::load_cfg(config_dir.join(config))?
    };

    if let Some(panels) = panels {
        for panel in panels {
            cfg.include_custom_panel(load_custom_panel(panel)?);
        }
    }

    let sensor_mapping = sensor_mapping.as_ref();
    let mapping_cfg = if sensor_mapping.is_absolute() {
        sensor_mapping.to_path_buf()
    } else {
        config_dir.join(sensor_mapping)
    };
    if mapping_cfg.is_file() {
        let mut mapping = HashMap::new();
        read_key_value_file(&mapping_cfg, &mut mapping, None)?;
        cfg.set_sensor_mapping(mapping);
    } else {
        info!("Sensor mapping file {mapping_cfg:?} not found");
    }

    cfg.sensor_filter = load_sensor_filter(&mapping_cfg)?;

    Ok(cfg)
}

fn load_sensor_filter(mapping_cfg: &Path) -> anyhow::Result<Option<Vec<Regex>>> {
    if let Some(parent) = mapping_cfg.parent()
        && let Some(file_stem) = mapping_cfg.file_stem()
        && let Some(extension) = mapping_cfg.extension()
    {
        let filter_file = parent
            .join(format!("{}-filter", file_stem.to_string_lossy()))
            .with_extension(extension);

        if filter_file.is_file() {
            info!("Loading sensor filter file {filter_file:?}");
            return read_filter_file(filter_file);
        } else {
            info!("No sensor filter file {filter_file:?} available");
        }
    }

    Ok(None)
}

fn run_sensor_panel<B: Into<PathBuf>>(
    screen: &mut AooScreen,
    mut cfg: MonitorConfig,
    settings_path: PathBuf,
    config_dir: B,
    font_dir: B,
    sensor_path: B,
    img_save_path: Option<B>,
) -> anyhow::Result<()> {
    let font_dir = font_dir.into();
    let config_dir = config_dir.into();
    let img_save_path = img_save_path.map(|p| p.into());

    let mut renderer = PanelRenderer::new(DISPLAY_SIZE, &font_dir, &config_dir);
    if let Some(img_save_path) = &img_save_path {
        renderer.set_img_save_path(img_save_path);
        renderer.set_save_render_img(true);
        // renderer.set_save_processed_pic(true);
        // renderer.set_save_progress_layer(true);
    }

    let sensor_values: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));

    start_file_slurper(
        sensor_path,
        sensor_values.clone(),
        cfg.sensor_filter.clone(),
    )?;

    let mut settings = cfg::load_settings(&settings_path);
    let mut last_settings_mtime = fs::metadata(&settings_path).and_then(|m| m.modified()).ok();
    let mut last_settings_check = Instant::now();

    if !settings.general.active_panels.is_empty() {
        cfg.active_panels = settings.general.active_panels.clone();
    }

    let latest_frame: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
    let active_panel_name = Arc::new(Mutex::new(String::new()));
    let switch_panel_signal = Arc::new(AtomicBool::new(false));

    let web_port = if let Ok(p) = std::env::var("WEB_PORT") {
        p.parse::<u16>().unwrap_or(settings.general.web_port)
    } else {
        settings.general.web_port
    };

    web::start_web_server(web::WebServerConfig {
        port: web_port,
        settings_path: settings_path.clone(),
        latest_frame: latest_frame.clone(),
        active_panel_name: active_panel_name.clone(),
        switch_panel_signal: switch_panel_signal.clone(),
        sensor_values: sensor_values.clone(),
    });

    let mut is_sleeping = false;
    let mut all_standby_start: Option<Instant> = None;

    // panel switching loop
    loop {
        // Periodically check if settings.json changed on disk (every 5 seconds)
        if last_settings_check.elapsed() >= Duration::from_secs(5) {
            last_settings_check = Instant::now();
            if let Ok(m) = fs::metadata(&settings_path).and_then(|m| m.modified()) {
                if Some(m) != last_settings_mtime {
                    info!("Settings file {:?} modified on disk, reloading...", settings_path);
                    settings = cfg::load_settings(&settings_path);
                    last_settings_mtime = Some(m);
                    if !settings.general.active_panels.is_empty() {
                        cfg.active_panels = settings.general.active_panels.clone();
                    }
                }
            }
        }

        let refresh = if settings.general.refresh_interval > 0 {
            Duration::from_secs(settings.general.refresh_interval as u64)
        } else {
            Duration::from_millis((cfg.setup.refresh * 1000f32) as u64)
        };

        let switch_time = if settings.general.switch_time > 0 {
            Duration::from_secs(settings.general.switch_time as u64)
        } else {
            cfg.setup
                .switch_time
                .as_deref()
                .and_then(|v| f32::from_str(v).ok())
                .map(|v| Duration::from_millis((v * 1000.0) as u64))
                .unwrap_or(Duration::from_secs(5))
        };

        let panel = cfg
            .get_next_active_panel()
            .ok_or(anyhow!("No active panel"))?;

        info!("Switching panel: {}", panel.friendly_name());
        if let Ok(mut name_lock) = active_panel_name.lock() {
            *name_lock = panel.friendly_name().to_string();
        }
        let panel_switch_time = Instant::now();

        // active panel refresh loop
        let mut refresh_count = 1;
        loop {
            let upd_start_time = Instant::now();

            if img_save_path.is_some() {
                renderer.set_img_suffix(format!("-{refresh_count:02}"));
            }

            // Read latest sensor data for rendering and power evaluation
            let values = sensor_values.read().expect("RwLock is poisoned");

            // 1. Manual power override via /tmp/screen_power
            let manual_override = if let Ok(s) = fs::read_to_string("/tmp/screen_power") {
                match s.trim().to_lowercase().as_str() {
                    "off" | "sleep" => Some(false),
                    "on" | "wake" => Some(true),
                    _ => None,
                }
            } else {
                None
            };

            // 2. Schedule evaluation
            let now_time = chrono::Local::now().time();
            let in_sleep_schedule = settings.power.sleep_schedule_enabled
                && cfg::is_time_in_range(now_time, &settings.power.sleep_start_time, &settings.power.sleep_end_time);

            // 3. Disk spindown evaluation
            let all_disks_standby = values
                .get("unraid_all_disks_standby")
                .map(|v| v == "true")
                .unwrap_or(false);

            if all_disks_standby {
                if all_standby_start.is_none() {
                    all_standby_start = Some(Instant::now());
                }
            } else {
                all_standby_start = None;
            }

            let in_standby_sleep = settings.power.sleep_on_all_disks_standby
                && all_standby_start.map_or(false, |start| {
                    start.elapsed() >= Duration::from_secs(settings.power.standby_delay_seconds)
                });

            // 4. Network activity wake evaluation
            let mut network_traffic_active = false;
            if settings.power.wake_on_network_activity {
                if let Some(bytes_str) = values.get("net_default_bytes_per_sec").or_else(|| values.get("net_throughput_bytes")) {
                    if let Ok(bytes) = bytes_str.parse::<u64>() {
                        if bytes >= settings.power.network_wake_threshold_bytes {
                            network_traffic_active = true;
                        }
                    }
                }
            }

            // Power state decision
            let should_sleep = match manual_override {
                Some(true) => false,
                Some(false) => true,
                None => {
                    if network_traffic_active {
                        false
                    } else if in_sleep_schedule || in_standby_sleep {
                        true
                    } else {
                        false
                    }
                }
            };

            if should_sleep {
                if !is_sleeping {
                    info!(
                        "Display power: Entering sleep mode (schedule={}, standby={}). Turning off LCD backlight.",
                        in_sleep_schedule, in_standby_sleep
                    );
                    if let Err(e) = screen.off() {
                        error!("Error turning off screen: {e}");
                    }
                    is_sleeping = true;
                }

                drop(values);
                sleep(Duration::from_secs(1));
                continue;
            }

            if !should_sleep && is_sleeping {
                info!("Display power: Waking up display. Turning on LCD backlight.");
                if let Err(e) = screen.on() {
                    error!("Error turning on screen: {e}");
                }
                screen.clear_cache();
                is_sleeping = false;
            }

            update_panel(screen, &mut renderer, panel, &values, &latest_frame)?;
            drop(values);

            let elapsed = upd_start_time.elapsed();
            if refresh > elapsed {
                sleep(refresh - elapsed);
            }

            if panel_switch_time.elapsed() >= switch_time || switch_panel_signal.swap(false, Ordering::Relaxed) {
                break;
            }

            refresh_count += 1;
        }
    }
}

fn update_panel(
    screen: &mut AooScreen,
    renderer: &mut PanelRenderer,
    panel: &Panel,
    values: &HashMap<String, String>,
    latest_frame: &Arc<Mutex<Option<Vec<u8>>>>,
) -> anyhow::Result<()> {
    debug!("Displaying panel '{}'...", panel.friendly_name());

    match renderer.render(panel, values) {
        Ok(image) => {
            let mut jpeg_buf = std::io::Cursor::new(Vec::new());
            if image.write_to(&mut jpeg_buf, image::ImageFormat::Jpeg).is_ok() {
                if let Ok(mut frame_lock) = latest_frame.lock() {
                    *frame_lock = Some(jpeg_buf.into_inner());
                }
            }
            screen.send_image(&image)?;
        }
        Err(e) => error!("Error rendering panel '{}': {e:?}", panel.friendly_name()),
    }

    Ok(())
}
