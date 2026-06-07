import argparse
import colorsys
import tempfile
from pathlib import Path

from PIL import Image

from remove_connected_background import (
    alpha_bbox,
    parse_hex_color,
    remove_background,
    save_preview,
)


def build_red_alpha(image, min_red, min_saturation, low_redness, high_redness, hue_tolerance):
    rgba = image.convert("RGBA")
    w, h = rgba.size
    src = rgba.load()
    alpha = bytearray(w * h)

    for y in range(h):
        row = y * w
        for x in range(w):
            r, g, b, a = src[x, y]
            if a == 0:
                continue

            max_c = max(r, g, b)
            min_c = min(r, g, b)
            saturation = max_c - min_c
            if r < min_red or saturation < min_saturation:
                continue

            hue, _, _ = colorsys.rgb_to_hsv(r / 255.0, g / 255.0, b / 255.0)
            hue_deg = hue * 360.0
            hue_distance = min(abs(hue_deg), abs(360.0 - hue_deg))
            if hue_distance > hue_tolerance:
                continue

            redness = r - ((g + b) / 2.0)
            if redness <= low_redness:
                continue

            scaled = int(
                max(0.0, min(255.0, (redness - low_redness) * 255.0 / max(1.0, high_redness - low_redness)))
            )
            alpha[row + x] = min(a, scaled)

    return rgba, Image.frombytes("L", rgba.size, bytes(alpha))


def extract_red_stamp(
    input_path,
    output_path,
    tolerance,
    sample_step,
    clusters,
    soften,
    min_red,
    min_saturation,
    low_redness,
    high_redness,
    hue_tolerance,
    trim,
    trim_padding,
    preview_path,
    preview_bg,
):
    with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tmp:
        tmp_path = Path(tmp.name)

    try:
        remove_background(
            input_path=input_path,
            output_path=tmp_path,
            tolerance=tolerance,
            sample_step=sample_step,
            clusters=clusters,
            soften=soften,
            trim=False,
            trim_padding=trim_padding,
            preview_path=None,
            preview_bg=preview_bg,
        )

        base = Image.open(tmp_path).convert("RGBA")
        color_image, alpha_mask = build_red_alpha(
            base,
            min_red=min_red,
            min_saturation=min_saturation,
            low_redness=low_redness,
            high_redness=high_redness,
            hue_tolerance=hue_tolerance,
        )
        color_image.putalpha(alpha_mask)

        if trim:
            bbox = alpha_bbox(alpha_mask.tobytes(), color_image.size[0], color_image.size[1], trim_padding)
            if bbox:
                color_image = color_image.crop(bbox)

        output_path.parent.mkdir(parents=True, exist_ok=True)
        color_image.save(output_path)
        if preview_path:
            save_preview(color_image, preview_path, preview_bg)

        non_transparent = sum(1 for value in alpha_mask.tobytes() if value)
        print(f"input={input_path}")
        print(f"output={output_path}")
        print(f"size={base.size[0]}x{base.size[1]}")
        print(f"output_size={color_image.size[0]}x{color_image.size[1]}")
        print(f"non_transparent_pixels={non_transparent}")
        print("alpha_channel=RGBA")
        print(f"preview={preview_path}" if preview_path else "preview=")
    finally:
        tmp_path.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(
        description="Extract a red stamp from an image by reusing the connected-background cutout pass."
    )
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--tolerance", type=int, default=34)
    parser.add_argument("--sample-step", type=int, default=8)
    parser.add_argument("--clusters", type=int, default=3)
    parser.add_argument("--soften", type=int, default=1)
    parser.add_argument("--min-red", type=int, default=110)
    parser.add_argument("--min-saturation", type=int, default=14)
    parser.add_argument("--low-redness", type=float, default=14.0)
    parser.add_argument("--high-redness", type=float, default=72.0)
    parser.add_argument("--hue-tolerance", type=float, default=38.0)
    parser.add_argument("--trim", action="store_true")
    parser.add_argument("--trim-padding", type=int, default=8)
    parser.add_argument("--preview", type=Path)
    parser.add_argument(
        "--preview-bg",
        type=parse_hex_color,
        default=parse_hex_color("#202020"),
        help="Preview background color in #RRGGBB format.",
    )
    args = parser.parse_args()
    extract_red_stamp(
        input_path=args.input,
        output_path=args.output,
        tolerance=args.tolerance,
        sample_step=args.sample_step,
        clusters=args.clusters,
        soften=args.soften,
        min_red=args.min_red,
        min_saturation=args.min_saturation,
        low_redness=args.low_redness,
        high_redness=args.high_redness,
        hue_tolerance=args.hue_tolerance,
        trim=args.trim,
        trim_padding=args.trim_padding,
        preview_path=args.preview,
        preview_bg=args.preview_bg,
    )


if __name__ == "__main__":
    main()
