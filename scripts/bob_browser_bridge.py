#!/usr/bin/env python3
import sys
import time
import json
import random
from playwright.sync_api import sync_playwright

CDP_URL = "http://127.0.0.1:9222"

def connect_browser(p):
    try:
        browser = p.chromium.connect_over_cdp(CDP_URL)
        context = browser.contexts[0] if browser.contexts else browser.new_context()
        return browser, context
    except Exception as e:
        print(f"[!] No se detectó navegador en {CDP_URL}. Presiona 'Abrir Navegador Puente' en AXIS UI. Detalle: {e}")
        sys.exit(1)

def ask_gemini_safely(prompt_text):
    """
    GUARDRAIL DE SEGURIDAD GOOGLE:
    - Solo interactúa con https://gemini.google.com/app
    - Prohibido navegar a myaccount.google.com o modificar ajustes de cuenta.
    - Tecleo con cadencia humana para no disparar alertas anti-bot.
    """
    with sync_playwright() as p:
        browser, context = connect_browser(p)
        gemini_page = None
        for page in context.pages:
            if "gemini.google.com" in page.url:
                gemini_page = page
                break

        if not gemini_page:
            gemini_page = context.new_page()
            gemini_page.goto("https://gemini.google.com/app", wait_until="domcontentloaded", timeout=30000)
            time.sleep(2.5)

        gemini_page.bring_to_front()

        # Localizar el área de entrada de Gemini sin tocar ningún botón de cuenta
        input_selectors = [
            "div.ql-editor[contenteditable='true']",
            "rich-textarea div[contenteditable='true']",
            "div[role='textbox']",
            "textarea"
        ]
        box = None
        for sel in input_selectors:
            loc = gemini_page.locator(sel).first
            if loc.count() > 0 and loc.is_visible():
                box = loc
                break

        if not box:
            print("[!] No se encontró la barra de texto en Gemini. Verifica que tu sesión esté abierta en el navegador.")
            return

        box.click()
        time.sleep(random.uniform(0.3, 0.7))
        # Inserción limpia con pequeño retardo humano
        box.fill(prompt_text)
        time.sleep(random.uniform(0.5, 1.0))
        gemini_page.keyboard.press("Enter")

        # Esperar a que Gemini termine de generar la respuesta (estabilización del DOM)
        time.sleep(4.0)
        last_text = ""
        stable_ticks = 0
        for _ in range(45):
            time.sleep(1.2)
            responses = gemini_page.locator("message-content, model-response, .model-response-text").all_inner_texts()
            current_text = responses[-1].strip() if responses else ""
            if current_text and current_text == last_text:
                stable_ticks += 1
                if stable_ticks >= 3:
                    break
            else:
                stable_ticks = 0
                last_text = current_text

        print("=== RESPUESTA DE GEMINI (TUTOR EXTERNO QUIRÓN) ===")
        print(last_text if last_text else "Respuesta enviada en pestaña de Gemini (revisa la ventana activa).")

def inspect_active_page():
    """Extrae las preguntas, botones, inputs y texto visible de la pestaña activa (ej. Quiz / Plataforma)"""
    with sync_playwright() as p:
        browser, context = connect_browser(p)
        pages = [pg for pg in context.pages if not pg.url.startswith("devtools://")]
        if not pages:
            print("No hay pestañas activas.")
            return
        page = pages[-1]
        Title = page.title()
        url = page.url

        # Nunca inspeccionar ni tocar páginas de configuración de cuenta de Google
        if "myaccount.google.com" in url or "accounts.google.com" in url:
            print("[ESCUDO GOOGLE]: Pestaña de cuenta de Google ignorada por seguridad.")
            return

        js_extract = """() => {
            const bodyText = document.body ? document.body.innerText.slice(0, 3500) : '';
            const clickables = Array.from(document.querySelectorAll('button, input[type="radio"], input[type="checkbox"], label, a.btn, [role="radio"], [role="button"]'))
                .slice(0, 40)
                .map((el, idx) => {
                    const txt = (el.innerText || el.value || el.getAttribute('aria-label') || '').trim().replace(/\\s+/g, ' ');
                    return { idx, tag: el.tagName, type: el.type || '', text: txt.slice(0, 100) };
                })
                .filter(x => x.text.length > 0 || x.type === 'radio' || x.type === 'checkbox');
            return { bodyText, clickables };
        }"""
        data = page.evaluate(js_extract)
        print(f"URL Activa: {url}\nTítulo: {Title}\n--- TEXTO EN PANTALLA ---\n{data['bodyText']}\n--- ELEMENTOS INTERACTIVOS ---\n{json.dumps(data['clickables'], ensure_ascii=False, indent=2)}")

def click_by_text(target_text):
    """Hace clic con cadencia humana en la opción o botón cuyo texto coincida (para responder Quizes)"""
    with sync_playwright() as p:
        browser, context = connect_browser(p)
        pages = [pg for pg in context.pages if not pg.url.startswith("devtools://")]
        if not pages:
            print("No hay pestañas activas.")
            return
        page = pages[-1]
        if "myaccount.google.com" in page.url or "accounts.google.com" in page.url:
            print("[ESCUDO GOOGLE]: Acción bloqueada en página de cuenta Google.")
            return

        time.sleep(random.uniform(0.4, 0.9))
        loc = page.get_by_text(target_text, exact=False).first
        if loc.count() > 0:
            loc.scroll_into_view_if_needed()
            time.sleep(0.3)
            loc.click()
            time.sleep(1.2)
            print(f"[✓] Clic ejecutado sobre opción/botón: '{target_text}'.")
        else:
            print(f"[!] No se encontró un elemento con el texto '{target_text}'.")

if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "inspect"
    arg = sys.argv[2] if len(sys.argv) > 2 else ""
    if mode == "ask_gemini":
        ask_gemini_safely(arg)
    elif mode == "inspect":
        inspect_active_page()
    elif mode == "click":
        click_by_text(arg)
