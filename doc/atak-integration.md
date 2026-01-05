# ATAK/TAK Server Integration

Rayhunter can send Cursor on Target (CoT) alerts to TAK servers (ATAK, WinTAK, iTAK) when potential IMSI catcher (Stingray) activity is detected. Alerts appear as boundary shapes on the map showing the estimated operational range of the detected threat.

## Overview

When Rayhunter detects suspicious cellular activity, it can automatically send a CoT message to your TAK server. The alert includes:

- A circular boundary representing the estimated Stingray range (default 1km)
- Threat severity level (High/Medium/Low) with color coding
- Detection timestamp and details
- 72-hour persistence (configurable) with automatic refresh on new detections

## Configuration

### Via Web Interface

1. Connect to your Rayhunter device's web interface (default: `http://192.168.1.1:8080`)
2. Expand the **Configuration** section
3. Scroll to **ATAK/TAK Server Integration**
4. Enable the integration and fill in the required fields

### Via TOML Configuration

Add the following to your `config.toml`:

```toml
[atak]
enabled = true
tak_server_address = "192.168.1.100:8087"  # Your TAK server IP:port
latitude = 38.8977                          # Your static location
longitude = -77.0365
callsign = "STINGRAY"                       # Prefix for alert callsigns
device_uid = "rayhunter-001"                # Optional, auto-generated if not set
boundary_radius_meters = 1000               # Estimated Stingray range (default 1km)
stale_hours = 72                            # How long alerts persist
```

## Configuration Options

| Option | Required | Default | Description |
|--------|----------|---------|-------------|
| `enabled` | Yes | `false` | Enable/disable ATAK integration |
| `tak_server_address` | Yes | - | TAK server address in `host:port` format |
| `latitude` | Yes | - | Static latitude for the Rayhunter device |
| `longitude` | Yes | - | Static longitude for the Rayhunter device |
| `callsign` | No | `STINGRAY` | Prefix for alert callsigns (e.g., STINGRAY-HIGH) |
| `device_uid` | No | auto | Unique device identifier for CoT messages |
| `boundary_radius_meters` | No | `1000` | Radius of the threat boundary circle in meters |
| `stale_hours` | No | `72` | Hours until the alert expires on TAK clients |

## Stingray Range Estimates

The default boundary radius of 1000 meters (1km) is based on typical IMSI catcher operational ranges:

- **Portable/handheld units**: 200-500m
- **Vehicle-mounted units**: 500m-2km  
- **High-power/fixed installations**: Up to 2.5km

Adjust `boundary_radius_meters` based on your threat model and environment. Urban areas with many obstructions typically have shorter effective ranges.

## TAK Server Setup

### TCP Connection (Default)

Rayhunter connects to TAK servers via plain TCP. Ensure your TAK server is configured to accept TCP connections on the specified port.

For **FreeTAKServer**:
```yaml
# In config.yaml
Connector:
  tcp:
    port: 8087
```

For **TAK Server** (official):
- Configure a streaming TCP input on your desired port

### Network Requirements

- Rayhunter device must be on the same network as the TAK server (or have routing configured)
- For MANET deployments, ensure the TAK server is reachable via the mesh network
- Firewall must allow outbound TCP connections from Rayhunter to the TAK server port

## Alert Appearance in ATAK

Alerts appear as:

- **Red circle**: High severity threat
- **Orange circle**: Medium severity threat  
- **Yellow circle**: Low severity threat

The circle represents the estimated operational range of the detected IMSI catcher. The remarks field contains:
- Severity level
- Detection details
- Timestamp
- Estimated range

## Headless Operation

For deployments without web interface access, configure ATAK settings in `config.toml` before installation:

1. Edit your `config.toml` with ATAK settings
2. Install Rayhunter with the pre-configured file
3. Rayhunter will automatically connect to the TAK server on startup

## Troubleshooting

### Alerts not appearing in ATAK

1. Verify TAK server address and port are correct
2. Check network connectivity between Rayhunter and TAK server
3. Ensure TAK server is accepting TCP connections
4. Check Rayhunter logs for connection errors

### Connection refused errors

- Verify TAK server is running and listening on the configured port
- Check firewall rules on both Rayhunter device and TAK server
- Ensure correct IP address (not hostname) is used

### Alerts appearing at wrong location

- Verify latitude/longitude are correct in configuration
- Remember coordinates are static - update them if the device moves significantly
