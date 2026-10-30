//! `Gzip`, `Zip`, `Tar` and `Zone`: the work the prelude hands to the `qlang` command (compression
//! and the time zone database). Nothing here touches the disk: it turns bytes into bytes.

use qlang_core::host::{HostError, HostValue};
use std::io::{Cursor, Read, Write};

pub const ARCHIVE_EXTERNS: &[&str] = &[
    "archive_gzip",
    "archive_gunzip",
    "archive_read",
    "archive_write",
    "tz_offset",
    "tz_resolve",
];

fn fail(message: impl Into<String>) -> HostError {
    HostError::Other(message.into())
}

/// Reads at most `max` bytes: a small file that expands into gigabytes is refused.
fn read_limited(mut from: impl Read, max: u64, what: &str) -> Result<Vec<u8>, HostError> {
    let mut out = Vec::new();
    (&mut from)
        .take(max + 1)
        .read_to_end(&mut out)
        .map_err(|e| fail(format!("{what}: {e}")))?;
    if out.len() as u64 > max {
        return Err(fail(format!(
            "{what}: more than {max} bytes once decompressed"
        )));
    }
    Ok(out)
}

/// A name inside an archive that would write outside the folder it is extracted into.
fn unsafe_name(name: &str) -> bool {
    name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\0')
        || name.split(['/', '\\']).any(|part| part == "..")
        || name.chars().nth(1) == Some(':')
}

fn gunzip(data: &[u8], max: u64) -> Result<Vec<u8>, HostError> {
    read_limited(flate2::read::GzDecoder::new(data), max, "gzip")
}

fn gzip(data: &[u8], level: i64) -> Result<Vec<u8>, HostError> {
    let level = flate2::Compression::new(level.clamp(0, 9) as u32);
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), level);
    enc.write_all(data).map_err(|e| fail(e.to_string()))?;
    enc.finish().map_err(|e| fail(e.to_string()))
}

/// `[name, data, name, data...]`
type Parts = Vec<HostValue>;

fn push(parts: &mut Parts, name: &str, data: Vec<u8>) {
    parts.push(HostValue::Bytes(name.as_bytes().to_vec()));
    parts.push(HostValue::Bytes(data));
}

fn read_zip(data: &[u8], max: u64) -> Result<Parts, HostError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(data)).map_err(|e| fail(format!("zip: {e}")))?;
    let mut parts = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| fail(format!("zip: {e}")))?;
        let name = file.name().map(|n| n.to_string()).unwrap_or_default();
        if unsafe_name(&name) {
            return Err(fail(format!("zip: the name \"{name}\" leaves the folder")));
        }
        if file.is_dir() {
            continue;
        }
        let bytes = read_limited(&mut file, max.saturating_sub(total), "zip")?;
        total += bytes.len() as u64;
        push(&mut parts, &name, bytes);
    }
    Ok(parts)
}

fn read_tar(data: &[u8], max: u64) -> Result<Parts, HostError> {
    // a gzip file (`.tar.gz`) is recognised by its first bytes
    let plain;
    let data = if data.starts_with(&[0x1f, 0x8b]) {
        plain = gunzip(data, max)?;
        &plain[..]
    } else {
        data
    };
    let mut tar = tar::Archive::new(data);
    let mut parts = Vec::new();
    let mut total = 0u64;
    for entry in tar.entries().map_err(|e| fail(format!("tar: {e}")))? {
        let mut entry = entry.map_err(|e| fail(format!("tar: {e}")))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry
            .path()
            .map_err(|e| fail(format!("tar: {e}")))?
            .to_string_lossy()
            .to_string();
        if unsafe_name(&name) {
            return Err(fail(format!("tar: the name \"{name}\" leaves the folder")));
        }
        let bytes = read_limited(&mut entry, max.saturating_sub(total), "tar")?;
        total += bytes.len() as u64;
        push(&mut parts, &name, bytes);
    }
    Ok(parts)
}

fn pairs(parts: &[HostValue]) -> Result<Vec<(String, &[u8])>, HostError> {
    let mut out = Vec::new();
    for pair in parts.chunks(2) {
        let [HostValue::Bytes(name), HostValue::Bytes(data)] = pair else {
            return Err(fail("an archive is a list of names and contents"));
        };
        let name = String::from_utf8(name.clone()).map_err(|_| fail("a name is not text"))?;
        if unsafe_name(&name) {
            return Err(fail(format!("the name \"{name}\" leaves the folder")));
        }
        out.push((name, data.as_slice()));
    }
    Ok(out)
}

fn write_zip(parts: &[HostValue]) -> Result<Vec<u8>, HostError> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in pairs(parts)? {
        zip.start_file(name, options)
            .map_err(|e| fail(format!("zip: {e}")))?;
        zip.write_all(data).map_err(|e| fail(e.to_string()))?;
    }
    Ok(zip
        .finish()
        .map_err(|e| fail(format!("zip: {e}")))?
        .into_inner())
}

fn write_tar(parts: &[HostValue], gz: bool) -> Result<Vec<u8>, HostError> {
    let mut tar = tar::Builder::new(Vec::new());
    for (name, data) in pairs(parts)? {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, &name, data)
            .map_err(|e| fail(format!("tar: {e}")))?;
    }
    let bytes = tar.into_inner().map_err(|e| fail(format!("tar: {e}")))?;
    if gz { gzip(&bytes, 6) } else { Ok(bytes) }
}

fn zone(name: &str) -> Result<chrono_tz::Tz, HostError> {
    name.parse::<chrono_tz::Tz>().map_err(|_| {
        fail(format!(
            "unknown time zone \"{name}\" (like \"Europe/Paris\")"
        ))
    })
}

pub fn call(name: &str, args: &[HostValue]) -> Result<HostValue, HostError> {
    use chrono::{Offset, TimeZone};
    let wrong = || fail(format!("`{name}` was called with the wrong arguments"));
    match (name, args) {
        ("archive_gzip", [HostValue::Bytes(data), HostValue::Int(level)]) => {
            gzip(data, *level).map(HostValue::Bytes)
        }
        ("archive_gunzip", [HostValue::Bytes(data), HostValue::Int(max)]) => {
            gunzip(data, (*max).max(0) as u64).map(HostValue::Bytes)
        }
        (
            "archive_read",
            [
                HostValue::Str(kind),
                HostValue::Bytes(data),
                HostValue::Int(max),
            ],
        ) => {
            let max = (*max).max(0) as u64;
            let parts = match kind.as_str() {
                "zip" => read_zip(data, max)?,
                "tar" => read_tar(data, max)?,
                _ => return Err(wrong()),
            };
            Ok(HostValue::Array(parts))
        }
        ("archive_write", [HostValue::Str(kind), HostValue::Array(parts)]) => {
            let bytes = match kind.as_str() {
                "zip" => write_zip(parts)?,
                "tar" => write_tar(parts, false)?,
                "tar.gz" => write_tar(parts, true)?,
                _ => return Err(wrong()),
            };
            Ok(HostValue::Bytes(bytes))
        }
        // [offset in seconds, abbreviation] of the zone at an instant (milliseconds since 1970)
        ("tz_offset", [HostValue::Str(name), HostValue::Int(ms)]) => {
            let tz = zone(name)?;
            let at = chrono::DateTime::from_timestamp_millis(*ms)
                .ok_or_else(|| fail("this instant is out of range"))?;
            let local = tz.from_utc_datetime(&at.naive_utc());
            let offset = local.offset().fix().local_minus_utc();
            Ok(HostValue::Array(vec![
                HostValue::Str(offset.to_string()),
                HostValue::Str(local.format("%Z").to_string()),
            ]))
        }
        // the instant (ms) of a wall-clock reading (given as ms of the same reading in UTC)
        ("tz_resolve", [HostValue::Str(name), HostValue::Int(wall)]) => {
            let tz = zone(name)?;
            let naive = chrono::DateTime::from_timestamp_millis(*wall)
                .ok_or_else(|| fail("this date is out of range"))?
                .naive_utc();
            let instant = match tz.from_local_datetime(&naive) {
                chrono::LocalResult::Single(t) => t.timestamp_millis(),
                // an hour that happens twice (clocks go back): the first time
                chrono::LocalResult::Ambiguous(first, _) => first.timestamp_millis(),
                // an hour that does not exist (clocks go forward): read with the offset before it
                chrono::LocalResult::None => {
                    let before = tz.from_utc_datetime(&(naive - chrono::Duration::days(1)));
                    let offset = before.offset().fix().local_minus_utc() as i64;
                    *wall - offset * 1000
                }
            };
            Ok(HostValue::Int(instant))
        }
        _ => Err(wrong()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(s: &str) -> HostValue {
        HostValue::Bytes(s.as_bytes().to_vec())
    }

    #[test]
    fn gzip_round_trips_and_refuses_a_bomb() {
        let packed = gzip(&vec![b'a'; 100_000], 6).unwrap();
        assert!(packed.len() < 1000);
        assert_eq!(gunzip(&packed, 200_000).unwrap().len(), 100_000);
        assert!(format!("{:?}", gunzip(&packed, 1000).unwrap_err()).contains("more than"));
    }

    #[test]
    fn zip_and_tar_round_trip_and_refuse_names_that_leave_the_folder() {
        let parts = vec![
            bytes("a.txt"),
            bytes("one"),
            bytes("dir/b.txt"),
            bytes("two"),
        ];
        for (kind, write) in [("zip", "zip"), ("tar", "tar"), ("tar", "tar.gz")] {
            let packed = call(
                "archive_write",
                &[
                    HostValue::Str(write.into()),
                    HostValue::Array(parts.clone()),
                ],
            )
            .unwrap();
            let HostValue::Bytes(packed) = packed else {
                panic!()
            };
            let read = call(
                "archive_read",
                &[
                    HostValue::Str(kind.into()),
                    HostValue::Bytes(packed),
                    HostValue::Int(100_000),
                ],
            )
            .unwrap();
            let HostValue::Array(read) = read else {
                panic!()
            };
            assert_eq!(read.len(), 4, "{write}");
            assert!(
                matches!(&read[3], HostValue::Bytes(b) if b == b"two"),
                "{write}"
            );
        }
        for bad in ["../x", "/etc/passwd", "a/../../b", "C:\\x"] {
            let r = call(
                "archive_write",
                &[
                    HostValue::Str("zip".into()),
                    HostValue::Array(vec![bytes(bad), bytes("x")]),
                ],
            );
            assert!(r.is_err(), "{bad}");
        }
    }

    #[test]
    fn zones_give_their_offset_and_resolve_a_wall_clock() {
        // 2026-07-01 12:00 UTC: Paris is on summer time
        let summer = 1_782_907_200_000i64;
        let HostValue::Array(v) = call(
            "tz_offset",
            &[
                HostValue::Str("Europe/Paris".into()),
                HostValue::Int(summer),
            ],
        )
        .unwrap() else {
            panic!()
        };
        assert!(matches!(&v[0], HostValue::Str(s) if s == "7200"));
        assert!(matches!(&v[1], HostValue::Str(s) if s == "CEST"));
        // 14:00 on the wall in Paris in summer is 12:00 UTC
        let r = call(
            "tz_resolve",
            &[
                HostValue::Str("Europe/Paris".into()),
                HostValue::Int(summer + 7_200_000),
            ],
        )
        .unwrap();
        assert!(matches!(r, HostValue::Int(i) if i == summer));
        assert!(
            call(
                "tz_offset",
                &[HostValue::Str("Mars/Base".into()), HostValue::Int(0)]
            )
            .is_err()
        );
    }
}
