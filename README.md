# Verilog IDE

Desktop IDE for **Verilog** and **VHDL**, written in Rust with [iced](https://github.com/iced-rs/iced) (CPU software renderer — no GPU required).

## Features

- Start screen with only **New Verilog**, **New VHDL**, and **Open existing**. Nothing is opened until you choose one.
- Project explorer with file-type icons, and create, rename, and delete for files
- Light theme
- Multi-tab editor with line numbers and syntax highlighting for Verilog and VHDL
- New design / testbench templates for the project's language
- **Samples** menu: Verilog half adder (`samples/half_adder`) and VHDL half adder (`samples/half_adder_vhdl`)
- **Run** (F5): Verilog uses [xezim](https://github.com/aionhw/xezim) and writes a `.vcd`. VHDL uses [yavs](https://github.com/ru551n/yavs) and writes an `.fst`
- **Text / Wave** for the active `.vcd`, `.fst`, or `.ghw` sits on the right of the tab bar. Traces are drawn with [wellen](https://github.com/ekiwi/wellen) from [Surfer](https://gitlab.com/surfer-project/surfer)
- **Anon VCD**: flatten and rename the open `.vcd` with [vcd-anon](https://gitlab.com/surfer-project/vcd-anon). Writes `name.anon.vcd` and `name.anon.mapping.txt`
- **To SAIF**: convert a `.vcd` / `.fst` / `.ghw` dump to a switching-activity `.saif` with [wave2saif](https://gitlab.com/surfer-project/wave2saif)
- Console + problems panel, find, save shortcuts

## Setup and run (Linux / macOS / WSL)

```bash
chmod +x run.sh
./run.sh
```

What `run.sh` does:

1. Install missing **git** and **curl** with the distro package manager (a static curl binary if that cannot run)
2. Check free disk space, repair the git work tree, and install pinned **Nix 2.28.5** when the current Nix is missing or a different version
3. Enter a **nixpkgs 25.05** dev shell from `devops/flake.nix` (Rust **1.92.0**, iced, xezim, yavs, and wellen/Surfer build deps)
4. Cache the Nix environment under `~/.cache/verilog-ide/` for fast later runs
5. `cargo run` the IDE inside that shell

## Reproducibility

- **Nix**: `devops/flake.lock` pins nixpkgs and rust-overlay; the shell uses Rust **1.92.0** exactly
- **Cargo**: committed `Cargo.lock` pins crates.io; xezim, yavs, Surfer, vcd-anon, and wave2saif are vendored under `vendor/` so `cargo build` does not fetch their git repos. `build.rs` compiles the yavs binary into `vendor/yavs/target`
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

## Simulate

Create or open a project, then click **Run** (or press **F5**).

Verilog projects simulate with the bundled [xezim](https://github.com/aionhw/xezim) simulator **in-process** and write a `.vcd`. The sample is `samples/half_adder/half_adder.v` and `half_adder_tb.v`.

VHDL projects run the bundled [yavs](https://github.com/ru551n/yavs) binary (`analyze`, then `run --tier=interp`) and write an `.fst`. The sample is `samples/half_adder_vhdl/half_adder.vhd` and `half_adder_tb.vhd`. yavs needs a C compiler when the IDE is built, because its VHPI shim is compiled from C.

## Waveforms (Surfer / wellen)

[Surfer](https://gitlab.com/surfer-project/surfer) is vendored at `vendor/surfer` (`v0.7.0`). Its parser, [wellen](https://github.com/ekiwi/wellen), is a Cargo dependency and is compiled with the rest of the crate.

Open a `.vcd`, `.fst`, or `.ghw` file, then use **Text** or **Wave** on the right of the tab bar (or **View**). Wave draws traces in the editor pane (scroll to zoom, drag to pan, click to place a cursor). **Run** opens the new dump as a waveform.

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
src/vhdl_sim.rs   yavs subprocess runner (FST)
src/waveform.rs   Surfer/wellen waveform pane
vendor/xezim      Vendored [xezim](https://github.com/aionhw/xezim) (no git fetch)
vendor/xezim-core Vendored xezim-core (xezim path/patch dep)
vendor/yavs       Vendored [yavs](https://github.com/ru551n/yavs) (compiled by build.rs)
vendor/surfer     Vendored [Surfer](https://gitlab.com/surfer-project/surfer) v0.7.0
vendor/vcd-anon   Vendored [vcd-anon](https://gitlab.com/surfer-project/vcd-anon)
vendor/wave2saif  Vendored [wave2saif](https://gitlab.com/surfer-project/wave2saif)
samples/half_adder       Verilog half adder and testbench
samples/half_adder_vhdl  VHDL half adder and testbench
assets/           Application icon
scripts/host-prep.sh  Git, disk, and Nix preflight used by run.sh
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
