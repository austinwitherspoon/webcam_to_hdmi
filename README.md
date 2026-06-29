# Raspberry Pi 5 Webcam To HDMI Service

Map two USB webcams to two HDMI outputs on Raspberry Pi 5 with independent behavior:

- USB camera 1 -> HDMI 1 fullscreen
- USB camera 2 -> HDMI 2 fullscreen
- If a camera is missing, that HDMI shows black
- Plug/unplug on one camera does not restart or disturb the other output

> NOTE: This project was an experiment in vibe-coding. I have reviewed the code in this repo, but I do not have a deep understanding of the linux internals that allow most of this to work. I have confirmed it's functional on my own device, but use at your own risk.

## Prerequisites

### On Raspberry Pi 5

- Raspberry Pi OS / Debian-based image
- SSH enabled
- User with sudo access
- Two HDMI displays connected
- USB webcams connected

### On your development machine

- SSH client + SCP available in PATH
- Rust build path:
  - Preferred: native build on Pi (`--build-on-pi` / `-BuildOnPi`)
  - Optional: local cross build with `cross`

## Quick Start (Windows)

From repo root:

```powershell
powershell -ExecutionPolicy Bypass -File deploy/deploy_to_pi.ps1 -HostName [ip-of-pi] -UserName pi -BuildOnPi -InstallService
```

## Quick Start (Linux/macOS)

From repo root:

```bash
bash deploy/deploy_to_pi.sh --host-name [ip-of-pi] --user-name pi --build-on-pi --install-service
```

## Bash Deploy Script Options

```text
--host-name <host>      SSH host name (default: rp5.lan)
--user-name <user>      SSH user name (default: austin)
--target <triple>       Rust target triple for local cross build
--install-path <path>   Remote binary install path
--install-service       Install and restart systemd service
--no-build              Skip local build and deploy existing local binary
--build-on-pi           Build natively on the Raspberry Pi
--skip-setup            Skip running deploy/setup_pi.sh on the Pi
```

## Runtime Configuration

Main config file on Pi:

- `/etc/default/webcam_to_hdmi`

Key variables:

- `USB1_CAM_DEV` (default auto-detected)
- `USB2_CAM_DEV` (default auto-detected)
- `HDMI1_CONNECTOR_ID` (default auto-detected)
- `HDMI2_CONNECTOR_ID` (default auto-detected)
- `HDMI1_PLANE_ID` (optional)
- `HDMI2_PLANE_ID` (optional)
- `DRM_CARD_DEV` (default `/dev/dri/card1`)
- `WIDTH` (default `1920`)
- `HEIGHT` (default `1080`)
- `FPS` (default `30`)
- `POLL_MS` (default `1000`)
- `CAMERA_RETRY_MS` (default `5000`)

## Useful Commands

Check service status:

```bash
sudo systemctl status webcam_to_hdmi.service
```

View recent logs:

```bash
sudo journalctl -u webcam_to_hdmi.service -n 200 --no-pager
```

Restart service after config changes:

```bash
sudo systemctl restart webcam_to_hdmi.service
```

Inspect camera by-path mappings:

```bash
ls -l /dev/v4l/by-path
```

Inspect HDMI connector IDs:

```bash
modetest -M vc4 -c
```

## Validation Checklist

1. Only camera 1 connected:
  - HDMI1 shows camera
  - HDMI2 shows black
2. Only camera 2 connected:
  - HDMI2 shows camera
  - HDMI1 shows black
3. Both connected:
  - Both HDMI outputs show their mapped cameras
4. Unplug/replug camera 1:
  - HDMI2 stays stable
5. Unplug/replug camera 2:
  - HDMI1 stays stable