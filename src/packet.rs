//! Wire format: binary and JSON telemetry packet decoding and encoding.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Packet magic: ASCII "TLMY".
pub const MAGIC: u32 = 0x544C_4D59;
/// Protocol version written by the encoder.
pub const VERSION: u8 = 1;
/// Message type of a telemetry sample.
pub const MSG_TELEMETRY: u8 = 1;
/// Header size: magic, version, msg_type, payload_len, seq, timestamp_ns.
pub const HEADER_LEN: usize = 20;
/// Size of the telemetry payload.
pub const PAYLOAD_LEN: usize = 88;
/// Trailing CRC-32 size.
pub const CRC_LEN: usize = 4;
/// Size of a complete telemetry packet.
pub const PACKET_LEN: usize = HEADER_LEN + PAYLOAD_LEN + CRC_LEN;

/// Reasons a datagram could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    #[error("empty datagram")]
    Empty,
    #[error("truncated packet: need {needed} bytes, got {got}")]
    Truncated { needed: usize, got: usize },
    #[error("bad magic 0x{0:08X}")]
    BadMagic(u32),
    #[error("CRC mismatch: packet has 0x{expected:08X}, computed 0x{actual:08X}")]
    BadCrc { expected: u32, actual: u32 },
    #[error("unsupported message type {0}")]
    UnsupportedType(u8),
    #[error("payload too short: {0} bytes, need at least 88")]
    PayloadTooShort(usize),
    #[error("invalid JSON packet: {0}")]
    Json(String),
}

/// One system telemetry sample. Missing JSON fields default to zero.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Telemetry {
    pub cpu_percent: f32,
    pub mem_total_kb: u64,
    pub mem_avail_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
    pub uptime_s: u64,
    pub load1: f32,
    pub load5: f32,
    pub load15: f32,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

impl Telemetry {
    /// Used memory as a percentage of total memory.
    pub fn mem_used_pct(&self) -> f64 {
        used_pct(self.mem_total_kb, self.mem_avail_kb)
    }

    /// Used swap as a percentage of total swap.
    pub fn swap_used_pct(&self) -> f64 {
        used_pct(self.swap_total_kb, self.swap_free_kb)
    }
}

fn used_pct(total: u64, free: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let used = total.saturating_sub(free);
    (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
}

/// A decoded packet. `seq` is `None` for JSON packets that omit it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Packet {
    pub seq: Option<u32>,
    pub timestamp_ns: u64,
    pub telemetry: Telemetry,
}

fn be_u16(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}

fn be_u32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn be_u64(b: &[u8], o: usize) -> u64 {
    u64::from_be_bytes([
        b[o],
        b[o + 1],
        b[o + 2],
        b[o + 3],
        b[o + 4],
        b[o + 5],
        b[o + 6],
        b[o + 7],
    ])
}

fn be_f32(b: &[u8], o: usize) -> f32 {
    f32::from_bits(be_u32(b, o))
}

/// Decode a datagram, detecting binary or JSON by its first byte.
pub fn decode(buf: &[u8]) -> Result<Packet, DecodeError> {
    let first = buf.iter().copied().find(|b| !b.is_ascii_whitespace());
    match first {
        None => Err(DecodeError::Empty),
        Some(b'{') => decode_json(buf),
        Some(_) => decode_binary(buf),
    }
}

/// Decode a binary packet. Bytes after the CRC are ignored.
pub fn decode_binary(buf: &[u8]) -> Result<Packet, DecodeError> {
    if buf.is_empty() {
        return Err(DecodeError::Empty);
    }
    if buf.len() < 4 {
        return Err(DecodeError::Truncated {
            needed: HEADER_LEN,
            got: buf.len(),
        });
    }
    let magic = be_u32(buf, 0);
    if magic != MAGIC {
        return Err(DecodeError::BadMagic(magic));
    }
    if buf.len() < HEADER_LEN {
        return Err(DecodeError::Truncated {
            needed: HEADER_LEN,
            got: buf.len(),
        });
    }
    let msg_type = buf[5];
    let payload_len = usize::from(be_u16(buf, 6));
    let seq = be_u32(buf, 8);
    let timestamp_ns = be_u64(buf, 12);

    let body_end = HEADER_LEN + payload_len;
    let total = body_end + CRC_LEN;
    if buf.len() < total {
        return Err(DecodeError::Truncated {
            needed: total,
            got: buf.len(),
        });
    }
    let expected = be_u32(buf, body_end);
    let actual = crc32fast::hash(&buf[..body_end]);
    if expected != actual {
        return Err(DecodeError::BadCrc { expected, actual });
    }
    if msg_type != MSG_TELEMETRY {
        return Err(DecodeError::UnsupportedType(msg_type));
    }
    if payload_len < PAYLOAD_LEN {
        return Err(DecodeError::PayloadTooShort(payload_len));
    }

    let p = HEADER_LEN;
    let telemetry = Telemetry {
        cpu_percent: be_f32(buf, p),
        mem_total_kb: be_u64(buf, p + 4),
        mem_avail_kb: be_u64(buf, p + 12),
        swap_total_kb: be_u64(buf, p + 20),
        swap_free_kb: be_u64(buf, p + 28),
        uptime_s: be_u64(buf, p + 36),
        load1: be_f32(buf, p + 44),
        load5: be_f32(buf, p + 48),
        load15: be_f32(buf, p + 52),
        rx_bytes: be_u64(buf, p + 56),
        tx_bytes: be_u64(buf, p + 64),
        rx_packets: be_u64(buf, p + 72),
        tx_packets: be_u64(buf, p + 80),
    };
    Ok(Packet {
        seq: Some(seq),
        timestamp_ns,
        telemetry,
    })
}

/// Decode a one-line JSON packet. Unknown fields are ignored and missing
/// telemetry fields default to zero.
pub fn decode_json(buf: &[u8]) -> Result<Packet, DecodeError> {
    let value: serde_json::Value =
        serde_json::from_slice(buf).map_err(|e| DecodeError::Json(e.to_string()))?;
    if !value.is_object() {
        return Err(DecodeError::Json("expected a JSON object".to_string()));
    }
    let seq = value
        .get("seq")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok());
    let timestamp_ns = value
        .get("ts_ns")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let telemetry: Telemetry =
        serde_json::from_value(value).map_err(|e| DecodeError::Json(e.to_string()))?;
    Ok(Packet {
        seq,
        timestamp_ns,
        telemetry,
    })
}

/// Serialize the payload in wire order.
pub fn encode_payload(t: &Telemetry) -> Vec<u8> {
    let mut out = Vec::with_capacity(PAYLOAD_LEN);
    out.extend_from_slice(&t.cpu_percent.to_bits().to_be_bytes());
    out.extend_from_slice(&t.mem_total_kb.to_be_bytes());
    out.extend_from_slice(&t.mem_avail_kb.to_be_bytes());
    out.extend_from_slice(&t.swap_total_kb.to_be_bytes());
    out.extend_from_slice(&t.swap_free_kb.to_be_bytes());
    out.extend_from_slice(&t.uptime_s.to_be_bytes());
    out.extend_from_slice(&t.load1.to_bits().to_be_bytes());
    out.extend_from_slice(&t.load5.to_bits().to_be_bytes());
    out.extend_from_slice(&t.load15.to_bits().to_be_bytes());
    out.extend_from_slice(&t.rx_bytes.to_be_bytes());
    out.extend_from_slice(&t.tx_bytes.to_be_bytes());
    out.extend_from_slice(&t.rx_packets.to_be_bytes());
    out.extend_from_slice(&t.tx_packets.to_be_bytes());
    out
}

/// Build a complete frame (header, payload, CRC) around an arbitrary payload.
pub fn encode_frame(msg_type: u8, seq: u32, timestamp_ns: u64, payload: &[u8]) -> Vec<u8> {
    let len = u16::try_from(payload.len()).unwrap_or(u16::MAX);
    let payload = &payload[..usize::from(len)];
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len() + CRC_LEN);
    out.extend_from_slice(&MAGIC.to_be_bytes());
    out.push(VERSION);
    out.push(msg_type);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&timestamp_ns.to_be_bytes());
    out.extend_from_slice(payload);
    let crc = crc32fast::hash(&out);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

/// Encode a telemetry sample as a binary packet.
pub fn encode(seq: u32, timestamp_ns: u64, t: &Telemetry) -> Vec<u8> {
    encode_frame(MSG_TELEMETRY, seq, timestamp_ns, &encode_payload(t))
}

/// Encode a telemetry sample as a one-line JSON packet.
pub fn encode_json(seq: u32, timestamp_ns: u64, t: &Telemetry) -> String {
    let mut value = serde_json::to_value(t).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(map) = &mut value {
        map.insert("seq".to_string(), seq.into());
        map.insert("ts_ns".to_string(), timestamp_ns.into());
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Telemetry {
        Telemetry {
            cpu_percent: 42.5,
            mem_total_kb: 8_000_000,
            mem_avail_kb: 2_000_000,
            swap_total_kb: 1_000_000,
            swap_free_kb: 750_000,
            uptime_s: 86_400,
            load1: 0.5,
            load5: 0.25,
            load15: 0.125,
            rx_bytes: 1_000_000,
            tx_bytes: 500_000,
            rx_packets: 1_000,
            tx_packets: 800,
        }
    }

    #[test]
    fn packet_length_matches_constants() {
        assert_eq!(encode_payload(&sample()).len(), PAYLOAD_LEN);
        assert_eq!(encode(1, 2, &sample()).len(), PACKET_LEN);
    }

    #[test]
    fn binary_round_trip() {
        let bytes = encode(7, 123_456_789, &sample());
        let p = decode(&bytes).unwrap();
        assert_eq!(p.seq, Some(7));
        assert_eq!(p.timestamp_ns, 123_456_789);
        assert_eq!(p.telemetry, sample());
    }

    #[test]
    fn header_is_network_byte_order() {
        let bytes = encode(0x0102_0304, 0, &sample());
        assert_eq!(&bytes[0..4], b"TLMY");
        assert_eq!(bytes[4], VERSION);
        assert_eq!(bytes[5], MSG_TELEMETRY);
        assert_eq!(&bytes[6..8], &[0, 88]);
        assert_eq!(&bytes[8..12], &[1, 2, 3, 4]);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = encode(1, 1, &sample());
        bytes[0] = b'X';
        assert!(matches!(
            decode_binary(&bytes),
            Err(DecodeError::BadMagic(_))
        ));
    }

    #[test]
    fn rejects_corrupted_payload_with_crc_error() {
        let mut bytes = encode(1, 1, &sample());
        bytes[HEADER_LEN + 10] ^= 0xFF;
        assert!(matches!(
            decode_binary(&bytes),
            Err(DecodeError::BadCrc { .. })
        ));
    }

    #[test]
    fn rejects_corrupted_header_and_crc_field() {
        let mut bytes = encode(1, 1, &sample());
        bytes[9] ^= 0x01;
        assert!(matches!(
            decode_binary(&bytes),
            Err(DecodeError::BadCrc { .. })
        ));

        let mut bytes = encode(1, 1, &sample());
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        assert!(matches!(
            decode_binary(&bytes),
            Err(DecodeError::BadCrc { .. })
        ));
    }

    #[test]
    fn rejects_truncated_input_at_every_length() {
        let bytes = encode(1, 1, &sample());
        for len in 1..bytes.len() {
            let err = decode_binary(&bytes[..len]).unwrap_err();
            assert!(
                matches!(err, DecodeError::Truncated { .. }),
                "length {len} gave {err:?}"
            );
        }
        assert_eq!(decode_binary(&[]), Err(DecodeError::Empty));
    }

    #[test]
    fn truncated_reports_needed_size() {
        let bytes = encode(1, 1, &sample());
        let err = decode_binary(&bytes[..HEADER_LEN + 5]).unwrap_err();
        assert_eq!(
            err,
            DecodeError::Truncated {
                needed: PACKET_LEN,
                got: HEADER_LEN + 5
            }
        );
    }

    #[test]
    fn rejects_unsupported_message_type() {
        let bytes = encode_frame(9, 1, 1, &encode_payload(&sample()));
        assert_eq!(decode_binary(&bytes), Err(DecodeError::UnsupportedType(9)));
    }

    #[test]
    fn rejects_short_payload() {
        let bytes = encode_frame(MSG_TELEMETRY, 1, 1, &[0u8; 10]);
        assert_eq!(decode_binary(&bytes), Err(DecodeError::PayloadTooShort(10)));
    }

    #[test]
    fn accepts_longer_payload_and_trailing_bytes() {
        let mut payload = encode_payload(&sample());
        payload.extend_from_slice(&[0xAA; 16]);
        let mut bytes = encode_frame(MSG_TELEMETRY, 3, 4, &payload);
        bytes.extend_from_slice(&[0xBB; 5]);
        let p = decode_binary(&bytes).unwrap();
        assert_eq!(p.seq, Some(3));
        assert_eq!(p.telemetry, sample());
    }

    #[test]
    fn json_round_trip() {
        let line = encode_json(11, 99, &sample());
        let p = decode(line.as_bytes()).unwrap();
        assert_eq!(p.seq, Some(11));
        assert_eq!(p.timestamp_ns, 99);
        assert_eq!(p.telemetry, sample());
    }

    #[test]
    fn json_missing_fields_default_and_unknown_ignored() {
        let p = decode(br#"{"cpu_percent": 12.5, "extra": "x"}"#).unwrap();
        assert_eq!(p.seq, None);
        assert_eq!(p.timestamp_ns, 0);
        assert_eq!(p.telemetry.cpu_percent, 12.5);
        assert_eq!(p.telemetry.mem_total_kb, 0);
    }

    #[test]
    fn json_tolerates_whitespace_and_newline() {
        let p = decode(b"  {\"seq\": 5, \"load1\": 1.5}\r\n").unwrap();
        assert_eq!(p.seq, Some(5));
        assert_eq!(p.telemetry.load1, 1.5);
    }

    #[test]
    fn json_errors() {
        assert!(matches!(
            decode(b"{\"cpu_percent\": "),
            Err(DecodeError::Json(_))
        ));
        assert!(matches!(
            decode(b"{\"cpu_percent\": \"high\"}"),
            Err(DecodeError::Json(_))
        ));
        assert!(matches!(decode_json(b"[1,2]"), Err(DecodeError::Json(_))));
    }

    #[test]
    fn decode_detects_empty_and_garbage() {
        assert_eq!(decode(b""), Err(DecodeError::Empty));
        assert_eq!(decode(b"  \n"), Err(DecodeError::Empty));
        assert!(matches!(
            decode(b"hello world, this is not telemetry"),
            Err(DecodeError::BadMagic(_))
        ));
    }

    #[test]
    fn percentages() {
        let t = sample();
        assert!((t.mem_used_pct() - 75.0).abs() < 1e-9);
        assert!((t.swap_used_pct() - 25.0).abs() < 1e-9);
        assert_eq!(Telemetry::default().mem_used_pct(), 0.0);
        let odd = Telemetry {
            mem_total_kb: 10,
            mem_avail_kb: 50,
            ..Telemetry::default()
        };
        assert_eq!(odd.mem_used_pct(), 0.0);
    }
}
