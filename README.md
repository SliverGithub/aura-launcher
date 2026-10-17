# Aura Launcher

A terminal Steam client with a library, store, community/chat, market, inventory,
downloads and account dashboard. The Steam protocol engine is Aurelia (included in
this package).

## Build

Requirements:

- Rust 1.88 or newer (`rustup` recommended: https://rustup.rs)
- A C compiler and `pkg-config`
- Linux only: the D-Bus development headers. On Debian/Ubuntu:
  `sudo apt install build-essential pkg-config libdbus-1-dev`

Then:

```sh
./build.sh
```

The first build downloads the Rust dependencies. When it finishes:

```sh
./target/release/aura
```

The build produces two binaries:

- `target/release/aura` — Aura Launcher
- `target/release/aurelia` — the Steam engine used by the launcher

Keep them together; `aura` looks for `aurelia` next to itself.

## First run

Press `l` in Aura Launcher to log in with Steam. The launcher reads the Steam
library already installed by the official client, so existing games do not need
to be reinstalled.

Useful keys:

- `1`-`6`: Library / Store / Community / Market / Downloads / Account
- `l`: log in, `R` in Account: reconnect a stale session
- `m`: per-game management (Proton runtime, platform, runner, prefix)
- `i` in Market: load all inventories, `I`: one game
- `/`: search/filter, `[`/`]`: market pages, `PgUp`/`PgDn`: scroll details
- `?`: help, `q`: quit

## Notes

- Aura Launcher is unofficial and not affiliated with Valve.
- Family-Shared games can be installed; launching them uses the host Steam client.
- If the terminal does not support graphics, artwork is omitted automatically.

The original Aurelia documentation is in `README-AURELIA.md`.
