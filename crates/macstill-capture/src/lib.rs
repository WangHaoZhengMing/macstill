//! Bounded Wayland window and output capture. Images stay in memory until the caller saves them.
use anyhow::{Context, Result, bail, ensure};
use image::RgbaImage;
use std::{
    fs::File,
    os::{fd::AsFd, unix::fs::FileExt},
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop,
    protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool},
};

#[allow(
    dead_code,
    unused_imports,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    clippy::all
)]
mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;
    use wayland_protocols_wlr::foreign_toplevel::v1::client::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols_wlr::foreign_toplevel::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-toplevel-export-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/hyprland-toplevel-export-v1.xml");
}
use protocol::{
    hyprland_toplevel_export_frame_v1 as frame, hyprland_toplevel_export_manager_v1 as manager,
};

mod output;
pub use output::capture_output;

mod service;
pub use service::{Service, Snapshot, Target, Texture};

const MAX_BYTES: u32 = 64 * 1024 * 1024;
struct Buffer {
    file: File,
    proxy: wl_buffer::WlBuffer,
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
}
#[derive(Default)]
struct State {
    manager: Option<manager::HyprlandToplevelExportManagerV1>,
    shm: Option<wl_shm::WlShm>,
    buffer: Option<Buffer>,
    ready: bool,
    inverted: bool,
    error: Option<String>,
    generation: u64,
}
/// One dedicated connection, never sharing GPUI's event queue.
pub struct Capture {
    queue: EventQueue<State>,
    state: State,
}
impl Capture {
    pub fn connect() -> Result<Self> {
        let connection =
            Connection::connect_to_env().context("Cannot connect window capture to Wayland")?;
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut capture = Self {
            queue,
            state: State::default(),
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        while capture.state.manager.is_none() || capture.state.shm.is_none() {
            capture.pump(deadline)?;
        }
        Ok(capture)
    }
    fn pump(&mut self, deadline: Instant) -> Result<()> {
        self.queue.dispatch_pending(&mut self.state)?;
        ensure!(
            Instant::now() < deadline,
            "Window capture timed out or protocol unavailable"
        );
        self.queue.flush()?;
        if let Some(guard) = self.queue.prepare_read() {
            let fd = guard.connection_fd();
            let mut polls = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN,
            )];
            let timeout = rustix::event::Timespec::try_from(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(50)),
            )?;
            if rustix::event::poll(&mut polls, Some(&timeout))? > 0 {
                guard.read()?;
            }
        }
        self.queue.dispatch_pending(&mut self.state)?;
        Ok(())
    }
    /// Captures a single real toplevel, including windows on inactive workspaces.
    /// The compositor chooses source dimensions; allocation and wait time are bounded.
    pub fn capture(&mut self, address: &str, timeout: Duration) -> Result<RgbaImage> {
        self.capture_with_cursor(address, false, timeout)
    }
    pub fn capture_with_cursor(
        &mut self,
        address: &str,
        cursor: bool,
        timeout: Duration,
    ) -> Result<RgbaImage> {
        ensure!(
            address.starts_with("0x") && address.len() > 2,
            "Invalid window handle"
        );
        let handle = u64::from_str_radix(&address[2..], 16)? as u32;
        self.state.ready = false;
        self.state.inverted = false;
        self.state.error = None;
        self.state.generation += 1;
        let manager = self
            .state
            .manager
            .as_ref()
            .context("Window export is unavailable")?;
        let frame = manager.capture_toplevel(
            i32::from(cursor),
            handle,
            &self.queue.handle(),
            self.state.generation,
        );
        let deadline = Instant::now() + timeout;
        let result = (|| {
            while !self.state.ready {
                self.pump(deadline)?;
                if let Some(error) = &self.state.error {
                    bail!("{error}");
                }
            }
            let buffer = self
                .state
                .buffer
                .as_ref()
                .context("Capture returned no buffer")?;
            let mut bytes = vec![0; (buffer.stride * buffer.height) as usize];
            buffer.file.read_exact_at(&mut bytes, 0)?;
            decode(
                &bytes,
                buffer.width,
                buffer.height,
                buffer.stride,
                buffer.format,
                self.state.inverted,
            )
        })();
        frame.destroy();
        if let Some(buffer) = self.state.buffer.take() {
            buffer.proxy.destroy();
        }
        let _ = self.queue.flush();
        result
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match interface.as_str() {
                "hyprland_toplevel_export_manager_v1" => {
                    state.manager = Some(registry.bind(name, 1, qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}
impl Dispatch<frame::HyprlandToplevelExportFrameV1, u64> for State {
    fn event(
        state: &mut Self,
        proxy: &frame::HyprlandToplevelExportFrameV1,
        event: frame::Event,
        generation: &u64,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if *generation != state.generation {
            return;
        }
        let result = (|| -> Result<()> {
            match event {
                frame::Event::Buffer {
                    format,
                    width,
                    height,
                    stride,
                } => {
                    let WEnum::Value(format) = format else {
                        bail!("Unsupported capture format")
                    };
                    validate_buffer(width, height, stride, format)?;
                    let file = File::from(rustix::fs::memfd_create(
                        "macstill-window",
                        rustix::fs::MemfdFlags::CLOEXEC,
                    )?);
                    file.set_len(u64::from(stride) * u64::from(height))?;
                    let pool = state
                        .shm
                        .as_ref()
                        .context("No Wayland shared memory")?
                        .create_pool(file.as_fd(), (stride * height) as i32, qh, ());
                    let buffer = pool.create_buffer(
                        0,
                        width as i32,
                        height as i32,
                        stride as i32,
                        format,
                        qh,
                        (),
                    );
                    pool.destroy();
                    state.buffer = Some(Buffer {
                        file,
                        proxy: buffer,
                        width,
                        height,
                        stride,
                        format,
                    });
                }
                frame::Event::BufferDone => {
                    let buffer = state
                        .buffer
                        .as_ref()
                        .context("No supported capture buffer")?;
                    proxy.copy(&buffer.proxy, 1);
                }
                frame::Event::Flags {
                    flags: WEnum::Value(flags),
                } => state.inverted = flags.contains(frame::Flags::YInvert),
                frame::Event::Ready { .. } => state.ready = true,
                frame::Event::Failed => bail!("The compositor could not capture this window"),
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            state.error = Some(error.to_string());
        }
    }
}
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore manager::HyprlandToplevelExportManagerV1);

fn validate_buffer(width: u32, height: u32, stride: u32, format: wl_shm::Format) -> Result<()> {
    ensure!(
        matches!(
            format,
            wl_shm::Format::Argb8888
                | wl_shm::Format::Xrgb8888
                | wl_shm::Format::Abgr8888
                | wl_shm::Format::Xbgr8888
        ),
        "Unsupported window pixel format: {format:?}"
    );
    ensure!(
        width > 0
            && height > 0
            && width <= 16384
            && height <= 16384
            && stride >= width * 4
            && stride
                .checked_mul(height)
                .is_some_and(|bytes| bytes <= MAX_BYTES),
        "Window capture buffer exceeds limits"
    );
    Ok(())
}
fn decode(
    bytes: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
    inverted: bool,
) -> Result<RgbaImage> {
    validate_buffer(width, height, stride, format)?;
    ensure!(
        bytes.len() >= (stride * height) as usize,
        "Truncated capture buffer"
    );
    let mut image = RgbaImage::new(width, height);
    let bgr = matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888);
    let alpha = matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Abgr8888);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let row = if inverted { height - y - 1 } else { y };
        let offset = (row * stride + x * 4) as usize;
        let p = &bytes[offset..offset + 4];
        let [r, g, b] = if bgr {
            [p[2], p[1], p[0]]
        } else {
            [p[0], p[1], p[2]]
        };
        *pixel = image::Rgba([r, g, b, if alpha { p[3] } else { 255 }]);
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_stride_channel_order_and_inverted_rows() {
        let bytes = [30, 20, 10, 0, 0, 0, 0, 0, 60, 50, 40, 0, 0, 0, 0, 0];
        let image = decode(&bytes, 1, 2, 8, wl_shm::Format::Xrgb8888, true).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [40, 50, 60, 255]);
        assert_eq!(image.get_pixel(0, 1).0, [10, 20, 30, 255]);
        assert_eq!(
            decode(&[1, 2, 3, 4], 1, 1, 4, wl_shm::Format::Abgr8888, false)
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [1, 2, 3, 4]
        );
    }
    #[test]
    fn rejects_unbounded_overflowing_and_truncated_buffers() {
        for (w, h, stride) in [
            (0, 2, 8),
            (20000, 1, 80000),
            (100, 100, u32::MAX),
            (4000, 8000, 16000),
            (10, 10, 20),
        ] {
            assert!(validate_buffer(w, h, stride, wl_shm::Format::Argb8888).is_err());
        }
        assert!(decode(&[0; 3], 1, 1, 4, wl_shm::Format::Xrgb8888, false).is_err());
    }
}
