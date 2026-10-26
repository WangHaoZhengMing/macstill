#pragma once
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <fstream>
#include <cstring>
#include <hyprland/src/managers/input/InputManager.hpp>
#include <hyprland/src/protocols/ShortcutsInhibit.hpp>

// Begin warms the shell without mapping a layer or grabbing its keyboard. Raw
// vertical updates arrive before the directional gesture dispatcher's 5px gate.
static std::string gestureControl, gestureFile;
static CHyprSignalListener swipeBegin, swipeUpdate, swipeEnd;
static uint64_t gestureSequence = 0;
static Vector2D gestureDistance;
enum class GestureAxis { Idle, Pending, Horizontal, Up, Down };
static GestureAxis gestureAxis = GestureAxis::Idle;
static std::map<IPointer*, std::vector<CHyprSignalListener>> holdListeners;

static bool canPrepareGesture() {
    return !gestureControl.empty() && !gestureFile.empty() && !g_pSessionLockManager->isSessionLocked() &&
        g_pInputManager->getModsFromAllKBs() == 0 && !PROTO::shortcutsInhibit->isInhibited();
}

static void notifyShell(const char* command) {
    if (gestureControl.empty()) return;
    sockaddr_un address{};
    address.sun_family = AF_UNIX;
    if (gestureControl.size() >= sizeof(address.sun_path)) return;
    std::memcpy(address.sun_path, gestureControl.c_str(), gestureControl.size() + 1);
    const int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
    if (fd < 0) return;
    if (connect(fd, reinterpret_cast<sockaddr*>(&address), sizeof(address)) == 0)
        send(fd, command, std::strlen(command), MSG_NOSIGNAL | MSG_DONTWAIT);
    close(fd);
}

static void publishGesture(bool active, bool cancelled) {
    const bool up = gestureAxis == GestureAxis::Up;
    const double distance = gestureDistance.y * (up ? -1. : 1.);
    const auto temp = gestureFile + ".tmp";
    {
        std::ofstream file(temp);
        if (!file) return;
        file << Json{{"seq", gestureSequence}, {"direction", up ? "up" : "down"},
                     {"progress", std::clamp(distance / 260., 0., 1.)},
                     {"active", active}, {"cancelled", cancelled}}.dump();
    }
    if (std::rename(temp.c_str(), gestureFile.c_str()) == 0) notifyShell("mission-gesture");
}

static void watchTouchpadHolds() {
    std::set<IPointer*> present;
    for (const auto& pointer : g_pInputManager->m_pointers) {
        if (!pointer->m_isTouchpad) continue;
        present.insert(pointer.get());
        if (holdListeners.contains(pointer.get())) continue;
        auto held = std::make_shared<bool>(false);
        auto& listeners = holdListeners[pointer.get()];
        listeners.push_back(pointer->m_pointerEvents.holdBegin.listen([held](IPointer::SHoldBeginEvent e) {
            *held = e.fingers == 3 && canPrepareGesture();
            if (*held) notifyShell("mission-prepare");
        }));
        listeners.push_back(pointer->m_pointerEvents.holdEnd.listen([held](IPointer::SHoldEndEvent e) {
            // libinput cancels a hold when it becomes a swipe. Preserve its warm
            // scene for that transition; unused preparation expires in the Shell.
            if (*held && !e.cancelled) notifyShell("mission-unprepare");
            *held = false;
        }));
    }
    std::erase_if(holdListeners, [&](const auto& pair) { return !present.contains(pair.first); });
}

static void installGestures() {
    swipeBegin = Event::bus()->m_events.gesture.swipe.begin.listen([](IPointer::SSwipeBeginEvent e, Event::SCallbackInfo&) {
        gestureAxis = GestureAxis::Idle;
        if (e.fingers != 3 || !canPrepareGesture()) return;
        gestureSequence = std::max(gestureSequence + 1, uint64_t(e.timeMs));
        gestureDistance = {};
        gestureAxis = GestureAxis::Pending;
        notifyShell("mission-prepare");
    });
    swipeUpdate = Event::bus()->m_events.gesture.swipe.update.listen([](IPointer::SSwipeUpdateEvent e, Event::SCallbackInfo& info) {
        if (gestureAxis == GestureAxis::Idle || gestureAxis == GestureAxis::Horizontal) return;
        if (g_pSessionLockManager->isSessionLocked() || e.fingers != 3) {
            notifyShell("mission-unprepare");
            gestureAxis = GestureAxis::Idle;
            return;
        }
        gestureDistance += e.delta;
        if (gestureAxis == GestureAxis::Pending) {
            if (std::max(std::abs(gestureDistance.x), std::abs(gestureDistance.y)) < 1.) return;
            if (std::abs(gestureDistance.x) > std::abs(gestureDistance.y)) {
                gestureAxis = GestureAxis::Horizontal;
                notifyShell("mission-unprepare");
                return; // Keep Hyprland's native horizontal workspace gesture.
            }
            gestureAxis = gestureDistance.y < 0 ? GestureAxis::Up : GestureAxis::Down;
        }
        info.cancelled = true; // Do not also invoke Lua's directional fallback.
        publishGesture(true, false);
    });
    swipeEnd = Event::bus()->m_events.gesture.swipe.end.listen([](IPointer::SSwipeEndEvent e, Event::SCallbackInfo& info) {
        if (gestureAxis == GestureAxis::Up || gestureAxis == GestureAxis::Down) {
            info.cancelled = true;
            publishGesture(false, e.cancelled);
        }
        notifyShell("mission-unprepare");
        gestureAxis = GestureAxis::Idle;
    });
}
