#!/usr/bin/env python3
"""
REMoAINTER SBSAR & Smart Material Exporter
Uses Adobe Substance Automation Toolkit (sbsrender) to batch-render .sbsar graphs into
2048x2048 PBR texture maps (BaseColor, Normal, Roughness, Metallic, Height, AO),
generates material.json manifests, and extracts .spsm smart materials.
"""

import os
import sys
import json
import shutil
import subprocess
import argparse
from pathlib import Path
from PIL import Image

DEFAULT_SBSAR_DIR = "/mnt/shared/SubstancePainter11/App/resources/starter_assets/materials"
DEFAULT_SPSM_DIR = "/mnt/shared/SubstancePainter11/App/resources/starter_assets/smart-materials"
DEFAULT_OUT_DIR = "/mnt/shared/REMoAINTER/assets/materials"
DEFAULT_SHELF_THUMBS = "/mnt/shared/REMoAINTER/assets/shelf/thumbnails/materials"

def find_sbsrender():
    """Finds sbsrender executable on PATH or common installation locations."""
    # Check PATH
    sbs = shutil.which("sbsrender")
    if sbs:
        return sbs
    # Check environment variable
    if "SBSRENDER_PATH" in os.environ and os.path.exists(os.environ["SBSRENDER_PATH"]):
        return os.environ["SBSRENDER_PATH"]
    # Check common Substance directories
    candidates = [
        "/opt/Allegorithmic/Substance_Automation_Toolkit/sbsrender",
        "/usr/local/bin/sbsrender",
        "/mnt/shared/SubstancePainter11/App/sbsrender.exe",
        "/mnt/shared/SubstancePainter11/App/sbsrender",
    ]
    for c in candidates:
        if os.path.exists(c):
            return c
    return None

def process_sbsar(sbsrender_bin, sbsar_path, out_dir, resolution=2048):
    name = sbsar_path.stem.replace("_", " ").title()
    mat_dir = out_dir / name
    mat_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = mat_dir / "material.json"

    if manifest_path.exists():
        print(f"[*] Skipping {name} (already rendered)")
        return

    print(f"[+] Rendering SBSAR: {name} at {resolution}x{resolution}...")
    cmd = [
        sbsrender_bin,
        "render",
        "--input", str(sbsar_path),
        "--output-path", str(mat_dir),
        "--output-format", "png",
        "--output-name", "{inputName}_{outputNodeName}",
        "--set-resolutions", f"{resolution},{resolution}"
    ]

    try:
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        if res.returncode != 0:
            print(f"[!] Error rendering {sbsar_path.name}: {res.stderr}")
            return
    except Exception as e:
        print(f"[!] Subprocess error rendering {sbsar_path.name}: {e}")
        return

    # Scan rendered PNG outputs and build manifest
    manifest = {
        "name": name,
        "resolution": resolution,
        "channels": {}
    }

    for png in mat_dir.glob("*.png"):
        fname_lower = png.name.lower()
        if "basecolor" in fname_lower or "base_color" in fname_lower or "diffuse" in fname_lower or "albedo" in fname_lower:
            manifest["channels"]["base_color"] = png.name
        elif "normal" in fname_lower:
            manifest["channels"]["normal"] = png.name
        elif "roughness" in fname_lower:
            manifest["channels"]["roughness"] = png.name
        elif "metallic" in fname_lower or "metalness" in fname_lower:
            manifest["channels"]["metallic"] = png.name
        elif "ambientocclusion" in fname_lower or "ambient_occlusion" in fname_lower or "_ao" in fname_lower:
            manifest["channels"]["ambient_occlusion"] = png.name
        elif "height" in fname_lower or "displacement" in fname_lower:
            manifest["channels"]["height"] = png.name

    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"[✓] Created {manifest_path}")

def process_spsm(spsm_path, out_dir):
    """Extracts bundled textures and layer manifests from .spsm smart material files."""
    name = spsm_path.stem.replace("_", " ").title()
    mat_dir = out_dir / name
    mat_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = mat_dir / "material.json"

    if manifest_path.exists():
        return

    # Extract 7z archive if present
    tmp_dir = mat_dir / "_extracted"
    tmp_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(["7z", "x", "-y", f"-o{tmp_dir}", str(spsm_path)], capture_output=True)

    manifest = {
        "name": name,
        "type": "smart_material",
        "channels": {}
    }

    # Search for any extracted texture images
    for img_path in tmp_dir.rglob("*.*"):
        if img_path.suffix.lower() in [".png", ".jpg", ".tga", ".exr", ".webp"]:
            dest = mat_dir / img_path.name
            shutil.copy2(img_path, dest)
            fname_lower = img_path.name.lower()
            if "color" in fname_lower or "albedo" in fname_lower:
                manifest["channels"]["base_color"] = img_path.name
            elif "norm" in fname_lower:
                manifest["channels"]["normal"] = img_path.name
            elif "rough" in fname_lower:
                manifest["channels"]["roughness"] = img_path.name
            elif "metal" in fname_lower:
                manifest["channels"]["metallic"] = img_path.name

    shutil.rmtree(tmp_dir, ignore_errors=True)

    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

def main():
    parser = argparse.ArgumentParser(description="REMoAINTER Substance SBSAR & SPSM Batch Exporter")
    parser.add_argument("--sbsar-dir", default=DEFAULT_SBSAR_DIR, help="Path to starter_assets/materials")
    parser.add_argument("--spsm-dir", default=DEFAULT_SPSM_DIR, help="Path to starter_assets/smart-materials")
    parser.add_argument("--out-dir", default=DEFAULT_OUT_DIR, help="Destination directory for materials")
    parser.add_argument("--resolution", type=int, default=2048, help="Output map resolution (default: 2048)")
    parser.add_argument("--sbsrender", default=None, help="Explicit path to sbsrender binary")
    args = parser.parse_args()

    out_path = Path(args.out_dir)
    out_path.mkdir(parents=True, exist_ok=True)

    sbsrender_bin = args.sbsrender or find_sbsrender()

    print("=" * 65)
    print(" REMoAINTER - Substance PBR Material & SBSAR Exporter")
    print("=" * 65)

    if not sbsrender_bin:
        print("\n[!] NOTICE: Adobe Substance Automation Toolkit (sbsrender) was not found.")
        print("    SBSAR files require `sbsrender` to execute the procedural graph rendering.")
        print("    To render SBSAR files to full 2048/4096 texture maps:")
        print("      1. Install Substance 3D Automation Toolkit (SAT)")
        print("      2. Ensure `sbsrender` is in your PATH or pass `--sbsrender /path/to/sbsrender`")
        print("      3. Or set `export SBSRENDER_PATH=/path/to/sbsrender`")
        print("\n[*] Processing .spsm smart materials and bundled assets instead...")
    else:
        print(f"[✓] Found sbsrender: {sbsrender_bin}")
        sbsar_dir = Path(args.sbsar_dir)
        if sbsar_dir.exists():
            sbsar_files = list(sbsar_dir.rglob("*.sbsar"))
            print(f"[*] Found {len(sbsar_files)} SBSAR files in {sbsar_dir}")
            for s in sbsar_files:
                process_sbsar(sbsrender_bin, s, out_path, args.resolution)

    # Process .spsm smart materials
    spsm_dir = Path(args.spsm_dir)
    if spsm_dir.exists():
        spsm_files = list(spsm_dir.rglob("*.spsm"))
        print(f"[*] Found {len(spsm_files)} Smart Materials in {spsm_dir}")
        for sm in spsm_files:
            process_spsm(sm, out_path)

    print("\n[✓] Export complete. Materials indexed in:", out_path)

if __name__ == "__main__":
    main()
