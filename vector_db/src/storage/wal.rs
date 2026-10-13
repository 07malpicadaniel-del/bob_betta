use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use crate::common::types::{IslandCategory, NodeId, VectorData, VECTOR_DIM};

pub struct WalRecord<'a> {
    pub id: NodeId,
    pub category: IslandCategory,
    pub vector: &'a VectorData,
    pub text: &'a str,
}

pub struct RecoveredRecord {
    pub id: NodeId,
    pub category: IslandCategory,
    pub text: String,
    pub vector: VectorData,
}

pub struct WriteAheadLog {
    writer: BufWriter<File>,
}

impl WriteAheadLog {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            writer: BufWriter::new(file),
        })
    }

    /// Escritura secuencial binaria: [NodeId (8B)] [Category (1B)] [TextLen (4B)] [Text] [Vector (1536 * 4B)]
    pub fn append(&mut self, record: &WalRecord) -> io::Result<()> {
        self.writer.write_all(&record.id.to_le_bytes())?;

        let cat_byte = match record.category {
            IslandCategory::Code => 0u8,
            IslandCategory::EpisodicMemory => 1u8,
            IslandCategory::CoreRules => 2u8,
            IslandCategory::WorkingMemory => 3u8,
        };
        self.writer.write_all(&[cat_byte])?;

        let text_bytes = record.text.as_bytes();
        self.writer.write_all(&(text_bytes.len() as u32).to_le_bytes())?;
        self.writer.write_all(text_bytes)?;

        let slice_u8 = unsafe {
            std::slice::from_raw_parts(
                record.vector.raw.as_ptr() as *const u8,
                VECTOR_DIM * std::mem::size_of::<f32>(),
            )
        };
        self.writer.write_all(slice_u8)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Lee todo el archivo binario secuencialmente para reconstruir la memoria tras un reinicio
    pub fn replay<P: AsRef<Path>>(path: P) -> io::Result<Vec<RecoveredRecord>> {
        if !path.as_ref().exists() {
            return Ok(Vec::new());
        }

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut records = Vec::new();

        let mut id_buf = [0u8; 8];
        let mut cat_buf = [0u8; 1];
        let mut len_buf = [0u8; 4];

        while reader.read_exact(&mut id_buf).is_ok() {
            reader.read_exact(&mut cat_buf)?;
            let id = u64::from_le_bytes(id_buf);
            let category = match cat_buf[0] {
                0 => IslandCategory::Code,
                1 => IslandCategory::EpisodicMemory,
                2 => IslandCategory::CoreRules,
                _ => IslandCategory::WorkingMemory,
            };

            reader.read_exact(&mut len_buf)?;
            let text_len = u32::from_le_bytes(len_buf) as usize;

            let mut text_bytes = vec![0u8; text_len];
            reader.read_exact(&mut text_bytes)?;
            let text = String::from_utf8_lossy(&text_bytes).into_owned();

            let mut vector = VectorData::default();
            let slice_u8 = unsafe {
                std::slice::from_raw_parts_mut(
                    vector.raw.as_mut_ptr() as *mut u8,
                    VECTOR_DIM * std::mem::size_of::<f32>(),
                )
            };
            reader.read_exact(slice_u8)?;

            records.push(RecoveredRecord {
                id,
                category,
                text,
                vector,
            });
        }

        Ok(records)
    }
}
