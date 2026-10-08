"""cocotb runner backend for xezim.

``cocotb_tools.runner.get_runner()`` only knows the simulators cocotb ships
support for, so a harness that calls it directly — NVIDIA's CVDP benchmark
does, in ``test_runner.py`` — cannot select xezim no matter how complete the
VPI surface is. This module supplies the missing :class:`Runner` subclass and
registers it under the name ``xezim``.

Usage::

    import xezim_runner            # registers the backend as a side effect
    from cocotb_tools.runner import get_runner

    runner = get_runner("xezim")
    runner.build(verilog_sources=["dut.sv"], hdl_toplevel="dut")
    runner.test(hdl_toplevel="dut", test_module="test_dut")

or, without touching the harness, set ``SIM=xezim`` and preload this module
via ``PYTHONSTARTUP``/``sitecustomize``/``python -m``.

xezim has no separate elaboration step — it reads sources at run time — so
``build()`` only records the file list and ``test()`` runs the simulator once.
"""

from __future__ import annotations

import os
import shutil
from pathlib import Path
from typing import List, Mapping, Sequence, Union

import cocotb_tools.config
from cocotb_tools.runner import Runner, Verilog

PathLike = Union[str, os.PathLike]
_Command = List[str]

#: Binary to invoke. Override with ``XEZIM_BIN`` for an out-of-PATH build.
XEZIM_BIN = os.environ.get("XEZIM_BIN", "xezim")

#: cocotb ships one shared object per simulator, but the VPI ones are not
#: simulator-specific — they are plain IEEE 1800 clause 38 clients. The ``vcs``
#: flavour is the plain-VPI build and is what xezim is verified against.
_VPI_LIB_FLAVOUR = os.environ.get("XEZIM_COCOTB_VPI_FLAVOUR", "vcs")


class Xezim(Runner):
    """:class:`Runner` implementation for xezim.

    .. admonition:: Simulator-specific usage

       * There is no separate compile step; ``build()`` writes a file list and
         ``test()`` runs the simulator on it.
       * ``parameters`` is not supported — xezim has no parameter-override
         switch, so passing any raises rather than silently elaborating with
         the defaults.
       * ``pre_cmd`` is not supported.
    """

    supported_gpi_interfaces = {"verilog": ["vpi"]}

    # -- discovery ---------------------------------------------------------

    def _simulator_in_path(self) -> None:
        if shutil.which(XEZIM_BIN) is None and not Path(XEZIM_BIN).is_file():
            raise SystemExit(
                f"ERROR: {XEZIM_BIN} executable not found! "
                "Put it on PATH or set XEZIM_BIN to its full path."
            )

    # -- option translation ------------------------------------------------

    def _get_include_options(self, includes: Sequence[PathLike]) -> _Command:
        return [f"+incdir+{include}" for include in includes]

    def _get_define_options(self, defines: Mapping[str, object]) -> _Command:
        opts: _Command = []
        for name, value in defines.items():
            opts += ["-D", f"{name}={value}"]
        return opts

    def _get_parameter_options(self, parameters: Mapping[str, object]) -> _Command:
        if parameters:
            raise NotImplementedError(
                "xezim has no parameter-override switch, so "
                f"{sorted(parameters)} cannot be applied. Failing here rather "
                "than elaborating with the declared defaults, which would run "
                "a different design than the test asked for."
            )
        return []

    # -- artefacts ---------------------------------------------------------

    @property
    def sources_file(self) -> Path:
        """File list written by :meth:`build`, consumed by :meth:`test`."""
        return self.build_dir / "xezim_sources.f"

    @property
    def _vpi_lib(self) -> Path:
        return Path(cocotb_tools.config.libs_dir) / cocotb_tools.config.lib_name(
            "vpi", _VPI_LIB_FLAVOUR
        )

    # -- build -------------------------------------------------------------

    def _build_command(self) -> List[_Command]:
        # Entries are `_ValueAndTag`: `.value` is the path, `.tag` the language.
        sources = list(self._sources) + list(self._verilog_sources)
        if not sources:
            raise ValueError("xezim needs at least one Verilog/SystemVerilog source")
        for source in sources:
            if source.tag is not Verilog:
                raise ValueError(
                    f"xezim only supports Verilog/SystemVerilog. "
                    f"{str(source.value)!r} cannot be compiled."
                )

        lines: List[str] = []
        lines += [f"+incdir+{inc}" for inc in self.includes]
        for name, value in self.defines.items():
            lines.append(f"+define+{name}={value}")
        lines += [str(Path(source.value).resolve()) for source in sources]

        self.build_dir.mkdir(parents=True, exist_ok=True)
        self.sources_file.write_text("\n".join(lines) + "\n")

        # Nothing to execute: xezim elaborates at run time, so the "build" is
        # the file list above. Returning no commands keeps `build()` a no-op
        # rather than inventing a compile that would only be thrown away.
        return []

    # -- test --------------------------------------------------------------

    def _test_command(self) -> List[_Command]:
        if self.pre_cmd is not None:
            raise RuntimeError("pre_cmd is not implemented for xezim.")

        if not self.sources_file.is_file():
            raise FileNotFoundError(
                f"{self.sources_file} is missing — call build() before test()."
            )

        # libcocotbvpi_*.so links against libcocotb.so beside it.
        libs_dir = str(cocotb_tools.config.libs_dir)
        ld = self.env.get("LD_LIBRARY_PATH", os.environ.get("LD_LIBRARY_PATH", ""))
        self.env["LD_LIBRARY_PATH"] = f"{libs_dir}:{ld}" if ld else libs_dir

        cmd: _Command = [
            XEZIM_BIN,
            "--sv2017",
            "--vpi-lib",
            str(self._vpi_lib),
            "-f",
            str(self.sources_file),
        ]
        if self.hdl_toplevel is not None:
            cmd += ["-s", self.sim_hdl_toplevel]
        if self.timescale is not None:
            cmd += ["--module-timescale", "{}/{}".format(*self.timescale)]
        if self.waves or self.gui:
            cmd += ["--fst", str(self.build_dir / f"{self.sim_hdl_toplevel}.fst")]
        cmd += [
            arg.value if hasattr(arg, "value") else arg for arg in self.test_args
        ]
        cmd += list(self.plusargs)
        return [cmd]


def register() -> None:
    """Teach ``cocotb_tools.runner.get_runner`` about ``"xezim"``.

    Wraps the stock ``get_runner`` rather than editing its table, so it keeps
    working for every simulator cocotb already supports and stays correct if
    that table changes.
    """
    import cocotb_tools.runner as _runner

    if getattr(_runner.get_runner, "_xezim_wrapped", False):
        return

    _stock = _runner.get_runner

    def get_runner(simulator_name: str):
        if simulator_name == "xezim":
            return Xezim()
        return _stock(simulator_name)

    get_runner._xezim_wrapped = True  # type: ignore[attr-defined]
    get_runner.__doc__ = _stock.__doc__
    _runner.get_runner = get_runner
    _runner.Xezim = Xezim  # type: ignore[attr-defined]


register()
