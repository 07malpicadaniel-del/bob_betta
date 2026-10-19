#!/usr/bin/env python3
import sys
import os
import json
import numpy as np
from faster_whisper import WhisperModel

TECH_GLOSSARY = (
    "Daniel, BOB, AXIS, LEIKA, AURORA, Sertra, Molex, ObraVoice, NanoTechLab, "
    "OpenAI, Claude, Gemini, Qwen, Whisper, Rust, Cargo, Tauri, React, Python, "
    "Arch Linux, Hyprland, Wayland, Ryzen, NVIDIA RTX, VRAM, Hipocampo, Quirón, HNSW, SIMD."
)

def load_and_normalize_pcm(wav_path):
    if not os.path.exists(wav_path):
        return None
    raw_bytes = open(wav_path, "rb").read()
    if len(raw_bytes) <= 44:
        return None
    pcm_bytes = raw_bytes[44:]
    usable_len = (len(pcm_bytes) // 2) * 2
    if usable_len < 3200:
        return None

    samples = np.frombuffer(pcm_bytes[:usable_len], dtype=np.int16).astype(np.float32) / 32768.0
    peak = np.max(np.abs(samples))
    if peak < 0.002:
        return None
    return (samples / peak) * 0.90

def main():
    audio_path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/bob_voice_in.wav"
    target_lang = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] != "auto" else None

    audio_array = load_and_normalize_pcm(audio_path)
    if audio_array is None:
        print("JSON_OUT:" + json.dumps({"ok": False, "text": "", "error": "Audio vacío o micrófono en silencio"}))
        return

    model = WhisperModel("base", device="cpu", compute_type="int8", cpu_threads=12)
    segments, info = model.transcribe(
        audio_array,
        language=target_lang,
        initial_prompt=TECH_GLOSSARY,
        beam_size=5,
        word_timestamps=True,
        vad_filter=False
    )

    words_telemetry = []
    full_text_parts = []

    for seg in segments:
        if seg.text.strip():
            full_text_parts.append(seg.text.strip())
        if seg.words:
            for w in seg.words:
                clean_w = w.word.strip()
                if clean_w:
                    prob_pct = round(float(w.probability) * 100, 1)
                    words_telemetry.append({"word": clean_w, "prob": prob_pct})

    full_text = " ".join(full_text_parts).strip()
    if not full_text:
        print("JSON_OUT:" + json.dumps({"ok": False, "text": "", "error": "No se detectaron palabras"}))
        return

    avg_score = round(sum(w["prob"] for w in words_telemetry) / len(words_telemetry), 1) if words_telemetry else 92.0
    weak_words = [f"{w['word']} ({w['prob']}%)" for w in words_telemetry if w["prob"] < 82.0]
    breakdown_str = " | ".join([f"{w['word']}:{w['prob']}%" for w in words_telemetry])

    phonetic_report = (
        f"[TELEMETRÍA ACÚSTICA WHISPER]: Idioma: '{info.language}' | Claridad fonética: {avg_score}% | "
        f"Desglose: [{breakdown_str}] | Palabras a mejorar (<82%): {weak_words if weak_words else 'Ninguna'}."
    )

    print("JSON_OUT:" + json.dumps({
        "ok": True,
        "text": full_text,
        "language": info.language,
        "pronunciation_score": avg_score,
        "weak_words": weak_words,
        "phonetic_report": phonetic_report
    }, ensure_ascii=False))

if __name__ == "__main__":
    main()
