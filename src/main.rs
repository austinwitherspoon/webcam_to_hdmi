#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("webcam_to_hdmi currently supports Linux only (Raspberry Pi target).");
}

#[cfg(target_os = "linux")]
mod app {
    use gstreamer as gst;
    use gstreamer::prelude::*;
    use std::env;
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Runtime configuration for a single HDMI output pipeline.
    #[derive(Clone)]
    struct OutputConfig {
        cam_dev: String,
        connector_id: u32,
        plane_id: Option<u32>,
        width: u32,
        height: u32,
        fps: u32,
        poll_ms: u64,
    }

    #[derive(Copy, Clone, Eq, PartialEq)]
    enum DesiredState {
        Camera,
        Black,
    }

    struct OutputHandle {
        label: &'static str,
        state: DesiredState,
        pipeline: gst::Pipeline,
        bus: gst::Bus,
        // Keep duplicated fd alive for the lifetime of this pipeline.
        _drm_fd: Option<File>,
    }

    fn env_or_default(key: &str, default: &str) -> String {
        env::var(key).unwrap_or_else(|_| default.to_string())
    }

    fn parse_or_default<T>(key: &str, default: T) -> T
    where
        T: std::str::FromStr + Copy,
    {
        env::var(key)
            .ok()
            .and_then(|v| v.parse::<T>().ok())
            .unwrap_or(default)
    }

    fn parse_optional_u32(key: &str) -> Option<u32> {
        env::var(key).ok().and_then(|v| v.parse::<u32>().ok())
    }

    fn desired_state(cfg: &OutputConfig) -> DesiredState {
        if Path::new(&cfg.cam_dev).exists() {
            DesiredState::Camera
        } else {
            DesiredState::Black
        }
    }

    fn quote_gst_string(raw: &str) -> String {
        let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    }

    fn sink_tail(sink_name: &str, cfg: &OutputConfig) -> String {
        let plane = cfg
            .plane_id
            .map(|p| format!(" plane-id={}", p))
            .unwrap_or_default();
        format!(
            "kmssink name={sink_name} connector-id={connector}{plane} force-modesetting=true can-scale=false sync=false",
            sink_name = sink_name,
            connector = cfg.connector_id,
            plane = plane
        )
    }

    fn build_chain_desc(
        cfg: &OutputConfig,
        state: DesiredState,
        sink_name: &str,
    ) -> String {
        match state {
            DesiredState::Camera => {
                let dev = quote_gst_string(&cfg.cam_dev);
                format!(
                    "v4l2src device={dev} do-timestamp=true ! image/jpeg,framerate={fps}/1 ! jpegdec ! videoconvert ! video/x-raw,format=NV12 ! queue leaky=downstream max-size-buffers=2 max-size-time=0 max-size-bytes=0 ! {sink}",
                    dev = dev,
                    fps = cfg.fps,
                    sink = sink_tail(sink_name, cfg)
                )
            }
            DesiredState::Black => format!(
                "videotestsrc is-live=true pattern=black ! video/x-raw,width={width},height={height},framerate={fps}/1 ! queue leaky=downstream max-size-buffers=2 max-size-time=0 max-size-bytes=0 ! {sink}",
                fps = cfg.fps,
                width = cfg.width,
                height = cfg.height,
                sink = sink_tail(sink_name, cfg)
            ),
        }
    }

    // KMS mode needs a shared DRM fd passed to each kmssink instance.
    fn set_sink_properties(
        pipeline: &gst::Pipeline,
        label: &'static str,
        drm_fd: i32,
    ) -> bool {
        let Some(sink) = pipeline.by_name("sink") else {
            eprintln!("[SUPERVISOR:{}] pipeline missing sink", label);
            return false;
        };

        sink.set_property("fd", drm_fd);
        true
    }

    fn spawn_output_pipeline(
        label: &'static str,
        cfg: &OutputConfig,
        state: DesiredState,
        drm_master: &File,
    ) -> Option<OutputHandle> {
        let desc = build_chain_desc(cfg, state, "sink");

        let element = match gst::parse::launch(&desc) {
            Ok(elem) => elem,
            Err(err) => {
                eprintln!("[SUPERVISOR:{}] failed to parse pipeline: {}", label, err);
                return None;
            }
        };

        let pipeline = match element.downcast::<gst::Pipeline>() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("[SUPERVISOR:{}] parsed element is not a pipeline", label);
                return None;
            }
        };

        let (drm_fd_hold, drm_fd_raw) = match drm_master.try_clone() {
            Ok(cloned) => {
                let raw = cloned.as_raw_fd();
                (Some(cloned), raw)
            }
            Err(err) => {
                eprintln!(
                    "[SUPERVISOR:{}] failed to clone DRM fd for kmssink: {}",
                    label, err
                );
                return None;
            }
        };

        if !set_sink_properties(&pipeline, label, drm_fd_raw) {
            let _ = pipeline.set_state(gst::State::Null);
            return None;
        }

        if let Err(err) = pipeline.set_state(gst::State::Playing) {
            eprintln!("[SUPERVISOR:{}] failed to set PLAYING state: {}", label, err);
            let _ = pipeline.set_state(gst::State::Null);
            return None;
        }

        let Some(bus) = pipeline.bus() else {
            eprintln!("[SUPERVISOR:{}] pipeline has no bus", label);
            let _ = pipeline.set_state(gst::State::Null);
            return None;
        };

        let state_label = match state {
            DesiredState::Camera => "camera",
            DesiredState::Black => "black",
        };

        println!("[SUPERVISOR:{}] started pipeline ({})", label, state_label);

        Some(OutputHandle {
            label,
            state,
            pipeline,
            bus,
            _drm_fd: drm_fd_hold,
        })
    }

    fn stop_pipeline(handle: &OutputHandle) {
        if let Err(err) = handle.pipeline.set_state(gst::State::Null) {
            eprintln!(
                "[SUPERVISOR:{}] failed to stop pipeline cleanly: {}",
                handle.label, err
            );
        }
    }

    fn pipeline_failed(handle: &OutputHandle) -> bool {
        while let Some(msg) = handle.bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(0),
            &[gst::MessageType::Error, gst::MessageType::Eos],
        ) {
            match msg.view() {
                gst::MessageView::Error(err) => {
                    eprintln!(
                        "[SUPERVISOR:{}] pipeline error from {:?}: {} ({:?})",
                        handle.label,
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    return true;
                }
                gst::MessageView::Eos(..) => {
                    eprintln!("[SUPERVISOR:{}] pipeline reached EOS", handle.label);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn run_supervisor(
        cfg1: OutputConfig,
        cfg2: OutputConfig,
        drm_master: &File,
    ) {
        // If a camera stream flaps, back off briefly and show black to avoid
        // fast restart loops that cause visible flicker.
        let camera_retry_ms = parse_or_default("CAMERA_RETRY_MS", 5000_u64);
        let camera_retry = Duration::from_millis(camera_retry_ms);

        let mut retry1_until: Option<Instant> = None;
        let mut retry2_until: Option<Instant> = None;

        let initial1 = desired_state(&cfg1);
        let initial2 = desired_state(&cfg2);

        let mut active1 = spawn_output_pipeline("HDMI1", &cfg1, initial1, drm_master);
        let mut active2 = spawn_output_pipeline("HDMI2", &cfg2, initial2, drm_master);
        let poll = Duration::from_millis(cfg1.poll_ms.min(cfg2.poll_ms));

        loop {
            let now = Instant::now();
            let physical1 = desired_state(&cfg1);
            let physical2 = desired_state(&cfg2);

            let wanted1 = if matches!(physical1, DesiredState::Camera)
                && retry1_until.is_some_and(|until| now < until)
            {
                DesiredState::Black
            } else {
                physical1
            };

            let wanted2 = if matches!(physical2, DesiredState::Camera)
                && retry2_until.is_some_and(|until| now < until)
            {
                DesiredState::Black
            } else {
                physical2
            };

            if retry1_until.is_some_and(|until| now >= until) {
                retry1_until = None;
            }
            if retry2_until.is_some_and(|until| now >= until) {
                retry2_until = None;
            }

            if let Some(handle) = active1.as_ref()
                && (pipeline_failed(handle) || handle.state != wanted1)
            {
                if matches!(handle.state, DesiredState::Camera)
                    && matches!(physical1, DesiredState::Camera)
                {
                    retry1_until = Some(now + camera_retry);
                    eprintln!(
                        "[SUPERVISOR:HDMI1] camera unstable; using black for {}ms before retry",
                        camera_retry_ms
                    );
                }
                stop_pipeline(handle);
                active1 = None;
            }

            if let Some(handle) = active2.as_ref()
                && (pipeline_failed(handle) || handle.state != wanted2)
            {
                if matches!(handle.state, DesiredState::Camera)
                    && matches!(physical2, DesiredState::Camera)
                {
                    retry2_until = Some(now + camera_retry);
                    eprintln!(
                        "[SUPERVISOR:HDMI2] camera unstable; using black for {}ms before retry",
                        camera_retry_ms
                    );
                }
                stop_pipeline(handle);
                active2 = None;
            }

            if active1.is_none() {
                active1 = spawn_output_pipeline("HDMI1", &cfg1, wanted1, drm_master);
            }

            if active2.is_none() {
                active2 = spawn_output_pipeline("HDMI2", &cfg2, wanted2, drm_master);
            }

            std::thread::sleep(poll);
        }
    }

    pub fn run() {
        if let Err(err) = gst::init() {
            eprintln!("Failed to initialize GStreamer: {}", err);
            return;
        }

        let width = parse_or_default("WIDTH", 1920_u32);
        let height = parse_or_default("HEIGHT", 1080_u32);
        let fps = parse_or_default("FPS", 30_u32);
        let poll_ms = parse_or_default("POLL_MS", 1000_u64);

        let hdmi1 = OutputConfig {
            cam_dev: env_or_default("USB1_CAM_DEV", "/dev/v4l/by-path/usb-1-1-video-index0"),
            connector_id: parse_or_default("HDMI1_CONNECTOR_ID", 32_u32),
            plane_id: parse_optional_u32("HDMI1_PLANE_ID"),
            width,
            height,
            fps,
            poll_ms,
        };

        let hdmi2 = OutputConfig {
            cam_dev: env_or_default("USB2_CAM_DEV", "/dev/v4l/by-path/usb-1-2-video-index0"),
            connector_id: parse_or_default("HDMI2_CONNECTOR_ID", 41_u32),
            plane_id: parse_optional_u32("HDMI2_PLANE_ID"),
            width,
            height,
            fps,
            poll_ms,
        };

        let drm_card = env_or_default("DRM_CARD_DEV", "/dev/dri/card1");

        // One shared DRM fd avoids kmssink master contention between pipelines.
        let drm_master = match OpenOptions::new().read(true).write(true).open(&drm_card) {
            Ok(file) => file,
            Err(err) => {
                eprintln!(
                    "Failed to open DRM device {} for kmssink backend: {}",
                    drm_card, err
                );
                return;
            }
        };

        println!("Starting webcam_to_hdmi...");
        println!("Render backend: kms");
        println!("Using DRM device {}", drm_card);
        println!(
            "HDMI1: cam={} connector-id={} plane-id={:?} {}x{}@{}",
            hdmi1.cam_dev,
            hdmi1.connector_id,
            hdmi1.plane_id,
            hdmi1.width,
            hdmi1.height,
            hdmi1.fps
        );
        println!(
            "HDMI2: cam={} connector-id={} plane-id={:?} {}x{}@{}",
            hdmi2.cam_dev,
            hdmi2.connector_id,
            hdmi2.plane_id,
            hdmi2.width,
            hdmi2.height,
            hdmi2.fps
        );

        run_supervisor(hdmi1, hdmi2, &drm_master);
    }
}

#[cfg(target_os = "linux")]
fn main() {
    app::run();
}
