# About Macstill and System Information

`macstill-info` owns two independent native GPUI windows. Apple menu → About Macstill opens the compact summary; More Info opens or activates System Information without closing About. Repeating either action reuses its window in the current display session.

```bash
cargo run -p macstill-info -- --about
cargo run -p macstill-info -- --report
cargo run -p macstill-info -- --json
```

The shell exposes the same entry points as `macstill about` and `macstill info`. Local and release installers include the new companion and System Information desktop entry.

Inventory runs off the UI thread and reads `/proc`, `/sys`, and `/etc/os-release`. It reports CPU, memory, graphics drivers and display modes, PCI devices, power supplies, storage, USB topology, network interfaces, audio, camera devices, and the operating system. Refresh reloads the inventory in both windows. Missing files and restricted serial numbers remain unavailable; no elevated access, external commands, or system writes are requested. Graphics modes come from the kernel connector inventory; network interfaces are not live NetworkManager connections. RAM totals use binary units.

```bash
cargo test -p macstill-info
cargo clippy -p macstill-info --all-targets -- -D warnings
cargo test -p macstill-info info_visual_review -- --ignored --nocapture
```

The last check uses isolated fixture data and the real offscreen renderer, saving light, dark, and small-window PNGs under `target/info-review/`. Tests verify that More Info opens a second window, repeated clicks reuse it, category and device selection work, USB parents precede their children, and the private session socket does not overwrite regular files.
