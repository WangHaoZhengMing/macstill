//! One-shot full-resolution output capture for Screenshot.
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

use crate::{Buffer, decode, validate_buffer};
use wayland_client::protocol::wl_output;
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1 as frame, zwlr_screencopy_manager_v1 as manager,
};

#[derive(Default)]
struct State {
    manager: Option<manager::ZwlrScreencopyManagerV1>,
    shm: Option<wl_shm::WlShm>,
    outputs: Vec<(wl_output::WlOutput, String)>,
    buffer: Option<Buffer>,
    ready: bool,
    inverted: bool,
    error: Option<String>,
}
/// Capture one named output at its physical resolution, on a private connection.
/// Includes the compositor's output transform. Never starts a persistent stream.
pub fn capture_output(name: &str, cursor: bool, timeout: Duration) -> Result<RgbaImage> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut state = State::default();
    let deadline = Instant::now() + timeout;
    while state.manager.is_none()
        || state.shm.is_none()
        || !state.outputs.iter().any(|(_, n)| n == name)
    {
        pump(&mut queue, &mut state, deadline)?;
    }
    let output = &state.outputs.iter().find(|(_, n)| n == name).unwrap().0;
    let frame = state.manager.as_ref().unwrap().capture_output(
        i32::from(cursor),
        output,
        &queue.handle(),
        (),
    );
    let result = (|| {
        while !state.ready {
            pump(&mut queue, &mut state, deadline)?;
            if let Some(error) = &state.error {
                bail!("{error}");
            }
        }
        let buffer = state
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
            state.inverted,
        )
    })();
    frame.destroy();
    if let Some(buffer) = state.buffer.take() {
        buffer.proxy.destroy();
    }
    result
}
fn pump(queue: &mut EventQueue<State>, state: &mut State, deadline: Instant) -> Result<()> {
    queue.dispatch_pending(state)?;
    ensure!(
        Instant::now() < deadline,
        "Screen capture timed out; output or screencopy protocol unavailable"
    );
    queue.flush()?;
    if let Some(guard) = queue.prepare_read() {
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
    queue.dispatch_pending(state)?;
    Ok(())
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
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "zwlr_screencopy_manager_v1" if version >= 3 => {
                    state.manager = Some(registry.bind(name, 3, qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "wl_output" if version >= 4 => state
                    .outputs
                    .push((registry.bind(name, 4, qh, ()), String::new())),
                _ => {}
            }
        }
    }
}
impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            if let Some((_, n)) = state.outputs.iter_mut().find(|(p, _)| p == proxy) {
                *n = name;
            }
        }
    }
}
impl Dispatch<frame::ZwlrScreencopyFrameV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &frame::ZwlrScreencopyFrameV1,
        event: frame::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
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
                        "macstill-screenshot",
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
                    proxy.copy(&buffer.proxy);
                }
                frame::Event::Flags {
                    flags: WEnum::Value(flags),
                } => state.inverted = flags.contains(frame::Flags::YInvert),
                frame::Event::Ready { .. } => state.ready = true,
                frame::Event::Failed => bail!("The compositor could not capture this output"),
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
delegate_noop!(State: ignore manager::ZwlrScreencopyManagerV1);
