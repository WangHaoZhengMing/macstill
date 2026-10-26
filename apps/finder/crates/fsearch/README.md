# FSearch for Linux

A fuzzy filename index and trigram text index, used directly by Finder or through a JSON-lines Unix-socket daemon. Recursive inotify watching updates the index; periodic reconciliation handles missed events and watch limits. Filesystem traversal skips virtual filesystems and preserves Unix filename bytes internally.

```sh
cargo run -p fsearch -- 'report'
cargo run -p fsearch -- 'ext:rs regex:fn\\s+\\w+'
cargo run -p fsearch -- status
cargo run -p fsearch -- serve
cargo run -p fsearch -- stop
```

`FSEARCH_DIR` overrides the index directory. Otherwise it uses `$XDG_DATA_HOME/fsearch` or `~/.local/share/fsearch`. Run under your normal user account; unreadable paths are skipped.

The `Engine`, `Options` and `Query` types provide the in-process API. No daemon is required by the desktop app.

```sh
cargo test -p fsearch
```
