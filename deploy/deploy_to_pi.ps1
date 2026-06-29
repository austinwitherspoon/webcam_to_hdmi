param(
    [string]$HostName = "rp5.lan",
    [string]$UserName = "austin",
    [string]$Target = "aarch64-unknown-linux-musl",
    [string]$InstallPath = "/usr/local/bin/webcam_to_hdmi",
    [switch]$InstallService,
    [switch]$NoBuild,
    [switch]$BuildOnPi,
    [switch]$SkipSetup
)

$ErrorActionPreference = "Stop"

$binLocal = "target/$Target/release/webcam_to_hdmi"
$serviceLocal = "deploy/webcam_to_hdmi.service"
$setupLocal = "deploy/setup_pi.sh"
$buildOnPiLocal = "deploy/build_on_pi.sh"

if (-not (Test-Path $setupLocal)) {
    throw "Missing setup script: $setupLocal"
}

if ($BuildOnPi -and -not (Test-Path $buildOnPiLocal)) {
    throw "Missing build script: $buildOnPiLocal"
}

if ($BuildOnPi -and $NoBuild) {
    throw "-BuildOnPi and -NoBuild are mutually exclusive."
}

if (-not $NoBuild -and -not $BuildOnPi) {
    if (Test-Path $binLocal) {
        Write-Host "Removing existing binary artifact to force a fresh rebuild..."
        Remove-Item $binLocal -Force
    }

    Write-Host "Building release binary with cross..."
    cross build --release --target $Target

    if ($LASTEXITCODE -ne 0) {
        throw "cross build failed (exit code $LASTEXITCODE). Ensure Docker is running, or rerun with -NoBuild to deploy an already-built binary intentionally."
    }
}

if (-not $BuildOnPi -and -not (Test-Path $binLocal)) {
    throw "Binary not found after build: $binLocal"
}

if (-not $SkipSetup) {
    Write-Host "Copying and running Raspberry Pi setup script..."
    scp $setupLocal "$UserName@$HostName`:/tmp/webcam_to_hdmi_setup.sh"
    ssh "$UserName@$HostName" "chmod +x /tmp/webcam_to_hdmi_setup.sh; sudo /tmp/webcam_to_hdmi_setup.sh"
}

if ($BuildOnPi) {
    Write-Host "Syncing source to Raspberry Pi for native build..."
    ssh "$UserName@$HostName" "rm -rf /tmp/webcam_to_hdmi-src; mkdir -p /tmp/webcam_to_hdmi-src"

    if (Test-Path "Cargo.lock") {
        scp "Cargo.toml" "Cargo.lock" "$UserName@$HostName`:/tmp/webcam_to_hdmi-src/"
    }
    else {
        scp "Cargo.toml" "$UserName@$HostName`:/tmp/webcam_to_hdmi-src/"
    }

    scp -r "src" "$UserName@$HostName`:/tmp/webcam_to_hdmi-src/"
    scp $buildOnPiLocal "$UserName@$HostName`:/tmp/webcam_to_hdmi_build.sh"

    Write-Host "Building release binary on Raspberry Pi..."
    ssh "$UserName@$HostName" "chmod +x /tmp/webcam_to_hdmi_build.sh; /tmp/webcam_to_hdmi_build.sh /tmp/webcam_to_hdmi-src /tmp/webcam_to_hdmi"
}
else {
    Write-Host "Copying binary to $UserName@${HostName}:/tmp/webcam_to_hdmi ..."
    scp $binLocal "$UserName@$HostName`:/tmp/webcam_to_hdmi"
}

Write-Host "Installing binary at $InstallPath ..."
ssh "$UserName@$HostName" "sudo install -m 0755 /tmp/webcam_to_hdmi $InstallPath"

if ($InstallService) {
    Write-Host "Copying systemd units..."
    scp $serviceLocal "$UserName@$HostName`:/tmp/webcam_to_hdmi.service"

    $remote = @(
        "sudo install -m 0644 /tmp/webcam_to_hdmi.service /etc/systemd/system/webcam_to_hdmi.service",
        "sudo systemctl daemon-reload",
        "sudo systemctl disable --now webcam_weston.service || true",
        "sudo systemctl enable webcam_to_hdmi.service",
        "sudo systemctl restart webcam_to_hdmi.service",
        "sudo systemctl --no-pager --full status webcam_to_hdmi.service"
    ) -join "; "

    ssh "$UserName@$HostName" $remote
}

Write-Host "Done. To test manually:"
Write-Host "  ssh $UserName@$HostName"
Write-Host "  sudo $InstallPath"
