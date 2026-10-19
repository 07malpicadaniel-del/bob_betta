#!/usr/bin/env python3
import sys
import os
import json
from PIL import Image

def analyze_image(img_path):
    if not os.path.exists(img_path):
        return {"ok": False, "error": "Archivo de imagen no encontrado"}

    try:
        im = Image.open(img_path)
        width, height = im.size
        fmt = im.format or "IMAGE"
        mode = im.mode

        # Muestreo de paleta dominante (escala de grises / color)
        thumb = im.convert("RGB").resize((16, 16))
        pixels = list(thumb.getdata())
        avg_r = sum(p[0] for p in pixels) // len(pixels)
        avg_g = sum(p[1] for p in pixels) // len(pixels)
        avg_b = sum(p[2] for p in pixels) // len(pixels)

        ocr_lines = []
        try:
            from rapidocr_onnxruntime import RapidOCR
            engine = RapidOCR()
            result, _ = engine(img_path)
            if result:
                for item in result:
                    txt = item[1].strip()
                    conf = round(float(item[2]) * 100, 1)
                    if txt and conf >= 55.0:
                        ocr_lines.append(txt)
        except Exception:
            pass

        extracted_text = "\n".join(ocr_lines[:90])
        summary = (
            f"[IMAGEN ADJUNTA ANALIZADA EN LOCAL: {os.path.basename(img_path)}]\n"
            f"- Ruta en Workspace: {img_path}\n"
            f"- Dimensiones: {width}x{height} px ({fmt}, {mode}) | RGB Promedio: ({avg_r}, {avg_g}, {avg_b})\n"
            f"- Texto / Código / UI detectado dentro de la imagen ({len(ocr_lines)} bloques):\n"
            f"{extracted_text if extracted_text else '(Imagen gráfica sin texto legible o diagrama visual puro)'}"
        )

        return {
            "ok": True,
            "kind": "image",
            "path": img_path,
            "width": width,
            "height": height,
            "ocr_blocks": len(ocr_lines),
            "summary": summary
        }
    except Exception as e:
        return {"ok": False, "error": str(e)}

if __name__ == "__main__":
    target = sys.argv[1] if len(sys.argv) > 1 else ""
    print("JSON_OUT:" + json.dumps(analyze_image(target), ensure_ascii=False))
