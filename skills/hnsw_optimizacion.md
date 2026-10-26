# Skill: Optimización HNSW 768-dim en Rust (AVX2)

## Contexto
- **Índice**: HNSW, D=768, M=32, efConstruction=200, efSearch=64
- **Hardware**: Ryzen 9 (AVX2, 256-bit), RTX 5060 Ti
- **SLA**: Recall@10 > 0.95, p99 < 5ms
- **Fuente**: Gemini (consulta 2025-06-18) + síntesis Bob

---

## Invariante 1: Alineación Vectorial SIMD (AVX2)

### (1) Fórmula formal
```
D mod L_SIMD = 0  ⟹  768 mod 8 = 0
```
Garantiza exactamente **96 iteraciones vectoriales puras** por distancia (dot product / L2) sin tail processing.

### (2) Violación práctica
Padding dinámico incorrecto o offsets en runtime → el compilador genera bucles escalares de fallback para residuos → picos de latencia > 5ms en p99 durante efSearch=64.

### (3) Patrón Rust
```rust
const D: usize = 768;
const L_SIMD: usize = 8; // 256 bits / 32 bits
const N_LANES: usize = D / L_SIMD; // 96

#[repr(align(32))