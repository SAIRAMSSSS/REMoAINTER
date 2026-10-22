#!/usr/bin/env python3
"""Import Adobe Substance 3D Painter's starter-asset library into REMoAINTER's asset store.

Pipeline:
  - For every authentic .sbsar (materials / effects / generators / procedurals / alphas):
      7z-extract (sbsar == 7z archive), parse sbsdescription XML (label, category,
      keywords, graphtype, author, sbspresets), extract the authentic 512 icon as
      thumbnail.png, and for the *materials* group precompute fill_map.png (1024x1024
      RGB) by unprojecting the sphere-render icon onto the mesh UV layout.
  - Carve WebP thumbnails out of .spsm (HDF5) smart materials (binary carving only).
  - Copy fonts flat to assets/fonts/substance/ and flat alpha PNGs to
    assets/shelf/alphas/Substance/.

Write target: assets/substance_cache/ (plus those two copy dirs). Nothing else is
touched; no git operations. Idempotent (re-run overwrites).
"""

import json
import os
import shutil
import math
import struct
import subprocess
import tempfile
import time
import xml.etree.ElementTree as ET
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed
from io import BytesIO
from pathlib import Path

from PIL import Image

SRC = Path("/mnt/shared/SubstancePainter11/App/resources/starter_assets")
ROOT = Path("/mnt/shared/REMoAINTER")
DST = ROOT / "assets" / "substance_cache"
FONTS_DST = ROOT / "assets" / "fonts" / "substance"
ALPHAS_SHELF_DST = ROOT / "assets" / "shelf" / "alphas" / "Substance"

THREADS = 8
SBS_SIZE = 1024

# ---------------------------------------------------------------------------
# Mesh UV layout constants (copies of rust/src/graphics/mesh.rs — DO NOT CHANGE)
# ---------------------------------------------------------------------------
U0, U1 = 0.02, 0.70   # body rect, u range
V0, V1 = 0.02, 0.98   # body rect, v range
CAP_CX, CAP_CY, CAP_R = 0.855, 0.245, 0.095  # top cap circle (v larger edge = top cap side y small in image)
CAP_CX2, CAP_CY2 = 0.855, 0.755              # bottom cap circle
DELTA = 0.18849556           # body polar trim delta (rad, ~10.8 deg)
SPHERE_SAMPLE_MARGIN = 0.995


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def parse_value(raw: str):
    """Parse a presetinner value: comma-joined numbers -> float/list[float], else str."""
    raw = raw.strip()
    parts = [p.strip() for p in raw.split(",")]
    try:
        nums = [float(p) for p in parts]
    except ValueError:
        # booleans / multi-word values keep as string
        return raw
    if len(nums) == 1:
        return nums[0]
    return nums


def parse_xml_params(xml_path: Path):
    tree = ET.parse(str(xml_path))
    presets = []
    for preset in tree.iter("sbspreset"):
        inputs = []
        for pin in preset.findall("presetinput"):
            inputs.append(
                {
                    "identifier": pin.get("identifier", ""),
                    "type": int(pin.get("type", "0") or 0),
                    "value": parse_value(pin.get("value", "") or ""),
                    "uid": int(pin.get("uid", "0") or 0),
                }
            )
        presets.append({"label": preset.get("label", ""), "inputs": inputs})
    return presets


def detect_icon_kind(im_rgba: Image.Image):
    """Sphere render if circular alpha mask (or RGB-only center >> corner luminance)."""
    w, h = im_rgba.size
    n = min(w, h) // 20
    cx, cy = w // 2, h // 2
    px = im_rgba.load()

    def corner_a(x, y):
        r = 0
        for dx in range(n):
            for dy in range(n):
                r += px[x + dx, y + dy][3]
        return r / (n * n)

    def center_a():
        r = 0
        for dx in range(-n, n):
            for dy in range(-n, n):
                r += px[cx + dx, cy + dy][3]
        return r / (n * n * 4)

    center = center_a()
    corners = [corner_a(0, 0), corner_a(w - n, 0), corner_a(0, h - n), corner_a(w - n, h - n)]
    a_min = min(corners)
    a_max = max(corners)
    if center > 200 and a_min < 24 and a_max < 24:
        return "sphere"

    g = im_rgba.convert("L")
    gp = g.load()
    cl = sum(gp[cx + dx, cy + dy] for dx in range(-n, n) for dy in range(-n, n)) / (n * n * 4)
    cns = []
    for (x, y) in [(0, 0), (w - n, 0), (0, h - n), (w - n, h - n)]:
        s = 0
        for dx in range(n):
            for dy in range(n):
                s += gp[x + dx, y + dy]
        cns.append(s / (n * n))
    if cl > 40 and all(cl > 2.0 * (c + 8.0) for c in cns):
        return "sphere"
    return "flat"


def sphere_geometry(im_rgba: Image.Image):
    """Centroid + radius of the sphere disc from the alpha mask (or luminance fallback)."""
    w, h = im_rgba.size
    if im_rgba.mode in ("RGBA", "LA", "PA"):
        a = im_rgba.split()[-1]
        lo, hi = a.getextrema()
        if lo < hi:
            mask = a.point(lambda v: 255 if v > 64 else 0)
        else:
            mask = Image.new("L", im_rgba.size, 0)
    else:
        mask = Image.new("L", im_rgba.size, 0)
    if mask.getbbox() is None:
        lum = im_rgba.convert("L")
        lo, hi = lum.getextrema()
        if hi > lo + 8:
            mask = Image.new("L", im_rgba.size, 0)
            a_px = lum.load()
            m_px = mask.load()
            for y in range(h):
                for x in range(w):
                    if a_px[x, y] > max(24, int(lo * 0.35)):
                        m_px[x, y] = 255
    bbox = mask.getbbox()
    if bbox is None:
        return w / 2.0, h / 2.0, 0.23 * w
    xs0, ys0, xs1, ys1 = bbox
    cx = (xs0 + xs1) / 2.0
    cy = (ys0 + ys1) / 2.0
    rx = (xs1 - xs0) / 2.0
    ry = (ys1 - ys0) / 2.0
    return cx, cy, (rx + ry) / 2.0


def build_fill_map(icon_rgba: Image.Image) -> Image.Image:
    """Unproject the sphere icon onto the mesh UV layout -> 1024x1024 RGB."""
    W, H = icon_rgba.size
    icx, icy, R = sphere_geometry(icon_rgba)
    px = icon_rgba.convert("RGBA").load()
    out = Image.new("RGB", (SBS_SIZE, SBS_SIZE), (0, 0, 0))
    op = out.load()

    def sample_icon(sx: float, sy: float):
        # bilinear sample; sy positive => up on screen => smaller image y
        fx = icx + sx
        fy = icy - sy
        x0 = int(fx)
        y0 = int(fy)
        if x0 < 0 or y0 < 0 or x0 >= W - 1 or y0 >= H - 1:
            x0 = max(0, min(W - 2, x0))
            y0 = max(0, min(H - 2, y0))
        tx = fx - x0
        ty = fy - y0
        p00 = px[x0, y0]
        p10 = px[x0 + 1, y0]
        p01 = px[x0, y0 + 1]
        p11 = px[x0 + 1, y0 + 1]
        a00 = p00[3] / 255.0
        a10 = p10[3] / 255.0
        a01 = p01[3] / 255.0
        a11 = p11[3] / 255.0
        if a00 <= 0.0 and a10 <= 0.0 and a01 <= 0.0 and a11 <= 0.0:
            return (0.0, 0.0, 0.0)
        w00 = a00 * (1 - tx) * (1 - ty)
        w10 = a10 * tx * (1 - ty)
        w01 = a01 * (1 - tx) * ty
        w11 = a11 * tx * ty
        wsum = w00 + w10 + w01 + w11
        if wsum <= 1e-6:
            return (0.0, 0.0, 0.0)
        r = (w00 * p00[0] + w10 * p10[0] + w01 * p01[0] + w11 * p11[0]) / wsum
        g = (w00 * p00[1] + w10 * p10[1] + w01 * p01[1] + w11 * p11[1]) / wsum
        b = (w00 * p00[2] + w10 * p10[2] + w01 * p01[2] + w11 * p11[2]) / wsum
        return (r, g, b)

    # --- body island ---
    v_span = V1 - V0
    phi_span = math.pi - 2.0 * DELTA
    # per row: phi depends only on v; per column: theta depends only on u
    rows = list(range(int(V0 * SBS_SIZE), int(round(V1 * SBS_SIZE))))
    cols = list(range(int(U0 * SBS_SIZE), int(round(U1 * SBS_SIZE))))
    sin_theta = {c: math.sin(math.pi * (((c + 0.5) / SBS_SIZE - U0) / (U1 - U0) - 0.5)) for c in cols}
    r_lim_sq = (R * SPHERE_SAMPLE_MARGIN) ** 2
    for py in rows:
        v = (py + 0.5) / SBS_SIZE
        phi = DELTA + phi_span * (V1 - v) / v_span
        sin_phi = math.sin(phi)
        sy = math.cos(phi) * R  # +Y screen up: y_s = cos(phi)
        for c in cols:
            sx = sin_phi * sin_theta[c] * R
            d2 = sx * sx + sy * sy
            scale = 1.0
            if d2 > r_lim_sq:
                # just outside the silhouette: pull onto the rim so the body
                # island (incl. poles at theta extremes) is fully covered
                sy2_lim = min(sy * sy, r_lim_sq)
                sx = math.copysign(math.sqrt(max(0.0, r_lim_sq - sy2_lim)), sx)
                scale = 0.94  # fall back inward if the rim pixel is transparent
            r, g, b = sample_icon(sx, sy)
            while scale < 1.0 and (r, g, b) == (0.0, 0.0, 0.0) and scale > 0.5:
                scale -= 0.08
                r, g, b = sample_icon(sx * scale, sy * scale)
            op[c, py] = (int(r), int(g), int(b))

    # --- caps: orthographic polar view of the pole with sin() radial squeeze ---
    def fill_cap(ccx_uv, ccy_uv):
        top_cap = ccy_uv < 0.5  # top cap is the upper circle in the image (v smaller)
        cpx = ccx_uv * SBS_SIZE
        cpy = ccy_uv * SBS_SIZE
        rpx = CAP_R * SBS_SIZE
        for py in range(int(cpy - rpx) - 1, int(cpy + rpx) + 2):
            for px_x in range(int(cpx - rpx) - 1, int(cpx + rpx) + 2):
                dx = (px_x + 0.5 - cpx) / rpx
                dy = (py + 0.5 - cpy) / rpx
                rr = (dx * dx + dy * dy) ** 0.5
                if rr > 1.0:
                    continue
                theta_cap = math.atan2(dx, -dy)  # 0 at cap-circumference "up" direction
                # ortho polar view: cap center = pole, rim = sphere equator
                ang = 0.5 * math.pi * rr
                sx = math.sin(ang) * math.sin(theta_cap) * R
                sy_eff = math.cos(ang) * R
                sy = sy_eff if top_cap else -sy_eff
                r, g, b = sample_icon(sx, sy)
                op[px_x, py] = (int(r), int(g), int(b))

    fill_cap(CAP_CX, CAP_CY)
    fill_cap(CAP_CX2, CAP_CY2)
    return out


def unique_dir_name(group_dirs: dict, base: str) -> str:
    n = group_dirs.get(base, 0)
    group_dirs[base] = n + 1
    if n == 0:
        return base
    return f"{base}_{n}"


def process_sbsar(job):
    """Extract + parse one .sbsar -> item files. Returns (status, name, reason)."""
    group, src_path, sp_category, out_dir = job
    tmp = Path(tempfile.mkdtemp(prefix="sbsar_", dir="/tmp/opencode"))
    try:
        r = subprocess.run(
            ["7z", "x", "-y", f"-o{tmp}", str(src_path)],
            capture_output=True, text=True, timeout=120,
        )
        if r.returncode != 0:
            return ("error", src_path.name, f"7z rc={r.returncode}: {r.stderr.strip()[:120]}")
        xmls = [p for p in tmp.rglob("*.xml")]
        meta = {}
        xml_used = None
        for xp in xmls:
            try:
                tree = ET.parse(str(xp))
            except ET.ParseError:
                continue
            root = tree.getroot()
            if root.tag != "sbsdescription":
                continue
            graph = root.find("./graphs/graph")
            if graph is None:
                for g in root.iter("graph"):
                    graph = g
                    break
            if graph is None:
                continue
            xml_used = xp
            meta = {
                "label": graph.get("label") or src_path.stem,
                "graph_type": graph.get("graphtype") or "",
                "keywords": graph.get("keywords") or "",
                "sp_category": graph.get("category") or sp_category or "",
                "author": graph.get("author") or "",
                "icon_ref": graph.get("icon") or "",
            }
            break
        if xml_used is None:
            return ("error", src_path.name, "no sbsdescription xml in sbsar")

        icon_path = None
        if meta.get("icon_ref"):
            icon_path = next((p for p in tmp.rglob(meta["icon_ref"])), None)
        if icon_path is None:
            icon_path = next((p for p in tmp.rglob("*.png")), None)
        if icon_path is not None:
            try:
                icon = Image.open(icon_path).convert("RGBA")
            except Exception as e:
                icon = None
                return ("error", src_path.name, f"icon decode: {e}")
            icon_kind = detect_icon_kind(icon)
            icon.save(out_dir / "thumbnail.png")
        else:
            # No embedded icon (SP renders these previews inside the app itself).
            # Keep the item: meta + params are still authentic; placeholder thumb.
            icon_kind = "flat"
            placeholder = Image.new("RGBA", (256, 256), (52, 52, 56, 255))
            placeholder.save(out_dir / "thumbnail.png")

        params = parse_xml_params(xml_used)
        meta_doc = {
            "source_file": str(src_path.relative_to(SRC)),
            "label": meta.get("label", src_path.stem),
            "sp_category": meta.get("sp_category", sp_category or ""),
            "keywords": meta.get("keywords", ""),
            "graph_type": meta.get("graph_type", ""),
            "author": meta.get("author", ""),
            "icon_kind": icon_kind,
        }
        (out_dir / "meta.json").write_text(json.dumps(meta_doc, indent=2))
        (out_dir / "params.json").write_text(json.dumps({"presets": params}, indent=2))

        if group == "materials":
            icon = Image.open(out_dir / "thumbnail.png").convert("RGBA")
            build_fill_map(icon).save(out_dir / "fill_map.png")
        return ("ok", src_path.name, "")
    except Exception as e:  # tolerate & continue
        return ("error", src_path.name, f"{type(e).__name__}: {e}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def carve_webp(data: bytes, offset: int):
    """Given a 'RIFF' hit, return a complete WEBP blob or None."""
    if offset + 12 > len(data):
        return None
    if data[offset + 8: offset + 12] not in (b"WEBP",):
        # maybe RIFF payload starts later; search forward for 'WEBP' close by
        w = data.find(b"WEBP", offset, offset + 512)
        if w == -1:
            return None
        size_off = w - 4
        if size_off + 4 > len(data):
            return None
        size = struct.unpack("<I", data[size_off: size_off + 4])[0]
        blob = data[size_off: size_off + 8 + size]
    else:
        size = struct.unpack("<I", data[offset + 4: offset + 8])[0]
        blob = data[offset: offset + 8 + size]
    if not blob.startswith(b"RIFF"):
        return None
    try:
        im = Image.open(BytesIO(blob))
        im.load()
        return blob
    except Exception:
        return None


def process_spsm(job):
    """Carve biggest WebP blob out of an .spsm (HDF5) as thumbnail."""
    group, src_path, sp_category, out_dir = job
    try:
        data = src_path.read_bytes()
        best = None
        best_len = 0
        start = 0
        while True:
            i = data.find(b"RIFF", start)
            if i == -1:
                break
            blob = carve_webp(data, i)
            if blob is not None and len(blob) > best_len:
                best = blob
                best_len = len(blob)
                start = i + 8
            else:
                start = i + 4
        if best is None:
            return ("error", src_path.name, "no WebP blob found in spsm")
        im = Image.open(BytesIO(best)).convert("RGBA")
        kind = detect_icon_kind(im)
        im.save(out_dir / "thumbnail.png")
        meta_doc = {
            "source_file": str(src_path.relative_to(SRC)),
            "label": src_path.stem,
            "sp_category": sp_category,
            "keywords": "",
            "graph_type": "smart_material",
            "author": "",
            "icon_kind": kind,
        }
        (out_dir / "meta.json").write_text(json.dumps(meta_doc, indent=2))
        (out_dir / "params.json").write_text(json.dumps({"presets": []}, indent=2))
        return ("ok", src_path.name, "")
    except Exception as e:
        return ("error", src_path.name, f"{type(e).__name__}: {e}")


def collect_jobs():
    jobs = []
    seen_dirs = defaultdict(dict)
    groups = {
        "materials": SRC / "materials",
        "effects": SRC / "effects",
        "generators": SRC / "generators",
        "procedurals": SRC / "procedurals",
        "alphas": SRC / "alphas",
    }
    for group, root in groups.items():
        for src_path in sorted(root.rglob("*.sbsar")):
            rel = src_path.relative_to(root)
            if any(part.startswith(".") for part in rel.parts):
                continue  # skip .alg_meta and other hidden trees
            sp_category = rel.parent.as_posix() if rel.parent != Path(".") else ""
            base = src_path.stem
            name = unique_dir_name(seen_dirs[group], base)
            out_dir = DST / group / name
            out_dir.mkdir(parents=True, exist_ok=True)
            jobs.append((group, src_path, sp_category, out_dir))
    for src_path in sorted((SRC / "smart-materials").rglob("*.spsm")):
        rel = src_path.relative_to(SRC / "smart-materials")
        if any(part.startswith(".") for part in rel.parts):
            continue
        sp_category = rel.parent.as_posix() if rel.parent != Path(".") else ""
        name = unique_dir_name(seen_dirs["smart_materials"], src_path.stem)
        out_dir = DST / "smart_materials" / name
        out_dir.mkdir(parents=True, exist_ok=True)
        jobs.append(("smart_materials", src_path, sp_category, out_dir))
    return jobs


def copy_fonts_and_alpha_pngs():
    if FONTS_DST.exists():
        shutil.rmtree(FONTS_DST)
    FONTS_DST.mkdir(parents=True, exist_ok=True)
    counts = defaultdict(int)
    for f in sorted((SRC / "fonts").rglob("*")):
        if f.suffix.lower() in (".ttf", ".otf") and f.is_file():
            target = FONTS_DST / f.name
            if target.exists():
                # keep both if names collide across folders
                target = FONTS_DST / f"{f.stem}_{counts[f.stem] or 1}{f.suffix}"
            counts[f.stem] += 1
            shutil.copy2(f, target)
    if ALPHAS_SHELF_DST.exists():
        shutil.rmtree(ALPHAS_SHELF_DST)
    ALPHAS_SHELF_DST.mkdir(parents=True, exist_ok=True)
    n_alpha = 0
    for f in sorted((SRC / "alphas").rglob("*.png")):
        if any(p.startswith(".") for p in f.relative_to(SRC / "alphas").parts):
            continue
        shutil.copy2(f, ALPHAS_SHELF_DST / f.name)
        n_alpha += 1
    return sum(counts.values()) or len(list(FONTS_DST.glob("*"))), n_alpha


def main():
    os.makedirs("/tmp/opencode", exist_ok=True)
    jobs = collect_jobs()
    log(f"collected {len(jobs)} items; starting import with {THREADS} threads")

    counts = defaultdict(int)
    failures = []
    done = 0
    t0 = time.time()
    with ThreadPoolExecutor(max_workers=THREADS) as ex:
        futures = {ex.submit(process_spsm if job[0] == "smart_materials" else process_sbsar, job): job for job in jobs}
        for fut in as_completed(futures):
            status, name, reason = fut.result()
            counts[status] += 1
            if status == "error":
                failures.append((name, reason))
            done += 1
            if done % 100 == 0 or done == len(jobs):
                log(f"progress {done}/{len(jobs)} ok={counts['ok']} err={counts['error']} ({time.time()-t0:.1f}s)")

    fonts_n, alpha_png_n = copy_fonts_and_alpha_pngs()

    # summary
    log("=== SUMMARY ===")
    per_group = defaultdict(int)
    for g in ("materials", "smart_materials", "effects", "generators", "procedurals", "alphas"):
        d = DST / g
        per_group[g] = len(list(d.iterdir())) if d.exists() else 0
        log(f"  {g}: {per_group[g]} items")
    log(f"  fonts copied flat: {len(list(FONTS_DST.glob('*')))}")
    log(f"  flat alpha pngs to shelf: {alpha_png_n}")
    log(f"  failures: {len(failures)}")
    for name, reason in failures[:10]:
        log(f"    SKIP {name}: {reason}")
    if len(failures) > 10:
        log(f"    ... and {len(failures)-10} more")
    (DST / "_import_failures.txt").write_text(
        "\n".join(f"{n}\t{r}" for n, r in failures)
    )
    log(f"total wall time {time.time()-t0:.1f}s")


if __name__ == "__main__":
    main()
