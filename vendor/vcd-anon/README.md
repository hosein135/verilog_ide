# VCD Anon

VCD Anon is a simple tool to anonymize a VCD file. It does this by:

* Flatten the hierarchy
* Renaming all variables to `var_XX`
* Removing all parameters (by default, configurable)
* Changing all types to a smaller subset, see below (by default, configurable)
* Replace simulation date with anonymization date
* Replace simulator with VCD Anon

One purpose is to be able to upload a problematic VCD file for reporting an issue of a waveform viewer/VCD reading library in a reasonably anonymous way.

As part of the conversion, a mapping file (default: `mapping.txt`) is created. Hence, you can check that and state "`var_79` is the variable causing the issue". (Do not submit the mapping file if you want to keep the hierarchy from the public.)

Note that this is at a very early stage and it may not always keep the same behavior of the VCD file (e.g., some information may be dropped that is required to trigger the bug). In that case, please file an issue here.

**In all cases: check the output file to confirm that you are OK with uploading the file.**

## Type replacement

Below are more information about which types are replaced and what the resulting type is. Note that his also contains variable types from non-standard VCD extensions. The types here are the types supported by [wellen](https://github.com/ekiwi/wellen).

### Types resulting in `wire`

* `wire`
* `reg`
* `supply0`
* `supply1`
* `tri`
* `triand`
* `trior`
* `trireg`
* `tri0`
* `tri1`
* `wand`
* `wor`
* `logic`
* `port`
* `sparray`
* `bit`
* `byte`

### Types resulting in  `integer`

* `integer`
* `time`
* `int`
* `shortint`
* `enum`

### Types resulting in `real`

* `real`
* `realtime`
* `shortreal`

### Types not converted

* `event`
* `string`
* `longint`

## Changelog

### Version 0.2.0 (2025-12-22)

* More types are converted
* **BREAKING** - argument `--regs-kept` is replaced with `--types-kept`.

## Installing

Assuming that you have `rustc` and `cargo` installed:

```bash
cargo install --locked --git https://gitlab.com/surfer-project/vcd-anon.git
```

(You may want to try to drop `--locked` if you run into problems installing it.)

## Running

The help reads:

```text
Anonymizes and flattens VCD hierarchy into a single top-level module

Usage: vcd-anon.exe [OPTIONS] <INPUT> <OUTPUT>

Arguments:
  <INPUT>   Input VCD file to anonymize
  <OUTPUT>  Output VCD file path

Options:
  -m, --mapping <MAPPING>  Mapping file path [default: mapping.txt]
  -p, --parameters-kept    Keep parameter variables in the output
  -t, --types-kept         Keep variable types as is (do not convert to smaller subset)
  -h, --help               Print help
```

## Bugs/Suggestions/Issues

Please report in this repo.

Merge requests and other contributions are highly welcome!

## License

VCD-Anon is released under the [EUPL-1.2 license](https://interoperable-europe.ec.europa.eu/collection/eupl/eupl-text-eupl-12).
