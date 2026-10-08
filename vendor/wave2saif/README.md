# wave2saif

wave2saif is a small program to convert VCD, FST, and GHW wave form files to
the Switching Activity Interchange Format (SAIF) used for power analysis.
Note that this will only work on post-synthesis netlists and that for proper switching activity estimation [SDF](https://en.wikipedia.org/wiki/Standard_Delay_Format) should be used during the simulation.

FST and GHW are much more compact file formats compared to VCD, and OpenSTA only supports VCD and SAIF. Hence, it can be beneficial to store the simulation traces as FST and convert to SAIF rather than storing large VCD files.

| Simulator | VHDL | Verilog | VCD | FST | GHW | SDF |
| ----------| ---| -----| --- | --- | --- | --- |
| [GHDL](https://github.com/ghdl/ghdl) | Yes | No | Yes | Yes | Yes | Yes |
| [NVC](https://github.com/nickg/nvc) | Yes | UDPs | Yes | Yes | No | No |
| [Icarus Verilog](https://github.com/steveicarus/iverilog) | No | Yes | Yes | Yes | No | Yes |
| [Verilator](https://github.com/verilator/verilator/) | No | Yes | Yes | Yes | No | No |

## Installation

If you have a rust toolchain setup (at least version 1.81), simply run

```bash
cargo install cargo install --git https://gitlab.com/surfer-project/wave2saif.git wave2saif
```

### Pre-built binaries

The latest binary version of the main branch can be downloaded for

- [Linux - x86_64](https://gitlab.com/api/v4/projects/71368123/jobs/artifacts/main/raw/wave2saif_linux.zip?job=linux_build)
- [MacOS - aarch64](https://gitlab.com/api/v4/projects/71368123/jobs/artifacts/main/raw/wave2saif_macos-aarch64.zip?job=macos-aarch64_build)
- [Windows - x86_64](https://gitlab.com/api/v4/projects/71368123/jobs/artifacts/main/raw/wave2saif_win.zip?job=windows_build)

Open an issue if you think that more OS-architecture combinations are useful.

## Usage

```text
Usage: wave2saif.exe [OPTIONS] <INFILE>

Arguments:
  <INFILE>  Input file

Options:
  -o, --outfile <OUTFILE>  Output file
  -h, --help               Print help
  -V, --version            Print version
```

If `OUTFILE` is not provided, a file with the same name as `INFILE`, but
with a `.saif`-extension is written.

## Multi-threading

wave2saif and the underlying [wellen](https://github.com/ekiwi/wellen/)is multi-threaded. Using more threads will reduce the time it takes to perform the conversion, but at the expense of higher memory use. To control the number of threads, use the environment variable `RAYON_NUM_THREADS`. For example, to use four threads:

```bash
RAYON_NUM_THREADS=4 wave2saif ...
```

## Contributions

Contributions are welcome. Either as issues or merge requests.

## License

wave2saif is licensed under the [European Union Public License 1.2](https://interoperable-europe.ec.europa.eu/collection/eupl/eupl-text-eupl-12).
