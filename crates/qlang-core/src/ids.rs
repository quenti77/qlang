//! `uuid` and `ulid`: both are 128 bits. Nothing here reads a clock or a random source;
//! the interpreter asks the host and passes the time and the random bits in.

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The first 48 bits of a `ulid` or of a version 7 `uuid`.
pub const TIME_MASK: u128 = (1 << 48) - 1;
/// The last 80 bits of a `ulid`.
pub const ULID_RANDOM_MASK: u128 = (1 << 80) - 1;

/// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, in lower case.
pub fn format_uuid(v: u128) -> String {
    let h = format!("{v:032x}");
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

pub fn parse_uuid(text: &str) -> Option<u128> {
    let b = text.as_bytes();
    if b.len() != 36 {
        return None;
    }
    let mut v: u128 = 0;
    for (i, c) in b.iter().enumerate() {
        if matches!(i, 8 | 13 | 18 | 23) {
            if *c != b'-' {
                return None;
            }
            continue;
        }
        v = (v << 4) | (*c as char).to_digit(16)? as u128;
    }
    Some(v)
}

/// A random uuid, version 4: 122 random bits.
pub fn uuid_v4(random: u128) -> u128 {
    with_version_and_variant(random, 4)
}

/// A uuid, version 7: the time in milliseconds (48 bits) and 74 random bits.
pub fn uuid_v7(time_ms: u128, random: u128) -> u128 {
    with_version_and_variant((random & ((1 << 80) - 1)) | ((time_ms & TIME_MASK) << 80), 7)
}

fn with_version_and_variant(bits: u128, version: u128) -> u128 {
    let cleared = bits & !(0xF << 76) & !(0x3 << 62);
    cleared | (version << 76) | (0x2 << 62)
}

pub fn uuid_version(v: u128) -> i64 {
    ((v >> 76) & 0xF) as i64
}

/// 26 characters of Crockford base32: 10 for the time, 16 for the random part.
pub fn format_ulid(v: u128) -> String {
    let mut out = [0u8; 26];
    for (i, slot) in out.iter_mut().enumerate() {
        let shift = 5 * (25 - i);
        *slot = CROCKFORD[((v >> shift) & 31) as usize];
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_ulid(text: &str) -> Option<u128> {
    let b = text.as_bytes();
    if b.len() != 26 {
        return None;
    }
    let mut v: u128 = 0;
    for (i, c) in b.iter().enumerate() {
        let d = CROCKFORD.iter().position(|k| *k == c.to_ascii_uppercase())? as u128;
        // 26 * 5 = 130 bits: the first character only has 3 usable bits
        if i == 0 && d > 7 {
            return None;
        }
        v = (v << 5) | d;
    }
    Some(v)
}

pub fn ulid_time(v: u128) -> i64 {
    (v >> 80) as i64
}

pub fn make_ulid(time_ms: u128, random: u128) -> u128 {
    ((time_ms & TIME_MASK) << 80) | (random & ULID_RANDOM_MASK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_round_trip() {
        let v = uuid_v4(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let text = format_uuid(v);
        assert_eq!(parse_uuid(&text), Some(v));
        assert_eq!(&text[14..15], "4");
        assert!(matches!(&text[19..20], "8" | "9" | "a" | "b"));
        assert_eq!(uuid_version(v), 4);
        assert_eq!(parse_uuid(&text.to_uppercase()), Some(v));
    }

    #[test]
    fn uuid_rejects_malformed_text() {
        assert_eq!(parse_uuid(""), None);
        assert_eq!(parse_uuid("0123456789abcdef0123456789abcdef"), None);
        assert_eq!(parse_uuid("0123456x-89ab-cdef-0123-456789abcdef"), None);
        assert_eq!(parse_uuid("01234567_89ab-cdef-0123-456789abcdef"), None);
    }

    #[test]
    fn uuid_v7_keeps_the_time_first() {
        let v = uuid_v7(1_700_000_000_000, 12345);
        assert_eq!(uuid_version(v), 7);
        assert_eq!((v >> 80) as u64, 1_700_000_000_000);
    }

    #[test]
    fn ulid_known_value() {
        // the example of the ulid specification
        assert_eq!(format_ulid(make_ulid(1_469_918_176_385, 0)), "01ARYZ6S410000000000000000");
        let v = parse_ulid("01ARYZ6S41TSV4RRFFQ69G5FAV").unwrap();
        assert_eq!(ulid_time(v), 1_469_918_176_385);
        assert_eq!(format_ulid(v), "01ARYZ6S41TSV4RRFFQ69G5FAV");
    }

    #[test]
    fn ulid_rejects_invalid_text() {
        assert_eq!(parse_ulid("01ARYZ6S41TSV4RRFFQ69G5FA"), None);
        assert_eq!(parse_ulid("81ARYZ6S41TSV4RRFFQ69G5FAV"), None);
        assert_eq!(parse_ulid("01ARYZ6S41TSV4RRFFQ69G5FAU"), None);
        assert_eq!(parse_ulid("01arYZ6S41TSV4RRFFQ69G5FAV").map(format_ulid).as_deref(), Some("01ARYZ6S41TSV4RRFFQ69G5FAV"));
    }
}
