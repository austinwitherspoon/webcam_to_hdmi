#!/usr/bin/env bash
set -euo pipefail

HOST_NAME="rp5.lan"
USER_NAME="austin"
TARGET="aarch64-unknown-linux-musl"
INSTALL_PATH="/usr/local/bin/webcam_to_hdmi"
INSTALL_SERVICE=false
NO_BUILD=false
BUILD_ON_PI=false
SKIP_SETUP=false

usage() {
  cat <<'EOF'
Usage: deploy/deploy_to_pi.sh [options]

Options:
  --host-name <host>      SSH host name (default: rp5.lan)
  --user-name <user>      SSH user name (default: austin)
  --target <triple>       Rust target triple for local cross build
                          (default: aarch64-unknown-linux-musl)
  --install-path <path>   Remote binary install path
                          (default: /usr/local/bin/webcam_to_hdmi)
  --install-service       Install and restart systemd service
  --no-build              Skip local build and deploy existing local binary
  --build-on-pi           Build natively on the Raspberry Pi
  --skip-setup            Skip running deploy/setup_pi.sh on the Pi
  -h, --help              Show this help message

Examples:
  ./deploy/deploy_to_pi.sh --host-name rp5.lan --user-name austin --build-on-pi --install-service
  ./deploy/deploy_to_pi.sh --host-name pi.local --user-name pi --no-build --install-service
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host-name)
      HOST_NAME="$2"
      shift 2
      ;;
    --user-name)
      USER_NAME="$2"
      shift 2
      ;;
    --target)
      TARGET="$2"
      shift 2
      ;;
    --install-path)
      INSTALL_PATH="$2"
      shift 2
      ;;
    --install-service)
      INSTALL_SERVICE=true
      shift
      ;;
    --no-build)
      NO_BUILD=true
      shift
      ;;
    --build-on-pi)
      BUILD_ON_PI=true
      shift
      ;;
    --skip-setup)
      SKIP_SETUP=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

bin_local="target/${TARGET}/release/webcam_to_hdmi"
service_local="deploy/webcam_to_hdmi.service"
setup_local="deploy/setup_pi.sh"
build_on_pi_local="deploy/build_on_pi.sh"

if [[ ! -f "$setup_local" ]]; then
  echo "Missing setup script: $setup_local" >&2
  exit 1
fi

if [[ "$BUILD_ON_PI" == true && ! -f "$build_on_pi_local" ]]; then
  echo "Missing build script: $build_on_pi_local" >&2
  exit 1
fi

if [[ "$BUILD_ON_PI" == true && "$NO_BUILD" == true ]]; then
  echo "--build-on-pi and --no-build are mutually exclusive." >&2
  exit 1
fi

if [[ "$NO_BUILD" == false && "$BUILD_ON_PI" == false ]]; then
  if [[ -f "$bin_local" ]]; then
    echo "Removing existing binary artifact to force a fresh rebuild..."
    rm -f "$bin_local"
  fi

  echo "Building release binary with cross..."
  cross build --release --target "$TARGET"
fi

if [[ "$BUILD_ON_PI" == false && ! -f "$bin_local" ]]; then
  echo "Binary not found after build: $bin_local" >&2
  exit 1
fi

if [[ "$SKIP_SETUP" == false ]]; then
  echo "Copying and running Raspberry Pi setup script..."
  scp "$setup_local" "$USER_NAME@$HOST_NAME:/tmp/webcam_to_hdmi_setup.sh"
  ssh "$USER_NAME@$HOST_NAME" "chmod +x /tmp/webcam_to_hdmi_setup.sh; sudo /tmp/webcam_to_hdmi_setup.sh"
fi

if [[ "$BUILD_ON_PI" == true ]]; then
  echo "Syncing source to Raspberry Pi for native build..."
  ssh "$USER_NAME@$HOST_NAME" "rm -rf /tmp/webcam_to_hdmi-src; mkdir -p /tmp/webcam_to_hdmi-src"

  cargo_files=("Cargo.toml")
  if [[ -f "Cargo.lock" ]]; then
    cargo_files+=("Cargo.lock")
  fi
  scp "${cargo_files[@]}" "$USER_NAME@$HOST_NAME:/tmp/webcam_to_hdmi-src/"

  scp -r "src" "$USER_NAME@$HOST_NAME:/tmp/webcam_to_hdmi-src/"
  scp "$build_on_pi_local" "$USER_NAME@$HOST_NAME:/tmp/webcam_to_hdmi_build.sh"

  echo "Building release binary on Raspberry Pi..."
  ssh "$USER_NAME@$HOST_NAME" "chmod +x /tmp/webcam_to_hdmi_build.sh; /tmp/webcam_to_hdmi_build.sh /tmp/webcam_to_hdmi-src /tmp/webcam_to_hdmi"
else
  echo "Copying binary to $USER_NAME@$HOST_NAME:/tmp/webcam_to_hdmi ..."
  scp "$bin_local" "$USER_NAME@$HOST_NAME:/tmp/webcam_to_hdmi"
fi

echo "Installing binary at $INSTALL_PATH ..."
ssh "$USER_NAME@$HOST_NAME" "sudo install -m 0755 /tmp/webcam_to_hdmi $INSTALL_PATH"

if [[ "$INSTALL_SERVICE" == true ]]; then
  echo "Copying systemd unit..."
  scp "$service_local" "$USER_NAME@$HOST_NAME:/tmp/webcam_to_hdmi.service"

  remote_cmds="sudo install -m 0644 /tmp/webcam_to_hdmi.service /etc/systemd/system/webcam_to_hdmi.service; \
sudo systemctl daemon-reload; \
sudo systemctl disable --now webcam_weston.service || true; \
sudo systemctl enable webcam_to_hdmi.service; \
sudo systemctl restart webcam_to_hdmi.service; \
sudo systemctl --no-pager --full status webcam_to_hdmi.service"

  ssh "$USER_NAME@$HOST_NAME" "$remote_cmds"
fi

echo "Done. To test manually:"
echo "  ssh $USER_NAME@$HOST_NAME"
echo "  sudo $INSTALL_PATH"
