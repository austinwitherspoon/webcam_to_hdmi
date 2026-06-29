# Raspberry Pi 5 Webcam To HDMI Service

Map two USB webcams to two HDMI outputs on Raspberry Pi 5 with independent behavior:

- USB camera 1 -> HDMI 1 fullscreen
- USB camera 2 -> HDMI 2 fullscreen
- If a camera is missing, that HDMI shows black
- Plug/unplug on one camera does not restart or disturb the other output

> NOTE: This project was an experiment in vibe-coding. I have reviewed the code in this repo, but I do not have a deep understanding of the linux internals that allow most of this to work. I have confirmed it's functional on my own device, but use at your own risk.

## One-Line Install On Raspberry Pi

Run this on the Raspberry Pi:

```bash
curl -fsSL https://github.com/austinwitherspoon/webcam_to_hdmi/releases/latest/download/setup.sh | sudo sh
```

What this does:

- Downloads the latest release binary from GitHub Releases
- Installs runtime dependencies (GStreamer, DRM/V4L tools)
- Auto-detects camera by-path and HDMI connector IDs
- Writes `/etc/default/webcam_to_hdmi`
- Installs and starts `webcam_to_hdmi.service`

## GitHub Release Build

GitHub Actions workflow:

- Builds Linux arm64 binary (`aarch64-unknown-linux-gnu`)
- Publishes release assets on tag pushes matching `v*`
- Uploads these assets:
  - `webcam_to_hdmi-aarch64-unknown-linux-gnu`
  - `setup.sh`

To publish a new release, create and push a tag such as `v0.2.0`.

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

## Dev Deploy over SSH (Windows)

From repo root:

```powershell
powershell -ExecutionPolicy Bypass -File dev_deploy/deploy_to_pi.ps1 -HostName [ip-of-pi] -UserName pi -BuildOnPi -InstallService
```

## Dev Deploy over SSH (Linux/macOS)

From repo root:

```bash
bash dev_deploy/deploy_to_pi.sh --host-name [ip-of-pi] --user-name pi --build-on-pi --install-service
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
--skip-setup            Skip running dev_deploy/setup_pi.sh on the Pi
```

## Runtime Configuration

Main config file on Pi:

- `/etc/default/webcam_to_hdmi`

Key variables:

- `USB1_CAM_DEV` (default `/dev/v4l/by-path/platform-xhci-hcd.1-usb-0:1:1.0-video-index0`)
- `USB2_CAM_DEV` (default `/dev/v4l/by-path/platform-xhci-hcd.0-usb-0:1:1.0-video-index0`)
- `HDMI1_CONNECTOR_ID` (default `35`)
- `HDMI2_CONNECTOR_ID` (default `44`)
- `HDMI1_PLANE_ID` (optional)
- `HDMI2_PLANE_ID` (optional)
- `DRM_CARD_DEV` (default `/dev/dri/card1`)
- `WIDTH` (default `1920`)
- `HEIGHT` (default `1080`)
- `FPS` (default `30`)
- `POLL_MS` (default `1000`)
- `CAMERA_RETRY_MS` (default `5000`)

`deploy/setup.sh` uses these known-good defaults first so setup does not depend on cameras being connected during install; if matching devices are present, it will still prefer detected by-path values.

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
