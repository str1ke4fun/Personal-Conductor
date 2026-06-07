import argparse
import tempfile
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

from remove_connected_background import (
    alpha_bbox,
    parse_hex_color,
    remove_background,
    save_preview,
)


def clamp01(value):
    return np.clip(value, 0.0, 1.0)


def build_red_score(image):
    rgb = np.array(image.convert("RGB"), dtype=np.uint8)
    bgr = cv2.cvtColor(rgb, cv2.COLOR_RGB2BGR)
    lab = cv2.cvtColor(bgr, cv2.COLOR_BGR2LAB).astype(np.float32)
    hsv = cv2.cvtColor(bgr, cv2.COLOR_BGR2HSV).astype(np.float32)

    r = rgb[:, :, 0].astype(np.float32)
    g = rgb[:, :, 1].astype(np.float32)
    b = rgb[:, :, 2].astype(np.float32)

    a_pos = lab[:, :, 1] - 128.0
    red_excess = r - np.maximum(g, b)
    warmth = r - ((g + b) * 0.5)
    saturation = hsv[:, :, 1]
    value = hsv[:, :, 2]

    a_score = clamp01((a_pos - 1.0) / 26.0)
    red_score = clamp01((red_excess - 2.0) / 42.0)
    warm_score = clamp01((warmth - 4.0) / 54.0)
    sat_score = clamp01((saturation - 4.0) / 78.0)
    bright_score = clamp01((value - 42.0) / 150.0)

    score = (
        (0.55 * a_score)
        + (0.40 * warm_score)
        + (0.28 * red_score)
        + (0.18 * sat_score)
    )
    score *= 0.55 + (0.45 * bright_score)
    score[value < 28.0] = 0.0

    local = cv2.GaussianBlur(score, (0, 0), sigmaX=0.7, sigmaY=0.7)
    score = np.maximum(score, local * 0.92)
    return clamp01(score)


def build_black_line_score(image):
    rgb = np.array(image.convert("RGB"), dtype=np.uint8)
    bgr = cv2.cvtColor(rgb, cv2.COLOR_RGB2BGR)
    lab = cv2.cvtColor(bgr, cv2.COLOR_BGR2LAB).astype(np.float32)
    hsv = cv2.cvtColor(bgr, cv2.COLOR_BGR2HSV).astype(np.float32)

    r = rgb[:, :, 0].astype(np.float32)
    g = rgb[:, :, 1].astype(np.float32)
    b = rgb[:, :, 2].astype(np.float32)

    a_pos = lab[:, :, 1] - 128.0
    warmth = r - ((g + b) * 0.5)
    saturation = hsv[:, :, 1]
    value = hsv[:, :, 2]

    dark_score = clamp01((92.0 - value) / 78.0)
    unsat_score = clamp01((72.0 - saturation) / 72.0)
    not_red_score = 1.0 - clamp01((np.maximum(a_pos, warmth) - 4.0) / 18.0)

    score = dark_score * ((0.68 * unsat_score) + (0.32 * not_red_score))
    deep_black = dark_score * clamp01((36.0 - saturation) / 36.0)
    score = np.maximum(score, deep_black * 0.92)
    return clamp01(score)


def clean_alpha(alpha_u8):
    binary = np.where(alpha_u8 >= 22, 255, 0).astype(np.uint8)
    component_count, labels, stats, _ = cv2.connectedComponentsWithStats(binary, connectivity=8)
    filtered = np.zeros_like(alpha_u8)

    for component_id in range(1, component_count):
        area = stats[component_id, cv2.CC_STAT_AREA]
        if area < 4:
            continue
        filtered[labels == component_id] = alpha_u8[labels == component_id]

    blurred = cv2.GaussianBlur(filtered, (0, 0), sigmaX=0.6, sigmaY=0.6)
    filtered = np.maximum(filtered, (blurred * 0.92).astype(np.uint8))
    filtered = np.where(filtered >= 10, filtered, 0).astype(np.uint8)
    return filtered


def refine_alpha(red_score, black_score):
    base_alpha = (clamp01(red_score) ** 0.72) * 255.0
    black_penalty = clamp01((black_score - 0.18) / 0.70) ** 1.15
    alpha = base_alpha * (1.0 - (0.98 * black_penalty))

    strong_red = red_score >= 0.24
    weak_red = red_score >= 0.07
    safe_pixels = black_score <= 0.34
    support = cv2.dilate((strong_red.astype(np.uint8) * 255), np.ones((3, 3), dtype=np.uint8), iterations=1) > 0
    support_mask = support & weak_red & safe_pixels
    alpha = np.maximum(alpha, np.where(support_mask, np.minimum(255.0, base_alpha * 1.12), 0.0))

    suppressed = (black_score >= 0.48) & (red_score <= 0.18)
    alpha[suppressed] = 0.0
    return clean_alpha(alpha.astype(np.uint8))


def upscale_rgba(rgba, scale):
    rgb = rgba[:, :, :3].astype(np.float32)
    alpha = rgba[:, :, 3].astype(np.float32) / 255.0
    premultiplied = rgb * alpha[:, :, None]

    up_premultiplied = cv2.resize(
        premultiplied,
        None,
        fx=scale,
        fy=scale,
        interpolation=cv2.INTER_LANCZOS4,
    )
    up_alpha = cv2.resize(
        rgba[:, :, 3].astype(np.float32),
        None,
        fx=scale,
        fy=scale,
        interpolation=cv2.INTER_CUBIC,
    )

    up_premultiplied = cv2.addWeighted(
        up_premultiplied,
        1.35,
        cv2.GaussianBlur(up_premultiplied, (0, 0), sigmaX=0.9, sigmaY=0.9),
        -0.35,
        0,
    )
    up_alpha = cv2.addWeighted(
        up_alpha,
        1.25,
        cv2.GaussianBlur(up_alpha, (0, 0), sigmaX=0.8, sigmaY=0.8),
        -0.25,
        0,
    )
    up_alpha = clamp01(up_alpha / 255.0) ** 0.82
    up_alpha = (up_alpha * 255.0).astype(np.uint8)
    up_alpha = cv2.medianBlur(up_alpha, 3)

    alpha_float = np.maximum(up_alpha.astype(np.float32) / 255.0, 1e-5)
    restored = up_premultiplied / alpha_float[:, :, None]
    restored = np.clip(restored, 0.0, 255.0)

    dominance = clamp01(
        (restored[:, :, 0] - np.maximum(restored[:, :, 1], restored[:, :, 2])) / 255.0
    )
    restored[:, :, 0] = np.clip(restored[:, :, 0] * (1.03 + (0.12 * dominance)), 0.0, 255.0)
    restored[:, :, 1] = np.clip(restored[:, :, 1] * (0.97 - (0.08 * dominance)), 0.0, 255.0)
    restored[:, :, 2] = np.clip(restored[:, :, 2] * (0.97 - (0.05 * dominance)), 0.0, 255.0)

    return np.dstack((restored.astype(np.uint8), up_alpha))


def enhance_red_stamp(
    input_path,
    output_path,
    preview_path,
    preview_bg,
    scale,
    tolerance,
    sample_step,
    clusters,
    soften,
    trim,
    trim_padding,
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
        base_rgba = np.array(base, dtype=np.uint8)

        base_score = build_red_score(base)
        black_score = build_black_line_score(base)
        base_alpha = refine_alpha(base_score, black_score)

        base_rgba[:, :, 3] = np.minimum(base_rgba[:, :, 3], base_alpha)
        upscaled = upscale_rgba(base_rgba, scale)

        if trim:
            bbox = alpha_bbox(upscaled[:, :, 3].tobytes(), upscaled.shape[1], upscaled.shape[0], trim_padding * scale)
            if bbox:
                left, top, right, bottom = bbox
                upscaled = upscaled[top:bottom, left:right]

        output = Image.fromarray(upscaled, mode="RGBA")
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output.save(output_path)
        if preview_path:
            save_preview(output, preview_path, preview_bg)

        alpha_nonzero = int(np.count_nonzero(upscaled[:, :, 3]))
        print(f"input={input_path}")
        print(f"output={output_path}")
        print(f"output_size={output.size[0]}x{output.size[1]}")
        print(f"scale={scale}")
        print(f"non_transparent_pixels={alpha_nonzero}")
        print("alpha_channel=RGBA")
        print(f"preview={preview_path}" if preview_path else "preview=")
    finally:
        tmp_path.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(
        description="Rebuild and upscale a red stamp cutout from a scanned image."
    )
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--preview", type=Path)
    parser.add_argument(
        "--preview-bg",
        type=parse_hex_color,
        default=parse_hex_color("#202020"),
        help="Preview background color in #RRGGBB format.",
    )
    parser.add_argument("--scale", type=int, default=4)
    parser.add_argument("--tolerance", type=int, default=34)
    parser.add_argument("--sample-step", type=int, default=8)
    parser.add_argument("--clusters", type=int, default=3)
    parser.add_argument("--soften", type=int, default=1)
    parser.add_argument("--trim", action="store_true")
    parser.add_argument("--trim-padding", type=int, default=8)
    args = parser.parse_args()

    enhance_red_stamp(
        input_path=args.input,
        output_path=args.output,
        preview_path=args.preview,
        preview_bg=args.preview_bg,
        scale=args.scale,
        tolerance=args.tolerance,
        sample_step=args.sample_step,
        clusters=args.clusters,
        soften=args.soften,
        trim=args.trim,
        trim_padding=args.trim_padding,
    )


if __name__ == "__main__":
    main()
