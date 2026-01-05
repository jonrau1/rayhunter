//! ATAK Cursor on Target (CoT) integration for Stingray alerts
//!
//! Sends threat detection events to TAK servers as CoT messages with
//! boundary shapes representing the estimated Stingray range.

use std::time::Duration;

use chrono::{DateTime, Utc};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_util::task::TaskTracker;

use rayhunter::analysis::analyzer::EventType;

/// ATAK/TAK Server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AtakConfig {
    /// Enable ATAK CoT integration
    pub enabled: bool,
    /// TAK Server endpoint (e.g., "192.168.1.100:8087" for TCP)
    pub tak_server_address: Option<String>,
    /// Device UID for CoT messages (auto-generated if not set)
    pub device_uid: Option<String>,
    /// Callsign prefix for alerts (e.g., "STINGRAY")
    pub callsign: String,
    /// Static latitude for the device location
    pub latitude: Option<f64>,
    /// Static longitude for the device location
    pub longitude: Option<f64>,
    /// Estimated Stingray range in meters (default 1000m based on typical operational range)
    pub boundary_radius_meters: u32,
    /// How long CoT events persist before going stale (in hours)
    pub stale_hours: u32,
}

impl Default for AtakConfig {
    fn default() -> Self {
        AtakConfig {
            enabled: false,
            tak_server_address: None,
            device_uid: None,
            callsign: "STINGRAY".to_string(),
            latitude: None,
            longitude: None,
            boundary_radius_meters: 1000, // 1km default - typical Stingray range is 200m-2km
            stale_hours: 72,
        }
    }
}

impl AtakConfig {
    /// Check if the configuration is valid for sending CoT messages
    pub fn is_valid(&self) -> bool {
        self.enabled
            && self.tak_server_address.is_some()
            && self.latitude.is_some()
            && self.longitude.is_some()
    }

    /// Get the device UID, generating one if not configured
    pub fn get_device_uid(&self) -> String {
        self.device_uid
            .clone()
            .unwrap_or_else(|| format!("rayhunter-{}", uuid_simple()))
    }
}

/// Simple UUID-like string generator (no external dependency)
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", duration.as_secs(), duration.subsec_nanos())
}

/// ATAK alert message to be sent to TAK server
#[derive(Debug, Clone)]
pub struct AtakAlert {
    pub event_type: EventType,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

impl AtakAlert {
    pub fn new(event_type: EventType, message: String) -> Self {
        AtakAlert {
            event_type,
            message,
            timestamp: Utc::now(),
        }
    }
}

/// ATAK notification service
pub struct AtakService {
    config: AtakConfig,
    tx: Sender<AtakAlert>,
    rx: Receiver<AtakAlert>,
}

impl AtakService {
    pub fn new(config: AtakConfig) -> Self {
        let (tx, rx) = mpsc::channel(10);
        Self { config, tx, rx }
    }

    pub fn new_handler(&self) -> Sender<AtakAlert> {
        self.tx.clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_valid()
    }
}

/// Generate CoT XML for a Stingray alert with boundary shape
fn generate_cot_xml(config: &AtakConfig, alert: &AtakAlert) -> String {
    let now = alert.timestamp;
    let stale = now + chrono::Duration::hours(config.stale_hours as i64);

    let time_str = now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let stale_str = stale.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

    let lat = config.latitude.unwrap_or(0.0);
    let lon = config.longitude.unwrap_or(0.0);
    let radius = config.boundary_radius_meters;

    // Generate unique event UID
    let event_uid = format!(
        "{}-alert-{}",
        config.get_device_uid(),
        now.timestamp_millis()
    );

    // CoT type: a-h-G = atom, hostile, ground
    // For different severity levels, we could use different types
    let cot_type = match alert.event_type {
        EventType::High => "a-h-G", // hostile
        EventType::Medium => "a-u-G", // unknown (caution)
        EventType::Low => "a-n-G",  // neutral (info)
        EventType::Informational => "a-n-G",
    };

    // Color based on severity (ARGB format)
    let color = match alert.event_type {
        EventType::High => "-65536",    // Red
        EventType::Medium => "-23296",  // Orange
        EventType::Low => "-256",       // Yellow
        EventType::Informational => "-16711936", // Green
    };

    let severity_str = match alert.event_type {
        EventType::High => "HIGH",
        EventType::Medium => "MEDIUM",
        EventType::Low => "LOW",
        EventType::Informational => "INFO",
    };

    let callsign = format!("{}-{}", config.callsign, severity_str);

    // Escape XML special characters in message
    let escaped_message = alert
        .message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0" uid="{event_uid}" type="{cot_type}" time="{time_str}" start="{time_str}" stale="{stale_str}" how="m-g">
  <point lat="{lat}" lon="{lon}" hae="0" ce="{radius}" le="9999999"/>
  <detail>
    <shape>
      <ellipse major="{radius}" minor="{radius}" angle="0"/>
    </shape>
    <strokeColor value="{color}"/>
    <strokeWeight value="3.0"/>
    <fillColor value="{color}"/>
    <contact callsign="{callsign}"/>
    <remarks>POTENTIAL IMSI CATCHER DETECTED

Severity: {severity_str}
Details: {escaped_message}
Detection Time: {time_str}
Estimated Range: {radius}m

This boundary represents the approximate operational range of a detected cell-site simulator (Stingray). Exercise caution in this area.</remarks>
    <precisionlocation altsrc="DTED0"/>
    <link uid="{}" relation="p-p" type="a-f-G" remarks="Rayhunter Detection Device"/>
  </detail>
</event>"#,
        config.get_device_uid()
    )
}

/// Send CoT message to TAK server via TCP
async fn send_cot_tcp(address: &str, cot_xml: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = TcpStream::connect(address).await?;
    stream.write_all(cot_xml.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// Run the ATAK notification worker
pub fn run_atak_worker(task_tracker: &TaskTracker, mut atak_service: AtakService) {
    task_tracker.spawn(async move {
        if !atak_service.config.is_valid() {
            if atak_service.config.enabled {
                warn!("ATAK is enabled but configuration is incomplete (missing server address or coordinates)");
            }
            // Drain the channel without sending
            loop {
                if atak_service.rx.recv().await.is_none() {
                    break;
                }
            }
            return;
        }

        let server_address = atak_service
            .config
            .tak_server_address
            .clone()
            .expect("validated above");

        info!(
            "ATAK CoT service started, sending to {}",
            server_address
        );

        let mut retry_count: u32 = 0;
        let max_retries: u32 = 5;

        loop {
            match atak_service.rx.recv().await {
                Some(alert) => {
                    debug!("Received ATAK alert: {:?}", alert.event_type);

                    let cot_xml = generate_cot_xml(&atak_service.config, &alert);
                    debug!("Generated CoT XML:\n{}", cot_xml);

                    // Attempt to send with retries
                    loop {
                        match send_cot_tcp(&server_address, &cot_xml).await {
                            Ok(()) => {
                                info!(
                                    "Successfully sent CoT alert to TAK server: {:?}",
                                    alert.event_type
                                );
                                retry_count = 0;
                                break;
                            }
                            Err(e) => {
                                retry_count += 1;
                                error!(
                                    "Failed to send CoT to TAK server (attempt {}/{}): {}",
                                    retry_count, max_retries, e
                                );

                                if retry_count >= max_retries {
                                    error!("Max retries reached, dropping alert");
                                    retry_count = 0;
                                    break;
                                }

                                // Exponential backoff
                                let delay = Duration::from_secs(2u64.pow(retry_count));
                                tokio::time::sleep(delay).await;
                            }
                        }
                    }
                }
                None => {
                    info!("ATAK service channel closed, shutting down");
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AtakConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.boundary_radius_meters, 1000);
        assert_eq!(config.stale_hours, 72);
        assert_eq!(config.callsign, "STINGRAY");
    }

    #[test]
    fn test_config_validation() {
        let mut config = AtakConfig::default();
        assert!(!config.is_valid());

        config.enabled = true;
        assert!(!config.is_valid());

        config.tak_server_address = Some("192.168.1.100:8087".to_string());
        assert!(!config.is_valid());

        config.latitude = Some(38.8977);
        config.longitude = Some(-77.0365);
        assert!(config.is_valid());
    }

    #[test]
    fn test_cot_xml_generation() {
        let config = AtakConfig {
            enabled: true,
            tak_server_address: Some("192.168.1.100:8087".to_string()),
            device_uid: Some("test-device".to_string()),
            callsign: "STINGRAY".to_string(),
            latitude: Some(38.8977),
            longitude: Some(-77.0365),
            boundary_radius_meters: 1000,
            stale_hours: 72,
        };

        let alert = AtakAlert::new(EventType::High, "Test alert message".to_string());
        let xml = generate_cot_xml(&config, &alert);

        assert!(xml.contains("a-h-G")); // hostile type for high severity
        assert!(xml.contains("38.8977")); // latitude
        assert!(xml.contains("-77.0365")); // longitude
        assert!(xml.contains("STINGRAY-HIGH")); // callsign
        assert!(xml.contains("major=\"1000\"")); // radius
    }
}
