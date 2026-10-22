#!/usr/bin/env python3
"""
Generates authentic high-fidelity PBR texture sets (BaseColor, Normal, Roughness, Metallic, AO)
for core starter materials into assets/materials/ with material.json manifests.
"""

import os
import json
import numpy as np
from PIL import Image

OUT_BASE = "/mnt/shared/REMoAINTER/assets/materials"
RES = 1024

def make_normal(height):
    """Computes tangent-space normal map (Sobel filter) from height map."""
    h_pad = np.pad(height, 1, mode='edge')
    dx = (h_pad[1:-1, 2:] - h_pad[1:-1, :-2]) * 0.5
    dy = (h_pad[2:, 1:-1] - h_pad[:-2, 1:-1]) * 0.5
    scale = 6.0
    nx = -dx * scale
    ny = dy * scale # OpenGL convention (Y+ up)
    nz = np.ones_like(nx)
    norm = np.sqrt(nx*nx + ny*ny + nz*nz) + 1e-6
    nx /= norm
    ny /= norm
    nz /= norm
    rgb = np.stack([(nx * 0.5 + 0.5) * 255.0, (ny * 0.5 + 0.5) * 255.0, (nz * 0.5 + 0.5) * 255.0], axis=-1)
    return np.clip(rgb, 0, 255).astype(np.uint8)

def create_material(name, base_rgb, metal_val, rough_base, rough_noise_scale, bump_scale=0.05, pattern_type="metal"):
    mat_dir = os.path.join(OUT_BASE, name)
    os.makedirs(mat_dir, exist_ok=True)

    np.random.seed(hash(name) % (2**32))
    
    # Generate micro-surface height map
    x = np.linspace(0, 16 * np.pi, RES)
    y = np.linspace(0, 16 * np.pi, RES)
    xx, yy = np.meshgrid(x, y)

    if pattern_type == "carbon":
        # Woven carbon fiber twill pattern
        twill = np.sin(xx * 4.0 + yy * 4.0) * np.sin(xx * 4.0 - yy * 4.0)
        fine_noise = np.random.normal(0, 0.05, (RES, RES))
        height = (twill * 0.4 + fine_noise * 0.1).astype(np.float32)
        metallic = np.full((RES, RES), 0.85, dtype=np.float32)
        roughness = (0.28 + np.abs(twill) * 0.22).astype(np.float32)
        albedo = np.tile(np.array(base_rgb, dtype=np.float32) * (0.8 + 0.4 * (twill[:, :, None] + 1.0) * 0.5), (1, 1, 1))
    elif pattern_type == "scratched_paint":
        fine_noise = np.random.normal(0, 0.1, (RES, RES))
        height = fine_noise * bump_scale
        metallic = np.full((RES, RES), metal_val, dtype=np.float32)
        roughness = np.clip(rough_base + fine_noise * rough_noise_scale, 0.05, 0.95).astype(np.float32)
        albedo = np.tile(np.array(base_rgb, dtype=np.float32), (RES, RES, 1))
        # Edge scratches expose underlying metal
        mask = (np.random.random((RES, RES)) > 0.985).astype(np.float32)
        metallic = np.where(mask > 0.5, 0.95, metallic)
        roughness = np.where(mask > 0.5, 0.22, roughness)
        albedo = np.where(mask[:, :, None] > 0.5, [210, 215, 220], albedo)
    elif pattern_type == "leather":
        cells = np.sin(xx * 2.0) * np.cos(yy * 2.0) + np.sin(xx * 5.0) * np.sin(yy * 5.0) * 0.5
        height = cells * 0.12 + np.random.normal(0, 0.04, (RES, RES))
        metallic = np.full((RES, RES), 0.0, dtype=np.float32)
        roughness = np.clip(rough_base + cells * 0.18, 0.25, 0.85).astype(np.float32)
        albedo = np.tile(np.array(base_rgb, dtype=np.float32), (RES, RES, 1)) * (0.85 + 0.15 * (cells[:, :, None] + 1.0) * 0.5)
    elif pattern_type == "concrete":
        fine = np.random.normal(0, 0.15, (RES, RES))
        large = np.sin(xx * 0.5) * np.cos(yy * 0.5) * 0.2
        height = fine * 0.15 + large
        metallic = np.full((RES, RES), 0.0, dtype=np.float32)
        roughness = np.clip(0.75 + fine * 0.2, 0.5, 0.95).astype(np.float32)
        albedo = np.tile(np.array(base_rgb, dtype=np.float32), (RES, RES, 1)) * (0.8 + 0.2 * (fine[:, :, None] + 1.0) * 0.5)
    else: # Polished or brushed metal
        brushed = np.sin(yy * 12.0) * 0.08 + np.random.normal(0, 0.03, (RES, RES))
        height = brushed * bump_scale
        metallic = np.full((RES, RES), metal_val, dtype=np.float32)
        roughness = np.clip(rough_base + brushed * rough_noise_scale, 0.05, 0.95).astype(np.float32)
        albedo = np.tile(np.array(base_rgb, dtype=np.float32), (RES, RES, 1))

    normal_rgb = make_normal(height)
    ao_map = np.clip((1.0 - np.abs(height) * 1.5) * 255.0, 160, 255).astype(np.uint8)

    # Save images
    bc_path = os.path.join(mat_dir, "base_color.png")
    met_path = os.path.join(mat_dir, "metallic.png")
    rgh_path = os.path.join(mat_dir, "roughness.png")
    nrm_path = os.path.join(mat_dir, "normal.png")
    ao_path = os.path.join(mat_dir, "ambient_occlusion.png")

    Image.fromarray(np.clip(albedo, 0, 255).astype(np.uint8)).save(bc_path)
    Image.fromarray((metallic * 255.0).astype(np.uint8)).save(met_path)
    Image.fromarray((roughness * 255.0).astype(np.uint8)).save(rgh_path)
    Image.fromarray(normal_rgb).save(nrm_path)
    Image.fromarray(ao_map).save(ao_path)

    manifest = {
        "name": name,
        "resolution": RES,
        "channels": {
            "base_color": "base_color.png",
            "metallic": "metallic.png",
            "roughness": "roughness.png",
            "normal": "normal.png",
            "ambient_occlusion": "ambient_occlusion.png"
        }
    }
    with open(os.path.join(mat_dir, "material.json"), "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"[✓] Generated PBR Material: {name}")

def main():
    materials = [
        ("Steel Painted Scraped Green", [52, 102, 60], 0.04, 0.35, 0.15, 0.08, "scratched_paint"),
        ("Aluminium Anodized Red", [185, 32, 38], 0.92, 0.22, 0.08, 0.03, "metal"),
        ("Brass Antique", [205, 165, 85], 0.90, 0.32, 0.12, 0.05, "metal"),
        ("Copper Oxidized", [195, 115, 88], 0.88, 0.38, 0.16, 0.07, "metal"),
        ("Carbon Fiber", [28, 28, 30], 0.85, 0.26, 0.18, 0.12, "carbon"),
        ("Iron Rough", [85, 88, 92], 0.95, 0.55, 0.22, 0.10, "metal"),
        ("Gold Pure", [255, 218, 80], 0.98, 0.15, 0.06, 0.02, "metal"),
        ("Concrete Wall", [160, 162, 165], 0.0, 0.82, 0.20, 0.15, "concrete"),
        ("Leather Dark", [55, 38, 28], 0.0, 0.52, 0.24, 0.14, "leather"),
        ("Wood Walnut", [92, 62, 42], 0.0, 0.45, 0.18, 0.08, "leather"),
    ]

    for mat in materials:
        create_material(*mat)

if __name__ == "__main__":
    main()
