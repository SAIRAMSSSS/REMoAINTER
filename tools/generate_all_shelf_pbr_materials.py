#!/usr/bin/env python3
"""
Generates complete 5-channel PBR texture sets (base_color, normal, roughness, metallic, ambient_occlusion)
for all authentic materials in assets/shelf/materials/ and assets/shelf/smart_materials/.
"""

import os
import json
import numpy as np
from PIL import Image

BASE_DIR = "/mnt/shared/REMoAINTER"
SHELF_MATS_DIR = os.path.join(BASE_DIR, "assets/shelf/materials")
SHELF_SM_DIR = os.path.join(BASE_DIR, "assets/shelf/smart_materials")
OUT_MATS_DIR = os.path.join(BASE_DIR, "assets/materials")
RES = 1024

os.makedirs(OUT_MATS_DIR, exist_ok=True)

def infer_material_props(name):
    lower = name.lower()
    # (is_metal, base_metal_val, base_rough_val, bump_strength)
    if any(m in lower for m in ["gold", "chrome"]):
        return (True, 0.98, 0.18, 0.04)
    elif any(m in lower for m in ["copper", "brass", "bronze"]):
        return (True, 0.92, 0.28, 0.06)
    elif any(m in lower for m in ["aluminium", "aluminum"]):
        return (True, 0.90, 0.25, 0.05)
    elif any(m in lower for m in ["steel", "iron", "metal", "foil", "welding"]):
        return (True, 0.90, 0.35, 0.08)
    elif any(m in lower for m in ["carbon"]):
        return (False, 0.15, 0.28, 0.12)
    elif any(m in lower for m in ["leather"]):
        return (False, 0.0, 0.52, 0.14)
    elif any(m in lower for m in ["fabric", "denim", "linen", "wool", "cotton", "lace", "nylon", "ripstop"]):
        return (False, 0.0, 0.72, 0.16)
    elif any(m in lower for m in ["wood", "bark"]):
        return (False, 0.0, 0.48, 0.15)
    elif any(m in lower for m in ["concrete", "asphalt", "stone", "rock", "slate", "travertine", "mud"]):
        return (False, 0.0, 0.82, 0.22)
    elif any(m in lower for m in ["plastic", "rubber"]):
        return (False, 0.0, 0.32, 0.05)
    elif any(m in lower for m in ["clay", "terracotta", "earthenware", "cork"]):
        return (False, 0.0, 0.78, 0.18)
    else:
        return (False, 0.0, 0.50, 0.10)

def sobel_normal_map(height, bump_scale=4.0):
    """Computes tangent-space normal map using Sobel operator (OpenGL Y+ up convention)."""
    h_pad = np.pad(height, 1, mode='edge')
    # Sobel kernels
    dx = (h_pad[:-2, 2:] + 2.0 * h_pad[1:-1, 2:] + h_pad[2:, 2:]) - \
         (h_pad[:-2, :-2] + 2.0 * h_pad[1:-1, :-2] + h_pad[2:, :-2])
    dx *= 0.125 * bump_scale

    dy = (h_pad[2:, :-2] + 2.0 * h_pad[2:, 1:-1] + h_pad[2:, 2:]) - \
         (h_pad[:-2, :-2] + 2.0 * h_pad[:-2, 1:-1] + h_pad[:-2, 2:])
    dy *= 0.125 * bump_scale

    nx = -dx
    ny = dy  # OpenGL +Y is up
    nz = np.ones_like(nx)
    len_n = np.sqrt(nx * nx + ny * ny + nz * nz) + 1e-7
    nx /= len_n
    ny /= len_n
    nz /= len_n

    rgb = np.stack([
        (nx * 0.5 + 0.5) * 255.0,
        (ny * 0.5 + 0.5) * 255.0,
        (nz * 0.5 + 0.5) * 255.0
    ], axis=-1)
    return np.clip(rgb, 0, 255).astype(np.uint8)

def process_image_to_pbr(img_path, mat_name, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    
    # Load and resize to 1024x1024
    img = Image.open(img_path).convert("RGB")
    img_res = img.resize((RES, RES), Image.Resampling.LANCZOS)
    rgb_arr = np.array(img_res, dtype=np.float32)

    is_metal, base_metal, base_rough, bump_scale = infer_material_props(mat_name)

    # 1. Base Color
    base_color_img = Image.fromarray(np.clip(rgb_arr, 0, 255).astype(np.uint8))
    base_color_img.save(os.path.join(out_dir, "base_color.png"))

    # 2. Normalized Height / Displacement from luminance
    lum = (rgb_arr[:, :, 0] * 0.299 + rgb_arr[:, :, 1] * 0.587 + rgb_arr[:, :, 2] * 0.114) / 255.0
    # Contrast stretch luminance
    lum_centered = lum - np.mean(lum)
    height = lum_centered * 1.5

    # 3. Normal Map
    normal_arr = sobel_normal_map(height, bump_scale=bump_scale * 12.0)
    normal_img = Image.fromarray(normal_arr)
    normal_img.save(os.path.join(out_dir, "normal.png"))

    # 4. Roughness Map (Micro-surface roughness variations)
    rough_variation = (lum_centered * 0.35)
    rough_arr = np.clip(base_rough + rough_variation, 0.04, 0.98)
    rough_img = Image.fromarray((rough_arr * 255.0).astype(np.uint8))
    rough_img.save(os.path.join(out_dir, "roughness.png"))

    # 5. Metallic Map
    if is_metal:
        met_arr = np.clip(base_metal + lum_centered * 0.15, 0.70, 1.0)
    else:
        met_arr = np.full((RES, RES), base_metal, dtype=np.float32)
    met_img = Image.fromarray((met_arr * 255.0).astype(np.uint8))
    met_img.save(os.path.join(out_dir, "metallic.png"))

    # 6. Ambient Occlusion Map
    ao_arr = np.clip(1.0 - (np.maximum(-height, 0.0) * 1.8), 0.35, 1.0)
    ao_img = Image.fromarray((ao_arr * 255.0).astype(np.uint8))
    ao_img.save(os.path.join(out_dir, "ambient_occlusion.png"))

    # 7. Material manifest
    manifest = {
        "name": mat_name,
        "resolution": RES,
        "is_metal": is_metal,
        "channels": {
            "base_color": "base_color.png",
            "normal": "normal.png",
            "roughness": "roughness.png",
            "metallic": "metallic.png",
            "ambient_occlusion": "ambient_occlusion.png"
        }
    }
    with open(os.path.join(out_dir, "material.json"), "w") as f:
        json.dump(manifest, f, indent=2)

def main():
    count = 0
    # 1. Process all shelf materials
    if os.path.exists(SHELF_MATS_DIR):
        for f in os.listdir(SHELF_MATS_DIR):
            if f.lower().endswith(('.png', '.webp', '.jpg', '.jpeg')):
                stem = os.path.splitext(f)[0]
                img_p = os.path.join(SHELF_MATS_DIR, f)
                out_p = os.path.join(OUT_MATS_DIR, stem)
                process_image_to_pbr(img_p, stem, out_p)
                count += 1
                print(f"[{count:02d}] Generated PBR set for Material: {stem}")

    # 2. Process all shelf smart materials
    if os.path.exists(SHELF_SM_DIR):
        for f in os.listdir(SHELF_SM_DIR):
            if f.lower().endswith(('.png', '.webp', '.jpg', '.jpeg')):
                stem = os.path.splitext(f)[0]
                img_p = os.path.join(SHELF_SM_DIR, f)
                out_p = os.path.join(OUT_MATS_DIR, stem)
                # Only generate if not already generated with real textures
                if not os.path.exists(os.path.join(out_p, "normal.png")):
                    process_image_to_pbr(img_p, stem, out_p)
                    count += 1
                    print(f"[{count:02d}] Generated PBR set for Smart Material: {stem}")

    print(f"\n[SUCCESS] Successfully generated {count} complete PBR texture sets in {OUT_MATS_DIR}")

if __name__ == "__main__":
    main()
