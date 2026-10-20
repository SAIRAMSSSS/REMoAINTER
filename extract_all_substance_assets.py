import os, sys, glob, struct, subprocess
from PIL import Image

SP_DIR = '/mnt/shared/SubstancePainter11/App/resources/starter_assets'
OUT_DIR = '/mnt/shared/REMoAINTER/assets/shelf'

os.makedirs(f"{OUT_DIR}/smart_materials", exist_ok=True)
os.makedirs(f"{OUT_DIR}/smart_masks", exist_ok=True)
os.makedirs(f"{OUT_DIR}/materials", exist_ok=True)
os.makedirs(f"{OUT_DIR}/thumbnails/smart_materials", exist_ok=True)
os.makedirs(f"{OUT_DIR}/thumbnails/smart_masks", exist_ok=True)
os.makedirs(f"{OUT_DIR}/thumbnails/materials", exist_ok=True)

def extract_webp(filepath):
    try:
        with open(filepath, 'rb') as f:
            data = f.read()
        idx = data.find(b'RIFF')
        while idx != -1:
            if idx + 12 <= len(data) and data[idx+8:idx+12] == b'WEBP':
                size = struct.unpack('<I', data[idx+4:idx+8])[0] + 8
                return data[idx:idx+size]
            idx = data.find(b'RIFF', idx + 1)
    except Exception as e:
        print(f"Error reading {filepath}: {e}")
    return None

# 1. Extract Smart Materials
spsm_count = 0
for root, dirs, files in os.walk(f"{SP_DIR}/smart-materials"):
    for f in files:
        if f.endswith('.spsm'):
            src = os.path.join(root, f)
            name = os.path.splitext(f)[0]
            webp_data = extract_webp(src)
            if webp_data:
                webp_out = f"{OUT_DIR}/smart_materials/{name}.webp"
                png_thumb = f"{OUT_DIR}/thumbnails/smart_materials/{name}.png"
                with open(webp_out, 'wb') as wf:
                    wf.write(webp_data)
                try:
                    im = Image.open(webp_out)
                    im.thumbnail((128, 128), Image.Resampling.LANCZOS)
                    im.save(png_thumb, 'PNG')
                    spsm_count += 1
                except Exception as e:
                    print(f"Thumbnail error {name}: {e}")

print(f"Extracted {spsm_count} Smart Materials")

# 2. Extract Smart Masks
spmsk_count = 0
for root, dirs, files in os.walk(f"{SP_DIR}/smart-masks"):
    for f in files:
        if f.endswith('.spmsk'):
            src = os.path.join(root, f)
            name = os.path.splitext(f)[0]
            webp_data = extract_webp(src)
            if webp_data:
                webp_out = f"{OUT_DIR}/smart_masks/{name}.webp"
                png_thumb = f"{OUT_DIR}/thumbnails/smart_masks/{name}.png"
                with open(webp_out, 'wb') as wf:
                    wf.write(webp_data)
                try:
                    im = Image.open(webp_out)
                    im.thumbnail((128, 128), Image.Resampling.LANCZOS)
                    im.save(png_thumb, 'PNG')
                    spmsk_count += 1
                except Exception as e:
                    print(f"Thumbnail error {name}: {e}")

print(f"Extracted {spmsk_count} Smart Masks")

# 3. Extract SBSAR Materials
sbsar_count = 0
for root, dirs, files in os.walk(f"{SP_DIR}/materials"):
    for f in files:
        if f.endswith('.sbsar') and not f.startswith('.'):
            src = os.path.join(root, f)
            name = os.path.splitext(f)[0].replace('_', ' ').title()
            # 7z list
            res = subprocess.run(['7z', 'l', src], capture_output=True, text=True)
            icon_file = None
            for line in res.stdout.splitlines():
                if 'icon' in line.lower() and line.strip().endswith('.png'):
                    icon_file = line.split()[-1]
                    break
            if icon_file:
                tmp_dir = f"/tmp/sbsar_{sbsar_count}"
                os.makedirs(tmp_dir, exist_ok=True)
                subprocess.run(['7z', 'e', '-y', f'-o{tmp_dir}', src, icon_file], capture_output=True)
                extracted = os.path.join(tmp_dir, os.path.basename(icon_file))
                if os.path.exists(extracted):
                    png_out = f"{OUT_DIR}/materials/{name}.png"
                    png_thumb = f"{OUT_DIR}/thumbnails/materials/{name}.png"
                    try:
                        im = Image.open(extracted)
                        im.save(png_out, 'PNG')
                        im.thumbnail((128, 128), Image.Resampling.LANCZOS)
                        im.save(png_thumb, 'PNG')
                        sbsar_count += 1
                    except Exception as e:
                        pass
                import shutil
                shutil.rmtree(tmp_dir, ignore_errors=True)

print(f"Extracted {sbsar_count} SBSAR Materials")
