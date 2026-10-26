# Macstill Wayland focus patch

This is `gpui-pre-linux` 0.3.9 from crates.io, under its included Apache-2.0
license. The workspace pins this copy through `[patch.crates-io]`.

Hyprland 0.56.2's `mouseMoveUnified` gives keyboard focus to an interactive
layer surface on pointer motion even with `input:follow_mouse = 0`. Consequently
Macstill's full-screen background cannot permanently advertise `OnDemand`.

Only the `macstill-desktop` namespace, with `Layer::Background` and requested
`KeyboardInteractivity::OnDemand`, uses the following behavior:

- Initially advertise `None`; pointer entry/motion never enables keyboard input.
- On mouse-button release, request `Exclusive` if the desktop is inactive.
  Hyprland focuses a layer when it becomes exclusive. Requesting on release
  avoids Hyprland's `releaseAllMouseButtons()` prematurely ending a desktop drag.
- On the compositor's keyboard-enter event, immediately downgrade to `OnDemand`.
  The desktop can receive rename text and shortcuts without keeping an exclusive
  grab that prevents application clicks from changing focus.
- On keyboard-leave, return to `None`, preventing subsequent background hover
  from stealing the application's keyboard focus.
- On pointer-leave, also return to `None`. Hyprland retains the last focused
  window when a layer takes keyboard focus, and its mouse-button handler skips
  refocus when clicking that same window. Disarming the desktop explicitly runs
  the compositor's `refocusLastWindow` path instead of waiting for keyboard-leave.
  A delayed keyboard-enter after pointer-leave must not re-enable interaction.
  Desktop keyboard interaction therefore lasts while the pointer remains over
  its surface; moving onto an application yields keyboard focus back.

The changes are confined to `src/linux/wayland/window.rs` and `client.rs`.
Other layers, windows, and platforms retain upstream behavior. Keep the namespace
and the Shell's desktop layer options in sync when upgrading this dependency.

Manual compositor regression check (requires the patched Shell running):

1. Focus a text editor, type, move over desktop background without clicking,
   and type again. Both inputs must reach the editor, including English/backspace.
2. Click a desktop file and use its keyboard shortcuts; open Rename from its
   context menu and verify typing, Enter and Escape.
3. Click another app and repeat step 1, including across different apps/outputs.
4. Drag a desktop file from an initially inactive desktop and verify its new
   position persists after release. Keyboard focus is acquired after release;
   before that, keyboard events still belong to the previously focused app.
5. Open/close Launchpad, Control Center and Mission Control and repeat step 1.
6. Focus app A, click the desktop, then move directly back to A and click its
   input. Typing must work without visiting B. Repeat with a desktop rename field
   and fast pointer movements while the keyboard-focus handshake is in flight.
