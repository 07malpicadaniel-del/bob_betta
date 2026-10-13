use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use vector_db::HippocampusEngine;
use vector_db::common::types::{IslandCategory, VectorData};

#[test]
fn test_neuroplasticity_and_background_decay() {
    let wal_file = "/tmp/test_hippocampus.wal";
    let _ = fs::remove_file(wal_file);

    // Se agrega el 6to argumento: Some(wal_file)
    let engine = Arc::new(HippocampusEngine::new(16, 64, 10, 0.001, 5, Some(wal_file)));
    let t0 = 1_700_000_000u64;

    // 1. Nodo en WorkingMemory destinado a expirar (peso bajo = 1)
    let mut vec_stale = VectorData::default();
    vec_stale.raw[0] = 0.5;
    let (id_stale, _) = engine.remember(
        "recuerdo temporal desechable".to_string(),
        vec_stale,
        Some(IslandCategory::WorkingMemory),
        1,
        t0,
    );

    // 2. Nodo en Code que recibirá consultas repetidas
    let mut vec_code = VectorData::default();
    vec_code.raw[0] = 1.0;
    let (id_code, _) = engine.remember(
        "fn hot_path_simd() { ... }".to_string(),
        vec_code.clone(),
        Some(IslandCategory::Code),
        1,
        t0,
    );

    let code_island = engine.islands.get(&IslandCategory::Code).unwrap();
    let initial_layer = code_island
        .nodes
        .get(&id_code)
        .unwrap()
        .active_layer
        .load(std::sync::atomic::Ordering::SeqCst);

    // 3. Ráfaga de consultas para disparar neuroplasticidad
    for _ in 0..6 {
        engine.recall("fn hot_path_simd", vec_code.clone(), 1, t0 + 1);
    }

    // 4. Ejecutar mantenimiento a t0 + 20 segundos (supera TTL de 10s)
    let (pruned, promoted) = engine.decay_worker.run_maintenance_cycle(&engine.islands, t0 + 20);

    assert_eq!(pruned, 1);
    let working_island = engine.islands.get(&IslandCategory::WorkingMemory).unwrap();
    assert!(working_island.nodes.get(&id_stale).is_none());

    assert!(promoted >= 1);
    let final_layer = code_island
        .nodes
        .get(&id_code)
        .unwrap()
        .active_layer
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(final_layer > initial_layer);

    // 5. Verificar concurrencia del hilo daemon
    let handle = engine.start_background_maintenance(Duration::from_millis(100));
    thread::sleep(Duration::from_millis(250));
    engine.stop_background_maintenance();
    let _ = handle.join();

    // 6. Probar recuperación en frío desde el WAL (pasando los 6 argumentos)
    let engine_recovered = HippocampusEngine::new(16, 64, 10, 0.001, 5, Some(wal_file));
    let recovered_items = engine_recovered.recover_from_wal();
    assert_eq!(recovered_items, 2);

    let _ = fs::remove_file(wal_file);
}
