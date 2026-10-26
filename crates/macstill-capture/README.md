# Window capture

Dedicated `hyprland-toplevel-export-v1` client and bounded capture worker for Stage Manager.
`Capture` exports one actual toplevel into anonymous `memfd` shared memory; `Service`
checks Hyprland window identity and caches resized frames on a separate thread.
No connection or capture starts simply by importing this crate.

The vendored protocol XML comes from
<https://github.com/hyprwm/hyprland-protocols/blob/main/protocols/hyprland-toplevel-export-v1.xml>
(version 2; SHA-256 `2b8e16da22df333494b616a2c83394dcc9aac04405da535126d68106786441ad`).
Its BSD-3-Clause notice remains in the XML and `../../assets/licenses/hyprland-toplevel-export-v1.txt`.
The rest of this crate follows the workspace license.

Run `cargo test -p macstill-capture`. To inspect a window explicitly:
`cargo run -p macstill-capture --example window -- 0xWINDOW /tmp/window.png`.
