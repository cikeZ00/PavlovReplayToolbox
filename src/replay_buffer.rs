// replay_buffer.rs

use std::error::Error;

pub struct ReplayBuffer {
    buffer: Vec<u8>,
    pos: usize,
}

impl ReplayBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        let mut buffer = vec![0u8; capacity];
        Self { buffer, pos: 0 }
    }

    pub fn offset(&self) -> usize {
        self.pos
    }

    pub fn write_int32(&mut self, value: i32) -> Result<(), Box<dyn Error>> {
        if self.pos + 4 > self.buffer.len() {
            return Err("Buffer overflow while writing int32".into());
        }
        self.buffer[self.pos..self.pos + 4].copy_from_slice(&value.to_le_bytes());
        self.pos += 4;
        Ok(())
    }

    pub fn write_int64(&mut self, value: i64) -> Result<(), Box<dyn Error>> {
        if self.pos + 8 > self.buffer.len() {
            return Err("Buffer overflow while writing int64".into());
        }
        self.buffer[self.pos..self.pos + 8].copy_from_slice(&value.to_le_bytes());
        self.pos += 8;
        Ok(())
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
        if self.pos + bytes.len() > self.buffer.len() {
            return Err("Buffer overflow while writing bytes".into());
        }
        self.buffer[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }

    /// Validates that the final offset equals the expected size.
    pub fn validate(&self, expected: usize) -> Result<(), Box<dyn Error>> {
        if self.pos != expected {
            Err(format!(
                "Invalid buffer size. Expected to write {} bytes, instead wrote {}",
                expected, self.pos
            )
                .into())
        } else {
            Ok(())
        }
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buffer
    }
}
