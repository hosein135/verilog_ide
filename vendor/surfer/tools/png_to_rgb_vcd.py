from __future__ import annotations

import argparse
from datetime import datetime, UTC
from pathlib import Path

from PIL import Image


def make_identifier(index: int) -> str:
    printable = [chr(code) for code in range(33, 127) if chr(code) not in {'$', '#'}]
    base = len(printable)
    digits: list[str] = []
    value = index

    while True:
        digits.append(printable[value % base])
        value //= base
        if value == 0:
            return ''.join(reversed(digits))


def rgb24_word(red: int, green: int, blue: int) -> int:
    return (red << 16) | (green << 8) | blue


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Convert a PNG image into a row-major 24-bit RGB VCD memory dump."
    )
    parser.add_argument("input", type=Path, help="Source PNG image")
    parser.add_argument("output", type=Path, help="Target VCD file")
    parser.add_argument(
        "--scope",
        default="image_memory",
        help="Top-level VCD module scope name",
    )
    args = parser.parse_args()

    image = Image.open(args.input).convert("RGB")
    width, height = image.size
    pixels = list(image.getdata())

    args.output.parent.mkdir(parents=True, exist_ok=True)

    with args.output.open("w", encoding="ascii", newline="\n") as handle:
        timestamp = datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ")
        handle.write(f"$date {timestamp} $end\n")
        handle.write("$version png_to_rgb_vcd.py $end\n")
        handle.write("$timescale 1ns $end\n\n")
        handle.write(f"$scope module {args.scope} $end\n")
        handle.write(" $var integer 32 ! width [31:0] $end\n")
        handle.write(' $var integer 32 \" height [31:0] $end\n')

        for index in range(len(pixels)):
            identifier = make_identifier(index + 2)
            handle.write(f" $var wire 24 {identifier} mem[{index}][23:0] $end\n")

        handle.write("$upscope $end\n")
        handle.write("$enddefinitions $end\n\n")
        handle.write("#0\n")
        handle.write(f"b{width:032b} !\n")
        handle.write(f"b{height:032b} \"\n")

        for index, (red, green, blue) in enumerate(pixels):
            identifier = make_identifier(index + 2)
            handle.write(f"b{rgb24_word(red, green, blue):024b} {identifier}\n")


if __name__ == "__main__":
    main()
