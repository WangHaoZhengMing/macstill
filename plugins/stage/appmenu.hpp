// Standard KDE appmenu surface association. Menu transport remains on session D-Bus.
#include <algorithm>
#include <cstring>
#include <wayland-server-core.h>
#include <hyprland/src/Compositor.hpp>
#include <hyprland/src/xwayland/XSurface.hpp>
#include "appmenu-server.h"
namespace AppMenu {
struct Menu {
    wl_resource* resource;
    WP<CWLSurfaceResource> surface;
    std::string service, path;
};
static wl_global* global = nullptr;
static std::set<wl_resource*> managers;
static std::vector<Menu*> menus;
static void release(wl_client*, wl_resource* resource) { wl_resource_destroy(resource); }
static void setAddress(wl_client*, wl_resource* resource, const char* service, const char* path) {
    auto* menu = static_cast<Menu*>(wl_resource_get_user_data(resource));
    // D-Bus validates names again in the host. Bound memory on the compositor side.
    if (std::strlen(service) > 255 || std::strlen(path) > 4096) return;
    menu->service = service; menu->path = path;
}
static void destroyMenu(wl_resource* resource) {
    auto* menu = static_cast<Menu*>(wl_resource_get_user_data(resource));
    std::erase(menus, menu);
    delete menu;
}
static const struct org_kde_kwin_appmenu_interface menuImpl = {setAddress, release};
static void create(wl_client* client, wl_resource* manager, uint32_t id, wl_resource* surface) {
    const auto version = wl_resource_get_version(manager);
    auto* resource = wl_resource_create(client, &org_kde_kwin_appmenu_interface, version, id);
    if (!resource) { wl_client_post_no_memory(client); return; }
    auto* menu = new Menu{resource, CWLSurfaceResource::fromResource(surface), {}, {}};
    menus.push_back(menu);
    wl_resource_set_implementation(resource, &menuImpl, menu, destroyMenu);
}
static const struct org_kde_kwin_appmenu_manager_interface managerImpl = {create, release};
static void bind(wl_client* client, void*, uint32_t version, uint32_t id) {
    auto* resource = wl_resource_create(client, &org_kde_kwin_appmenu_manager_interface, std::min(version, 2u), id);
    if (!resource) { wl_client_post_no_memory(client); return; }
    managers.insert(resource);
    wl_resource_set_implementation(resource, &managerImpl, nullptr, [](wl_resource* r) { managers.erase(r); });
}
static void init() {
    global = wl_global_create(g_pCompositor->m_wlDisplay, &org_kde_kwin_appmenu_manager_interface, 2, nullptr, bind);
    if (!global) throw std::runtime_error("Cannot advertise appmenu protocol");
}
static void finish() {
    if (global) { wl_global_destroy(global); global = nullptr; }
    while (!menus.empty()) wl_resource_destroy(menus.back()->resource);
    while (!managers.empty()) wl_resource_destroy(*managers.begin());
}
static Json windows() {
    Json result = Json::array();
    for (const auto& window : Desktop::windowState()->windows()) {
        if (!window->m_isMapped) continue;
        Json record{{"address", std::format("0x{:x}", reinterpret_cast<uintptr_t>(window.get()))}};
        if (auto x = window->m_xwaylandSurface.lock(); window->m_isX11 && x)
            record["xid"] = x->m_xID;
        auto surface = window->wlSurface()->resource();
        for (auto it = menus.rbegin(); it != menus.rend(); ++it) {
            auto* menu = *it;
            if (surface && menu->surface.lock() == surface && !menu->service.empty() && !menu->path.empty()) {
                record["endpoint"] = Json{{"owner", menu->service}, {"path", menu->path}};
                break;
            }
        }
        result.push_back(std::move(record));
    }
    return result;
}
}
