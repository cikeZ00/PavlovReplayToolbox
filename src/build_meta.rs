use crate::replay_buffer::ReplayBuffer;
use crate::MetaData;
use chrono::{DateTime, FixedOffset, TimeZone};
use std::error::Error;

const FRIENDLY_NAME_SIZE: usize = 514;

pub fn build_meta(meta: &MetaData) -> Result<Vec<u8>, Box<dyn Error>> {
    let expected_size = 48 + FRIENDLY_NAME_SIZE;
    let mut buf = ReplayBuffer::with_capacity(expected_size);

    buf.write_int32(0x1CA2E27F)?;
    buf.write_int32(6)?;
    buf.write_int32(meta.totalTime)?;
    buf.write_int32(meta.version)?;
    buf.write_int32(0)?;
    buf.write_int32(-257)?;

    let competitive_str = if meta.competitive { "competitive" } else { "casual" };
    let friendly_name_str = format!(
        "{},{},{},0,{},{}",
        meta.gameMode, meta.friendlyName, competitive_str, meta.workshop_mods, meta.live
    );

    let mut name_bytes: Vec<u8> = friendly_name_str
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();

    let mut friendly_name_buffer = vec![0u8; FRIENDLY_NAME_SIZE];
    for i in (0..(FRIENDLY_NAME_SIZE.saturating_sub(2))).step_by(2) {
        friendly_name_buffer[i] = 0x20;
        friendly_name_buffer[i + 1] = 0x00;
    }

    friendly_name_buffer[FRIENDLY_NAME_SIZE.saturating_sub(2)] = 0x00;
    friendly_name_buffer[FRIENDLY_NAME_SIZE - 1] = 0x00;

    let copy_len = std::cmp::min(name_bytes.len(), FRIENDLY_NAME_SIZE);
    friendly_name_buffer[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
    buf.write_bytes(&friendly_name_buffer)?;
    buf.write_int32(if meta.live { 1 } else { 0 })?;

    let created_time = DateTime::parse_from_rfc3339(&meta.created)
        .or_else(|_| {
            let ts = meta.created.parse::<i64>()?;
            let utc_dt = DateTime::<chrono::Utc>::from_timestamp(ts, 0)
                .ok_or("Invalid timestamp")?;
            Ok::<DateTime<FixedOffset>, Box<dyn Error>>(utc_dt.fixed_offset())
        })?;

    let timestamp = created_time.timestamp_millis() * 10000 + 621355968000000000;
    buf.write_int64(timestamp)?;
    buf.write_int32(0)?;
    buf.write_int32(0)?;
    buf.write_int32(0)?;

    buf.validate(expected_size)?;
    Ok(buf.into_inner())
}