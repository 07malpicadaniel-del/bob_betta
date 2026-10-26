# Skill: Sinapsis Externa con Gemini via CDP Nativo en Rust
**Keywords:** gemini, preguntar, consultar, quiron, navegador, duda, quiz, quizes

## 1. Protocolo de Consulta Segura a Gemini
1. Verificar que el Navegador Puente este abierto en el puerto `9222` (`--remote-debugging-port=9222 --remote-allow-origins=*`).
2. Invocar `[TOOL:ASK_GEMINI query="..."]` enviando exclusivamente la formulacion tecnica sin exponer claves ni datos sensibles.
3. Para resolver quizes en el navegador, ejecutar `[TOOL:BROWSER_INSPECT]` para leer el DOM de la pestana activa y `[TOOL:BROWSER_CLICK text="..."]` para seleccionar la opcion correcta.
