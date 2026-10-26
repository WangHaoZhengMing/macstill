//! KDE appmenu binding for a toolkit-owned Wayland display and surface.
use crate::Endpoint;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_surface},
};
use wayland_protocols_plasma::appmenu::client::{
    org_kde_kwin_appmenu::OrgKdeKwinAppmenu, org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager,
};
struct State;
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(State: ignore OrgKdeKwinAppmenuManager);
delegate_noop!(State: ignore OrgKdeKwinAppmenu);
pub struct Binding {
    menu: OrgKdeKwinAppmenu,
    manager: OrgKdeKwinAppmenuManager,
    connection: Connection,
    _queue: wayland_client::EventQueue<State>,
    _globals: wayland_client::globals::GlobalList,
}
impl Binding {
    /// # Safety
    /// The toolkit display must remain live until Binding is dropped. Surface must be
    /// live during this call and belong to that display. Drop on the toolkit thread.
    pub unsafe fn new(
        display: raw_window_handle::WaylandDisplayHandle,
        surface: raw_window_handle::WaylandWindowHandle,
        endpoint: &Endpoint,
    ) -> anyhow::Result<Self> {
        let backend = unsafe {
            wayland_backend::client::Backend::from_foreign_display(display.display.as_ptr().cast())
        };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<State>(&connection)?;
        let manager: OrgKdeKwinAppmenuManager = globals.bind(&queue.handle(), 1..=2, ())?;
        let id = unsafe {
            wayland_backend::client::ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                surface.surface.as_ptr().cast(),
            )
        }?;
        let surface = wl_surface::WlSurface::from_id(&connection, id)?;
        let menu = manager.create(&surface, &queue.handle(), ());
        menu.set_address(endpoint.owner.clone(), endpoint.path.clone());
        connection.flush()?;
        Ok(Self {
            menu,
            manager,
            connection,
            _queue: queue,
            _globals: globals,
        })
    }
}
impl Drop for Binding {
    fn drop(&mut self) {
        self.menu.release();
        if self.manager.version() >= 2 {
            self.manager.release();
        }
        let _ = self.connection.flush();
    }
}
