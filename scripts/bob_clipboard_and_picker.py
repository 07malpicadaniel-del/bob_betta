#!/usr/bin/env python3
import sys
import os
import time
import json
import base64
import shutil
import subprocess

def analyze_file_or_image(dest_path):
    ext = os.path.splitext(dest_path)[1].lower().lstrip(".")
    is_img = ext in ("png", "jpg", "jpeg", "webp", "bmp", "gif")
    fname = os.path.basename(dest_path)
    byte_len = os.path.getsize(dest_path)

    preview_data_url = None
    if is_img:
        try:
            from PIL import Image
            import io
            im = Image.open(dest_path)
            im.thumbnail((360, 360))
            buf = io.BytesIO()
            im.save(buf, format="PNG")
            b64_thumb = base64.b64encode(buf.getvalue()).decode("ascii")
            preview_data_url = f"data:image/png;base64,{b64_thumb}"
        except Exception:
            pass

        vision_script = os.path.expanduser("~/proyectos/bob_engine/scripts/bob_vision_ingest.py")
        out = subprocess.run([sys.executable, vision_script, dest_path], capture_output=True, text=True)
        summary = f"Imagen guardada en {dest_path}"
        for line in out.stdout.splitlines():
            if line.startswith("JSON_OUT:"):
                try:
                    data = json.loads(line[len("JSON_OUT:"):])
                    summary = data.get("summary", summary)
                except Exception:
                    pass
    else:
        try:
            raw_txt = open(dest_path, "r", encoding="utf-8", errors="replace").read()[:2500]
        except Exception:
            raw_txt = "(Archivo binario)"
        summary = (
            f"[ARCHIVO ADJUNTO EN WORKSPACE: `{fname}` ({byte_len} bytes)]\n"
            f"Ruta completa: `{dest_path}`\nContenido:\n```\n{raw_txt}\n```"
        )

    return {
        "ok": True,
        "filename": fname,
        "saved_path": dest_path,
        "is_image": is_img,
        "preview_data_url": preview_data_url,
        "summary": summary,
    }

def paste_from_wayland(attach_dir):
    os.makedirs(attach_dir, exist_ok=True)
    ts = int(time.time() * 1000)
    dest_png = os.path.join(attach_dir, f"paste_{ts}.png")

    # 1. Intentar extraer imagen desde Wayland (wl-paste) o XWayland (xclip)
    cmd = f"wl-paste --type image/png > '{dest_png}' 2>/dev/null || wl-paste --type image/jpeg > '{dest_png}' 2>/dev/null || xclip -selection clipboard -t image/png -o > '{dest_png}' 2>/dev/null"
    subprocess.run(["bash", "-c", cmd])

    if os.path.exists(dest_png) and os.path.getsize(dest_png) > 120:
        return {"ok": True, "items": [analyze_file_or_image(dest_png)]}

    if os.path.exists(dest_png):
        os.remove(dest_png)

    # 2. Verificar si lo que se copió en el portapapeles es una ruta de archivo o URI file://
    txt_out = subprocess.run(["bash", "-c", "wl-paste --no-newline 2>/dev/null || xclip -selection clipboard -o 2>/dev/null"], capture_output=True, text=True)
    clip_text = txt_out.stdout.strip()
    cleaned_path = clip_text.replace("file://", "").strip()
    if cleaned_path and os.path.isfile(cleaned_path):
        target = os.path.join(attach_dir, os.path.basename(cleaned_path))
        if os.path.abspath(cleaned_path) != os.path.abspath(target):
            shutil.copy2(cleaned_path, target)
        return {"ok": True, "items": [analyze_file_or_image(target)]}

    return {"ok": False, "error": "El portapapeles no contiene una imagen ni ruta de archivo."}

def pick_files_native(attach_dir):
    os.makedirs(attach_dir, exist_ok=True)
    selected_paths = []
    try:
        import tkinter as tk
        from tkinter import filedialog
        root = tk.Tk()
        root.withdraw()
        root.attributes("-topmost", True)
        paths = filedialog.askopenfilenames(
            title="Seleccionar Imágenes o Archivos para BOB",
            filetypes=[
                ("Imágenes y Código", "*.png *.jpg *.jpeg *.webp *.bmp *.rs *.py *.kt *.tsx *.ts *.js *.json *.toml *.md *.txt *.xml *.gradle *.kts *.sh"),
                ("Todos los archivos", "*.*")
            ]
        )
        selected_paths = list(paths)
        root.destroy()
    except Exception as e:
        return {"ok": False, "error": f"No se pudo abrir ventana de selección: {e}"}

    if not selected_paths:
        return {"ok": False, "error": "Selección cancelada"}

    items = []
    for p in selected_paths:
        if os.path.isfile(p):
            dest = os.path.join(attach_dir, os.path.basename(p))
            if os.path.abspath(p) != os.path.abspath(dest):
                shutil.copy2(p, dest)
            items.append(analyze_file_or_image(dest))

    return {"ok": True, "items": items}

if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "paste"
    attach_dir = sys.argv[2] if len(sys.argv) > 2 else "/tmp"
    if mode == "paste":
        res = paste_from_wayland(attach_dir)
    else:
        res = pick_files_native(attach_dir)
    print("JSON_OUT:" + json.dumps(res, ensure_ascii=False))
