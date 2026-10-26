//! Virtual pointer for the isolated Stage Manager live test. Reads x y width height on stdin.
use std::io::{self, BufRead};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, protocol::wl_registry};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};
#[derive(Default)]
struct State {
    manager: Option<ZwlrVirtualPointerManagerV1>,
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
            && interface == "zwlr_virtual_pointer_manager_v1"
        {
            state.manager = Some(registry.bind(name, version.min(2), qh, ()));
        }
    }
}
delegate_noop!(State: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ignore ZwlrVirtualPointerV1);
fn main() -> anyhow::Result<()> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state)?;
    let manager = state
        .manager
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No virtual pointer protocol"))?;
    let pointer = manager.create_virtual_pointer(None, &qh, ());
    queue.roundtrip(&mut state)?;
    let started = std::time::Instant::now();
    for line in io::stdin().lock().lines() {
        let line=line?;
        if line == "down" || line == "up" {
            use wayland_client::protocol::wl_pointer::ButtonState;
            pointer.button(started.elapsed().as_millis() as u32, 0x110,
                if line=="down" {ButtonState::Pressed}else{ButtonState::Released});
            pointer.frame();queue.roundtrip(&mut state)?;continue;
        }
        let coordinates = line
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        anyhow::ensure!(coordinates.len() == 4, "Expected x y width height");
        pointer.motion_absolute(
            started.elapsed().as_millis() as u32,
            coordinates[0],
            coordinates[1],
            coordinates[2],
            coordinates[3],
        );
        pointer.frame();
        queue.roundtrip(&mut state)?;
    }
    pointer.destroy();
    connection.flush()?;
    Ok(())
}
