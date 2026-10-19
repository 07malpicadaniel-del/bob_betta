#!/usr/bin/env python3
import sys
import re
import asyncio
import subprocess
import edge_tts

BOB_VOICES = {
    "bob_jorge": ("es-MX-JorgeNeural", "+10%", "-6Hz"),
    "bob_alvaro": ("es-ES-AlvaroNeural", "+8%", "-4Hz"),
    "bob_tomas": ("es-AR-TomasNeural", "+8%", "-4Hz"),
    "bob_christopher": ("en-US-ChristopherNeural", "+8%", "-5Hz"),
    "bob_ryan": ("en-GB-RyanNeural", "+6%", "-4Hz"),
    "bob_henri": ("fr-FR-HenriNeural", "+6%", "-3Hz"),
    "bob_conrad": ("de-DE-ConradNeural", "+6%", "-3Hz"),
    "bob_keita": ("ja-JP-KeitaNeural", "+6%", "-3Hz"),
}

LANG_DEFAULT_VOICE = {
    "es": "bob_jorge",
    "en": "bob_christopher",
    "fr": "bob_henri",
    "de": "bob_conrad",
    "ja": "bob_keita",
}

def clean_markdown_for_speech(text):
    text = re.sub(r"```[\s\S]*?```", " He generado el bloque de código en pantalla. ", text)
    text = re.sub(r"\[TOOL:[^\]]+\]", " ", text)
    text = re.sub(r"`([^`]+)`", r"\1", text)
    text = re.sub(r"[-#*_~>|\[\]\\]", " ", text)
    text = re.sub(r"[^\w\s,.:;¿?¡!()%/+áéíóúÁÉÍÓÚñÑüÜ'-]", " ", text)
    text = re.sub(r"\s+", " ", text).strip()
    return text[:1100]

async def synthesize_and_play(text, voice_or_lang):
    clean = clean_markdown_for_speech(text)
    if not clean:
        return

    key = voice_or_lang.strip()
    if key in BOB_VOICES:
        voice, rate, pitch = BOB_VOICES[key]
    else:
        mapped_key = LANG_DEFAULT_VOICE.get(key[:2].lower(), "bob_jorge")
        voice, rate, pitch = BOB_VOICES[mapped_key]

    out_mp3 = "/tmp/bob_tts_out.mp3"
    out_wav = "/tmp/bob_tts_out.wav"

    communicate = edge_tts.Communicate(clean, voice, rate=rate, pitch=pitch)
    await communicate.save(out_mp3)

    play_cmd = (
        f"(ffmpeg -y -loglevel quiet -i {out_mp3} -ar 24000 -ac 1 {out_wav} && pw-play {out_wav}) "
        f"|| ffplay -nodisp -autoexit -loglevel quiet {out_mp3} "
        f"|| mpv --no-video --really-quiet {out_mp3} "
        f"|| paplay {out_wav}"
    )
    subprocess.run(["bash", "-c", play_cmd], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

if __name__ == "__main__":
    txt = sys.argv[1] if len(sys.argv) > 1 else "Sistemas de voz de Bob operativos en Arch Linux."
    v_arg = sys.argv[2] if len(sys.argv) > 2 else "bob_jorge"
    asyncio.run(synthesize_and_play(txt, v_arg))
