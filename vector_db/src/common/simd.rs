use crate::common::types::{VectorData, VECTOR_DIM};

#[inline(always)]
pub fn dot_product(a: &VectorData, b: &VectorData) -> f32 {
    let mut sum0 = 0.0f32;
    let mut sum1 = 0.0f32;
    let mut sum2 = 0.0f32;
    let mut sum3 = 0.0f32;

    let chunk_size = 4;
    let iterations = VECTOR_DIM / chunk_size;

    for i in 0..iterations {
        let base = i * chunk_size;
        sum0 += a.raw[base] * b.raw[base];
        sum1 += a.raw[base + 1] * b.raw[base + 1];
        sum2 += a.raw[base + 2] * b.raw[base + 2];
        sum3 += a.raw[base + 3] * b.raw[base + 3];
    }

    let total = sum0 + sum1 + sum2 + sum3;
    total.clamp(-1.0, 1.0)
}
