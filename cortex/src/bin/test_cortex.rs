use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use cortex::{CortexEngine, ModelConfig};

fn main() {
    println!("==================================================");
    println!("     Test de Inferencia Directa: Lego 2 (CUDA)    ");
    println!("==================================================");

    // Ruta al modelo GGUF pasado como argumento o por defecto en ../models/
    let model_arg = env::args().nth(1).unwrap_or_else(|| {
        "../models/model.gguf".to_string()
    });
    let model_path = PathBuf::from(&model_arg);

    if !model_path.exists() {
        eprintln!("[!] No se encontró el archivo del modelo en: {:?}", model_path);
        eprintln!("[i] Uso: cargo run -p cortex --bin test_cortex -- <ruta_al_archivo.gguf>");
        return;
    }

    println!("[*] Cargando modelo en VRAM: {:?}", model_path);

    let config = ModelConfig {
        model_path: model_path.clone(),
        n_gpu_layers: 0, // Forzar todas las capas a la RTX 5060 Ti
        context_size: 2048,
        batch_size: 512,
        threads: 8,
    };

    let start_load = std::time::Instant::now();
    let mut engine = match CortexEngine::load_from_file(&model_path, config) {
        Ok(e) => {
            println!("[✓] Modelo cargado en GPU en {:.2?}", start_load.elapsed());
            e
        }
        Err(err) => {
            eprintln!("[!] Error al cargar modelo en VRAM: {}", err);
            return;
        }
    };

    let prompt = "System: Eres Bob, un agente autónomo de ingeniería.\nUser: Explica brevemente qué es la neuroplasticidad en sistemas de memoria.\nBob:";
    println!("\n[*] Enviando Prompt de prueba...");
    println!("--------------------------------------------------");

    let mut stdout = io::stdout();
    let start_infer = std::time::Instant::now();

    let result = engine.generate(prompt, 128, |token| {
        print!("{}", token);
        let _ = stdout.flush();
    });

    match result {
        Ok(_) => {
            println!("\n--------------------------------------------------");
            println!("[✓] Inferencia completada en {:.2?}", start_infer.elapsed());
        }
        Err(e) => {
            eprintln!("\n[!] Error en ejecución: {}", e);
        }
    }
}
