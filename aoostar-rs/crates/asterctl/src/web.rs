// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026

//! Embedded Web Management Server on Port 8744.
//! Serves the configuration dashboard and live display preview.

use crate::cfg::Settings;
use crate::web_assets::INDEX_HTML;
use log::{error, info, warn};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use tiny_http::{Header, Response, Server, StatusCode};

pub struct WebServerConfig {
    pub port: u16,
    pub settings_path: PathBuf,
    pub latest_frame: Arc<Mutex<Option<Vec<u8>>>>,
    pub active_panel_name: Arc<Mutex<String>>,
    pub switch_panel_signal: Arc<AtomicBool>,
    pub sensor_values: Arc<RwLock<HashMap<String, String>>>,
}

pub fn start_web_server(config: WebServerConfig) {
    thread::spawn(move || {
        let addr = format!("0.0.0.0:{}", config.port);
        let server = match Server::http(&addr) {
            Ok(s) => {
                info!("WebUI server listening at http://{}", addr);
                s
            }
            Err(e) => {
                error!("Failed to start WebUI server on {}: {}", addr, e);
                return;
            }
        };

        for mut request in server.incoming_requests() {
            let url = request.url().to_string();
            let path = url.split('?').next().unwrap_or("/");

            match (request.method(), path) {
                (&tiny_http::Method::Get, "/") => {
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
                    let response = Response::from_string(INDEX_HTML).with_header(header);
                    let _ = request.respond(response);
                }
                (&tiny_http::Method::Get, "/api/preview") => {
                    let frame = config.latest_frame.lock().unwrap();
                    if let Some(ref bytes) = *frame {
                        let header_type = Header::from_bytes(&b"Content-Type"[..], &b"image/jpeg"[..]).unwrap();
                        let header_cache = Header::from_bytes(&b"Cache-Control"[..], &b"no-cache, no-store, must-revalidate"[..]).unwrap();
                        let response = Response::from_data(bytes.clone())
                            .with_header(header_type)
                            .with_header(header_cache);
                        let _ = request.respond(response);
                    } else {
                        let response = Response::from_string("No preview available yet")
                            .with_status_code(StatusCode(503));
                        let _ = request.respond(response);
                    }
                }
                (&tiny_http::Method::Get, "/api/status") => {
                    let power_state = fs::read_to_string("/tmp/screen_power")
                        .unwrap_or_else(|_| "auto".to_string())
                        .trim()
                        .to_string();
                    let panel_name = config.active_panel_name.lock().unwrap().clone();
                    let sensors = config.sensor_values.read().unwrap().clone();

                    let status_obj = serde_json::json!({
                        "power_state": power_state,
                        "active_panel_name": panel_name,
                        "sensors": sensors,
                    });

                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let response = Response::from_string(status_obj.to_string()).with_header(header);
                    let _ = request.respond(response);
                }
                (&tiny_http::Method::Get, "/api/settings") => {
                    let settings_json = fs::read_to_string(&config.settings_path)
                        .unwrap_or_else(|_| "{}".to_string());
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let response = Response::from_string(settings_json).with_header(header);
                    let _ = request.respond(response);
                }
                (&tiny_http::Method::Post, "/api/settings") => {
                    let mut body = String::new();
                    if let Err(e) = request.as_reader().read_to_string(&mut body) {
                        let response = Response::from_string(format!("Error reading body: {e}"))
                            .with_status_code(StatusCode(400));
                        let _ = request.respond(response);
                        continue;
                    }

                    match serde_json::from_str::<Settings>(&body) {
                        Ok(new_settings) => {
                            if let Ok(pretty_json) = serde_json::to_string_pretty(&new_settings) {
                                if let Err(e) = fs::write(&config.settings_path, pretty_json) {
                                    let response = Response::from_string(format!("Failed to write settings: {e}"))
                                        .with_status_code(StatusCode(500));
                                    let _ = request.respond(response);
                                    continue;
                                }
                                info!("WebUI: Updated settings saved to {:?}", config.settings_path);
                                let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                                let response = Response::from_string(r#"{"success":true}"#).with_header(header);
                                let _ = request.respond(response);
                            } else {
                                let response = Response::from_string("Error serializing settings")
                                    .with_status_code(StatusCode(500));
                                let _ = request.respond(response);
                            }
                        }
                        Err(e) => {
                            warn!("WebUI: Invalid settings JSON received: {e}");
                            let response = Response::from_string(format!("Invalid settings: {e}"))
                                .with_status_code(StatusCode(400));
                            let _ = request.respond(response);
                        }
                    }
                }
                (&tiny_http::Method::Post, "/api/power") => {
                    let mut body = String::new();
                    let _ = request.as_reader().read_to_string(&mut body);
                    let val: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                    let action = val.get("action").and_then(|v| v.as_str()).unwrap_or("auto");
                    
                    let _ = fs::write("/tmp/screen_power", action);
                    info!("WebUI: Set screen power to '{}'", action);

                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let response = Response::from_string(r#"{"success":true}"#).with_header(header);
                    let _ = request.respond(response);
                }
                (&tiny_http::Method::Post, "/api/panel/switch") => {
                    config.switch_panel_signal.store(true, Ordering::Relaxed);
                    info!("WebUI: Triggered next panel switch");
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let response = Response::from_string(r#"{"success":true}"#).with_header(header);
                    let _ = request.respond(response);
                }
                _ => {
                    let response = Response::from_string("Not Found").with_status_code(StatusCode(404));
                    let _ = request.respond(response);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    #[test]
    fn test_web_server_startup_and_get() {
        let port = 18744;
        let temp_dir = std::env::temp_dir();
        let settings_path = temp_dir.join("test_web_settings.json");
        let _ = fs::write(&settings_path, "{}");

        let latest_frame = Arc::new(Mutex::new(Some(vec![1, 2, 3, 4])));
        let active_panel_name = Arc::new(Mutex::new("test_panel".to_string()));
        let switch_panel_signal = Arc::new(AtomicBool::new(false));
        let sensor_values = Arc::new(RwLock::new(HashMap::new()));

        start_web_server(WebServerConfig {
            port,
            settings_path: settings_path.clone(),
            latest_frame,
            active_panel_name,
            switch_panel_signal,
            sensor_values,
        });

        thread::sleep(Duration::from_millis(150));

        if let Ok(mut stream) = TcpStream::connect(format!("127.0.0.1:{port}")) {
            let req = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            stream.write_all(req.as_bytes()).unwrap();

            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();

            assert!(response.contains("HTTP/1.1 200 OK"));
            assert!(response.contains("AOOSTAR WTR Max"));
        }

        let _ = fs::remove_file(settings_path);
    }
}
