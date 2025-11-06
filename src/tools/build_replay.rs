use crate::tools::replay_processor::Chunk;
use std::error::Error;

/// A part of the replay file: either the meta part or a chunk.
pub enum ReplayPart {
    Meta(Vec<u8>),
    Chunk(Chunk),
}

/// Helper to write a string buffer as: [int32 length][utf8 bytes with null terminator]
fn write_string_buffer(s: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    // Append a null terminator
    let mut s_with_null = s.to_owned();
    s_with_null.push('\0');
    let s_bytes = s_with_null.as_bytes();
    let length = s_bytes.len() as i32;
    buf.extend(&length.to_le_bytes());
    buf.extend(s_bytes);
    buf
}

/// Build the final replay buffer.
pub fn build_replay(parts: &[ReplayPart]) -> Result<Vec<u8>, Box<dyn Error>> {
    // Pre-calculate total buffer size to avoid reallocations
    let mut total_size = 0;
    for part in parts {
        match part {
            ReplayPart::Meta(data) => {
                total_size += data.len();
            }
            ReplayPart::Chunk(chunk) => {
                // 8 bytes for chunk header
                total_size += 8;
                match chunk.chunk_type {
                    0 => {
                        total_size += chunk.data.len();
                    }
                    1 => {
                        total_size += 16 + chunk.data.len();
                    }
                    2 | 3 => {
                        // Estimate string buffer sizes
                        let id_len = chunk.id.as_ref().map(|s| s.len() + 5).unwrap_or(5);
                        let group_len = chunk.group.as_ref().map(|s| s.len() + 5).unwrap_or(5);
                        let meta_len = chunk.metadata.as_ref().map(|s| s.len() + 5).unwrap_or(5);
                        total_size += id_len + group_len + meta_len + 12 + chunk.data.len();
                    }
                    _ => {}
                }
            }
        }
    }
    
    let mut buffers: Vec<u8> = Vec::with_capacity(total_size);

    for part in parts {
        match part {
            ReplayPart::Meta(data) => {
                // Meta parts are assumed to be already serialized.
                buffers.extend(data);
            }
            ReplayPart::Chunk(chunk) => {
                let mut body_buffer = Vec::new();
                match chunk.chunk_type {
                    // Chunk type 0: Header. Write the raw data.
                    0 => {
                        body_buffer.extend(&chunk.data);
                    }
                    // Chunk type 1: Data chunk.
                    1 => {
                        let mut header_buf = [0u8; 16];
                        let time1 = chunk.time1.unwrap_or(0);
                        let time2 = chunk.time2.unwrap_or(0);
                        let data_len = chunk.data.len() as i32;
                        let size_in_bytes = chunk.size_in_bytes.unwrap_or(data_len);
                        header_buf[0..4].copy_from_slice(&time1.to_le_bytes());
                        header_buf[4..8].copy_from_slice(&time2.to_le_bytes());
                        header_buf[8..12].copy_from_slice(&data_len.to_le_bytes());
                        header_buf[12..16].copy_from_slice(&size_in_bytes.to_le_bytes());
                        body_buffer.extend(&header_buf);
                        body_buffer.extend(&chunk.data);
                    }
                    // Chunk types 2 and 3: Checkpoint / Event chunks.
                    2 | 3 => {
                        let id_buf = write_string_buffer(chunk.id.as_ref().unwrap());
                        let group_buf = write_string_buffer(chunk.group.as_ref().unwrap());
                        let meta_str = chunk.metadata.clone().unwrap_or_default();
                        let meta_buf = write_string_buffer(&meta_str);
                        let mut int_buf = [0u8; 12];
                        let time1 = chunk.time1.unwrap_or(0);
                        let time2 = chunk.time2.unwrap_or(0);
                        let data_len = chunk.data.len() as i32;
                        int_buf[0..4].copy_from_slice(&time1.to_le_bytes());
                        int_buf[4..8].copy_from_slice(&time2.to_le_bytes());
                        int_buf[8..12].copy_from_slice(&data_len.to_le_bytes());
                        body_buffer.extend(id_buf);
                        body_buffer.extend(group_buf);
                        body_buffer.extend(meta_buf);
                        body_buffer.extend(&int_buf);
                        body_buffer.extend(&chunk.data);
                    }
                    other => {
                        eprintln!("Unknown chunk type encountered: {}", other);
                        continue;
                    }
                }
                // Build chunk header (8 bytes): [chunk_type (int32), body length (int32)]
                let mut header_buffer = [0u8; 8];
                header_buffer[0..4].copy_from_slice(&chunk.chunk_type.to_le_bytes());
                let body_len = body_buffer.len() as i32;
                header_buffer[4..8].copy_from_slice(&body_len.to_le_bytes());
                buffers.extend(&header_buffer);
                buffers.extend(&body_buffer);
            }
        }
    }
    Ok(buffers)
}
