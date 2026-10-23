//! `bytes`: methods, the `bytes.*` functions and the byte variants of `Base64`, `Hex`, `Url` and
//! `Hash`.

use super::*;
use crate::codec;

impl Interp<'_> {
    pub(super) fn new_bytes(&self, data: Vec<u8>, span: Span) -> R<Value> {
        self.guard_alloc(data.len(), span)?;
        Ok(Value::Bytes(Rc::from(data)))
    }

    /// The methods of a `bytes`; `None` for a name it does not have.
    #[inline(never)]
    pub(super) fn bytes_method(
        &mut self,
        b: &Rc<[u8]>,
        name: &str,
        args: &[Value],
        span: Span,
    ) -> R<Option<Value>> {
        self.charge_bytes(b.len(), span)?;
        let int_arg = |i: usize| match args.get(i) {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        let bytes_arg = |i: usize| -> Rc<[u8]> {
            match args.get(i) {
                Some(Value::Bytes(x)) => x.clone(),
                _ => Rc::from(Vec::new()),
            }
        };
        Ok(Some(match name {
            "len" => Value::Int(b.len() as i64),
            "is_empty" => Value::Bool(b.is_empty()),
            "at" => {
                let i = int_arg(0);
                if i >= 0 && (i as usize) < b.len() {
                    Value::Int(i64::from(b[i as usize]))
                } else {
                    Value::None
                }
            }
            "slice" => {
                let n = b.len() as i64;
                let start = int_arg(0).clamp(0, n) as usize;
                let end = int_arg(1).clamp(0, n) as usize;
                let part = if start < end { &b[start..end] } else { &[][..] };
                self.new_bytes(part.to_vec(), span)?
            }
            "concat" => {
                let other = bytes_arg(0);
                self.guard_alloc(b.len() + other.len(), span)?;
                let mut out = Vec::with_capacity(b.len() + other.len());
                out.extend_from_slice(b);
                out.extend_from_slice(&other);
                Value::Bytes(Rc::from(out))
            }
            "to_string" => String::from_utf8(b.to_vec()).map_or(Value::None, |s| Value::str(&s)),
            "to_array" => {
                self.guard_alloc(b.len(), span)?;
                Value::array(b.iter().map(|x| Value::Int(i64::from(*x))).collect())
            }
            "hex" => Value::str(&codec::hex_encode(b)),
            "base64" => Value::str(&codec::base64_encode(b, false)),
            "find" => {
                let needle = bytes_arg(0);
                find_bytes(b, &needle).map_or(Value::None, |i| Value::Int(i as i64))
            }
            "contains" => Value::Bool(find_bytes(b, &bytes_arg(0)).is_some()),
            "starts_with" => Value::Bool(b.starts_with(&bytes_arg(0))),
            "ends_with" => Value::Bool(b.ends_with(&bytes_arg(0))),
            _ => return Ok(None),
        }))
    }

    /// `bytes.empty()` and `bytes.from_array(xs)`.
    pub(super) fn bytes_static(&mut self, name: &str, args: &[Value], span: Span) -> R<Value> {
        match (name, args.first()) {
            ("from_array", Some(Value::Array(xs))) => {
                let xs = xs.borrow();
                self.charge(xs.len(), span)?;
                let mut out = Vec::with_capacity(xs.len());
                for x in xs.iter() {
                    match x {
                        Value::Int(n) if (0..=255).contains(n) => out.push(*n as u8),
                        _ => return Ok(Value::None),
                    }
                }
                drop(xs);
                self.new_bytes(out, span)
            }
            _ => Ok(Value::Bytes(Rc::from(Vec::new()))),
        }
    }

    /// The `*_bytes` and `*_raw` functions of `Base64`, `Hex`, `Url` and `Hash`.
    pub(super) fn codec_bytes_call(
        &mut self,
        ty: &str,
        name: &str,
        args: &[Value],
        span: Span,
    ) -> R<Option<Value>> {
        let data: Rc<[u8]> = match args.first() {
            Some(Value::Bytes(b)) => b.clone(),
            Some(Value::Str(s)) => Rc::from(s.as_bytes()),
            _ => Rc::from(Vec::new()),
        };
        self.charge_bytes(data.len(), span)?;
        let text = |v: Option<&Value>| match v {
            Some(Value::Str(s)) => s.to_string(),
            _ => String::new(),
        };
        let out: Value = match (ty, name) {
            ("Base64", "encode_bytes") => Value::str(&codec::base64_encode(&data, false)),
            ("Base64", "encode_url_bytes") => Value::str(&codec::base64_encode(&data, true)),
            ("Hex", "encode_bytes") => Value::str(&codec::hex_encode(&data)),
            ("Url", "encode_bytes") => Value::str(&codec::url_encode(&data)),
            ("Base64", "decode_bytes") => match codec::base64_decode(&text(args.first())) {
                Some(b) => self.new_bytes(b, span)?,
                None => Value::None,
            },
            ("Hex", "decode_bytes") => match codec::hex_decode(&text(args.first())) {
                Some(b) => self.new_bytes(b, span)?,
                None => Value::None,
            },
            ("Url", "decode_bytes") => match codec::url_decode(&text(args.first())) {
                Some(b) => self.new_bytes(b, span)?,
                None => Value::None,
            },
            ("Hash", "sha256_bytes") => Value::str(&codec::hex_encode(&codec::sha256(&data))),
            ("Hash", "sha1_bytes") => Value::str(&codec::hex_encode(&codec::sha1(&data))),
            ("Hash", "md5_bytes") => Value::str(&codec::hex_encode(&codec::md5(&data))),
            ("Hash", "sha256_raw") => self.new_bytes(codec::sha256(&data).to_vec(), span)?,
            ("Hash", "hmac_sha256_bytes" | "hmac_sha256_raw") => {
                let message: Rc<[u8]> = match args.get(1) {
                    Some(Value::Bytes(b)) => b.clone(),
                    Some(Value::Str(s)) => Rc::from(s.as_bytes()),
                    _ => Rc::from(Vec::new()),
                };
                self.charge_bytes(message.len(), span)?;
                let mac = codec::hmac_sha256(&data, &message);
                if name == "hmac_sha256_raw" {
                    self.new_bytes(mac.to_vec(), span)?
                } else {
                    Value::str(&codec::hex_encode(&mac))
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }
}

/// The position of the first `needle` in `hay`.
pub(super) fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    hay.windows(needle.len()).position(|w| w == needle)
}
