# Skill: Ingenieria Bare-Metal en Rust y Tauri v2 (Arch Linux + Hyprland)
**Keywords:** rust, cargo, tauri, hyprland, wayland, simd, vector_db, bob_engine

## 1. Reglas Inquebrantables
1. Cero scripts en Python para logica del sistema: todo modulo de BOB debe escribirse en Rust nativo.
2. Respetar la paleta 100% monocromatica en AXIS UI (`zinc-950`, `zinc-900`, `zinc-800`, `white`) y evitar etiquetas `<select>` nativas en WebKit2GTK.
3. Validar toda compilacion con `cargo check` o `cargo build --release` garantizando 0 errores y 0 warnings.
