use crate::common::types::VectorData;

pub struct VectorNormalizer;

impl VectorNormalizer {
    /// Normaliza el vector in-place para que su longitud L2 sea exactamente 1.0
    pub fn normalize_l2(vector: &mut VectorData) {
        let mut sum_squares = 0.0f32;
        for &val in vector.raw.iter() {
            sum_squares += val * val;
        }

        let magnitude = sum_squares.sqrt();

        // Evita indeterminación o división por cero en vectores nulos
        if magnitude > f32::EPSILON {
            let inv_magnitude = 1.0 / magnitude;
            for val in vector.raw.iter_mut() {
                *val *= inv_magnitude;
            }
        }
    }
}
