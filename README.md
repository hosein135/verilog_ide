# Verilog IDE

Desktop IDE for **Verilog** HDL and **testbenches**, written in Rust with [iced](https://github.com/iced-rs/iced) (CPU software renderer — no GPU required).

## Features

- Project explorer for `.v`, `.sv`, and `.vh` files, with file-type icons
- Light theme
- Multi-tab code editor with line numbers and syntax highlighting
- New module / new testbench templates
- Half adder sample (`samples/half_adder`): `half_adder.v` and `half_adder_tb.v`. The IDE opens it on startup. Run simulates both with xezim, writes `half_adder.vcd`, and opens the waveform.
- **Run** button (F5): simulate Verilog + testbench with [xezim](https://github.com/aionhw/xezim) and write a `.vcd` waveform
- **Waveform / Text Editor** modes: each `.vcd` in the explorer has **Text** and **Wave** controls. The same pair sits on the editor toolbar while that `.vcd` is open. Traces are drawn with [wellen](https://github.com/ekiwi/wellen) from [Surfer](https://gitlab.com/surfer-project/surfer)
- **Anon VCD**: flatten and rename the open `.vcd` with [vcd-anon](https://gitlab.com/surfer-project/vcd-anon). Writes `name.anon.vcd` and `name.anon.mapping.txt`
- **To SAIF**: convert a `.vcd` / `.fst` / `.ghw` dump to a switching-activity `.saif` with [wave2saif](https://gitlab.com/surfer-project/wave2saif)
- Console + problems panel, find, save shortcuts

## Windows

`Verilog-IDE-0.1.0-x64.msi` is a 64-bit installer. It installs Verilog IDE for the current user under `%LOCALAPPDATA%\Verilog IDE`, adds Start menu and desktop shortcuts with the application icon, and installs the half-adder sample next to the program so Run can write `half_adder.vcd`.

## Setup and run (Linux / macOS / WSL)

```bash
chmod +x run.sh
./run.sh
```

What `run.sh` does:

1. Install **curl** (static binary) if missing
2. Install **Nix** (official installer) if missing
3. Enter a **nixpkgs 25.05** dev shell from `devops/flake.nix` (Rust **1.92.0**, iced + xezim + wellen/Surfer build deps)
4. Cache the Nix environment under `~/.cache/verilog-ide/` for fast later runs
5. `cargo run` the IDE inside that shell

## Reproducibility

- **Nix**: `devops/flake.lock` pins nixpkgs and rust-overlay; the shell uses Rust **1.92.0** exactly
- **Cargo**: committed `Cargo.lock` pins crates.io; xezim, Surfer, vcd-anon, and wave2saif are vendored under `vendor/` so `cargo build` does not fetch their git repos
- **Toolchain file**: `rust-toolchain.toml` selects 1.92.0 for rustup-based builds outside Nix

Useful flags:

| Flag | Meaning |
|------|---------|
| `--prep-only` | Install/cache Nix env only; do not launch |
| `--build` | `cargo build` only (no run) |
| `--release` | Release build / run |
| `--force-setup` | Re-fetch Nix packages into the system cache |

```bash
./run.sh --prep-only
./run.sh --release
./run.sh --build --release
./run.sh --force-setup
```

## Simulate (VCD)

Open a folder that contains RTL plus a testbench, then click **Run** (or press **F5**).

The bundled sample is `samples/half_adder/half_adder.v` and `half_adder_tb.v`. Run simulates them with the bundled [xezim](https://github.com/aionhw/xezim) simulator **in-process** and writes `half_adder.vcd`. Waveform support is enabled the same way as xezim `--wave`.

## Waveforms (Surfer / wellen)

[Surfer](https://gitlab.com/surfer-project/surfer) is vendored at `vendor/surfer` (`v0.7.0`). Its parser, [wellen](https://github.com/ekiwi/wellen), is a Cargo dependency and is compiled with the rest of the crate (same pattern as xezim).

On a `.vcd` row in the explorer, click **Wave** to draw traces in the editor pane (scroll to zoom, drag to pan, click to place a cursor), or **Text** to read the dump as text. The same **Text** / **Wave** pair is on the toolbar while that `.vcd` is open. Other dumps (`.fst` / `.ghw`) still open from **View → Waveform** or **View → Text Editor**.

After **Run**, if Waveform mode is on, the new `.vcd` opens as traces automatically.

## VCD tools

**Anon VCD** and **To SAIF** sit in the menu bar, under **Run**, and on the waveform toolbar. Each one uses the active waveform when it matches, otherwise an open waveform tab, otherwise the newest dump in the folder.

| Button | Tool | Result |
|--------|------|--------|
| Anon VCD | [vcd-anon](https://gitlab.com/surfer-project/vcd-anon) | `name.anon.vcd` plus `name.anon.mapping.txt` |
| To SAIF | [wave2saif](https://gitlab.com/surfer-project/wave2saif) | `name.saif` |

Anon VCD only accepts `.vcd`. To SAIF also accepts `.fst` and `.ghw`. Both crates are path dependencies, so `cargo build` compiles them with the IDE.

## Manual run (inside Nix shell)

```bash
nix develop devops
cargo run
# or
cargo run --release
```

## Layout

```
src/              Rust IDE sources (iced)
src/sim.rs        in-process xezim runner (VCD)
src/waveform.rs   Surfer/wellen waveform pane
vendor/xezim      Vendored [xezim](https://github.com/aionhw/xezim) (no git fetch)
vendor/xezim-core Vendored xezim-core (xezim path/patch dep)
vendor/surfer     Vendored [Surfer](https://gitlab.com/surfer-project/surfer) v0.7.0
vendor/vcd-anon   Vendored [vcd-anon](https://gitlab.com/surfer-project/vcd-anon)
vendor/wave2saif  Vendored [wave2saif](https://gitlab.com/surfer-project/wave2saif)
samples/half_adder  Verilog half adder and testbench
assets/           Application icon
installer/        WiX source for the Windows installer
Cargo.lock        Pinned Rust crate graph
rust-toolchain.toml  Rust 1.92.0 (rustup)
devops/flake.nix  Nix dev shell (nixpkgs 25.05)
devops/flake.lock Pinned Nix inputs
run.sh            Bootstrap Nix + run inside flake env
```

## GUI stack

- [iced](https://github.com/iced-rs/iced) with the **tiny-skia** software renderer (CPU-only, no Vulkan/OpenGL/nixGL)
- `ICED_BACKEND=tiny-skia` is set in `.cargo/config.toml` and the Nix shell hook

Works on non-NixOS Linux without [nixGL](https://github.com/nix-community/nixGL) or GPU drivers.
