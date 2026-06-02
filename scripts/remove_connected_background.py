import argparse
from collections import deque
from pathlib import Path

from PIL import Image


def dist2(a, b):
    return sum((int(x) - int(y)) ** 2 for x, y in zip(a, b))


def cluster_colors(colors, threshold):
    clusters = []
    threshold2 = threshold * threshold
    for color in colors:
        for cluster in clusters:
            if dist2(color, cluster["mean"]) <= threshold2:
                cluster["count"] += 1
                n = cluster["count"]
                cluster["mean"] = tuple(
                    round((cluster["mean"][i] * (n - 1) + color[i]) / n)
                    for i in range(3)
                )
                break
        else:
            clusters.append({"mean": color, "count": 1})
    clusters.sort(key=lambda item: item["count"], reverse=True)
    return [cluster["mean"] for cluster in clusters]


def border_samples(img, step):
    w, h = img.size
    px = img.load()
    samples = []
    for x in range(0, w, step):
        samples.append(px[x, 0][:3])
        samples.append(px[x, h - 1][:3])
    for y in range(0, h, step):
        samples.append(px[0, y][:3])
        samples.append(px[w - 1, y][:3])
    return samples


def connected_background_mask(img, palette, tolerance):
    w, h = img.size
    px = img.load()
    tolerance2 = tolerance * tolerance

    def is_background(x, y):
        color = px[x, y][:3]
        return any(dist2(color, p) <= tolerance2 for p in palette)

    mask = bytearray(w * h)
    queue = deque()

    def push(x, y):
        idx = y * w + x
        if mask[idx]:
            return
        if not is_background(x, y):
            return
        mask[idx] = 1
        queue.append((x, y))

    for x in range(w):
        push(x, 0)
        push(x, h - 1)
    for y in range(h):
        push(0, y)
        push(w - 1, y)

    while queue:
        x, y = queue.popleft()
        if x > 0:
            push(x - 1, y)
        if x + 1 < w:
            push(x + 1, y)
        if y > 0:
            push(x, y - 1)
        if y + 1 < h:
            push(x, y + 1)

    return mask


def soften_alpha(alpha, w, h, radius):
    if radius <= 0:
        return alpha
    src = bytearray(alpha)
    out = bytearray(alpha)
    for y in range(h):
        for x in range(w):
            idx = y * w + x
            if src[idx] == 0:
                continue
            near_bg = False
            for dy in range(-radius, radius + 1):
                yy = y + dy
                if yy < 0 or yy >= h:
                    continue
                for dx in range(-radius, radius + 1):
                    xx = x + dx
                    if xx < 0 or xx >= w:
                        continue
                    if src[yy * w + xx] == 0:
                        near_bg = True
                        break
                if near_bg:
                    break
            if near_bg:
                out[idx] = min(out[idx], 210)
    return out


def parse_hex_color(value):
    raw = value.strip()
    if raw.startswith("#"):
        raw = raw[1:]
    if len(raw) != 6:
        raise argparse.ArgumentTypeError("color must be #RRGGBB")
    try:
        return tuple(int(raw[i : i + 2], 16) for i in (0, 2, 4))
    except ValueError as exc:
        raise argparse.ArgumentTypeError("color must be #RRGGBB") from exc


def alpha_bbox(alpha, w, h, padding):
    xs = []
    ys = []
    for y in range(h):
        row = y * w
        for x in range(w):
            if alpha[row + x]:
                xs.append(x)
                ys.append(y)
    if not xs:
        return None
    left = max(min(xs) - padding, 0)
    top = max(min(ys) - padding, 0)
    right = min(max(xs) + padding + 1, w)
    bottom = min(max(ys) + padding + 1, h)
    return (left, top, right, bottom)


def save_preview(image, preview_path, background_rgb):
    preview_path.parent.mkdir(parents=True, exist_ok=True)
    background = Image.new("RGBA", image.size, (*background_rgb, 255))
    preview = Image.alpha_composite(background, image).convert("RGB")
    preview.save(preview_path)


def remove_background(
    input_path,
    output_path,
    tolerance,
    sample_step,
    clusters,
    soften,
    trim,
    trim_padding,
    preview_path,
    preview_bg,
):
    img = Image.open(input_path).convert("RGBA")
    w, h = img.size
    samples = border_samples(img, sample_step)
    palette = cluster_colors(samples, threshold=12)[:clusters]
    mask = connected_background_mask(img, palette, tolerance)

    alpha = bytearray([0 if value else 255 for value in mask])
    alpha = soften_alpha(alpha, w, h, soften)

    out = img.copy()
    out.putalpha(Image.frombytes("L", img.size, bytes(alpha)))
    if trim:
        bbox = alpha_bbox(alpha, w, h, trim_padding)
        if bbox:
            out = out.crop(bbox)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    out.save(output_path)
    if preview_path:
        save_preview(out, preview_path, preview_bg)

    removed = sum(1 for value in mask if value)
    kept = w * h - removed
    print(f"input={input_path}")
    print(f"output={output_path}")
    print(f"size={w}x{h}")
    print(f"output_size={out.size[0]}x{out.size[1]}")
    print(f"palette={palette}")
    print(f"removed_pixels={removed}")
    print(f"kept_pixels={kept}")
    print("alpha_channel=RGBA")
    print(f"preview={preview_path}" if preview_path else "preview=")


def main():
    parser = argparse.ArgumentParser(
        description="Remove a border-connected solid/checkerboard background."
    )
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--tolerance", type=int, default=34)
    parser.add_argument("--sample-step", type=int, default=16)
    parser.add_argument("--clusters", type=int, default=4)
    parser.add_argument("--soften", type=int, default=0)
    parser.add_argument(
        "--trim",
        action="store_true",
        help="Crop the output to the non-transparent bounding box.",
    )
    parser.add_argument("--trim-padding", type=int, default=24)
    parser.add_argument(
        "--preview",
        type=Path,
        help="Also save a flattened preview on a solid background.",
    )
    parser.add_argument(
        "--preview-bg",
        type=parse_hex_color,
        default=parse_hex_color("#202020"),
        help="Preview background color in #RRGGBB format.",
    )
    args = parser.parse_args()
    remove_background(
        args.input,
        args.output,
        args.tolerance,
        args.sample_step,
        args.clusters,
        args.soften,
        args.trim,
        args.trim_padding,
        args.preview,
        args.preview_bg,
    )


if __name__ == "__main__":
    main()
