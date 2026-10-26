# Skill: Creacion, Busqueda Autonoma y Lanzamiento de Juegos/Apps en Arch Linux (Hyprland + Alacritty)
**Keywords:** juego, game, tic tac toe, tik tak toe, snake, calculadora, noise cat, archivo, ruta, ejecutar, correr, ventana

## 1. Localizacion Autonoma de Archivos Creados Previamente
Si Daniel no recuerda la ruta de un archivo o proyecto que creaste antes, NUNCA le preguntes la ruta. Buscalo tu mismo inmediatamente ejecutando:
`[TOOL:EXEC_CMD cmd="find ~/proyectos/bob_workspaces /tmp -maxdepth 4 -not -path '*/.*'"]`

## 2. Escritura de Archivos Multilinea
Nunca uses `base64` ni `cat << EOF`. Usa `WRITE_FILE` colocando el atributo `path` y el contenido completo, o actualiza el archivo directamente en el Workspace detectado.

## 3. Compilacion y Lanzamiento Visible en Alacritty (Hyprland)
1. Compila el codigo en Rust con `rustc /ruta/archivo.rs -O -o /ruta/binario` (o `cargo build --release`).
2. Si el programa es interactivo de terminal (como Tic Tac Toe con `stdin`), lanzalo SIEMPRE abriendo una ventana nueva de Alacritty en el escritorio Hyprland de Daniel con:
   `[TOOL:EXEC_CMD cmd="alacritty --title 'BOB Game' -e bash -c '/ruta/binario; echo; read -p \"Presiona Enter para cerrar...\"' >/dev/null 2>&1 &"]`
3. Si el programa tiene interfaz grafica propia (egui/eframe/tkinter), lanzalo en segundo plano con:
   `[TOOL:EXEC_CMD cmd="/ruta/binario >/dev/null 2>&1 &"]`
