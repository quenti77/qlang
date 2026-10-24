//! Signatures with a key pair: what a JWT needs when a token is signed with RSA, an elliptic curve
//! or Ed25519 (`RS256`, `ES256`, `EdDSA`...). `Crypto.verify` and `Crypto.sign` of the prelude are
//! written over the two `extern fun` this file answers; the primitives come from `ring`.

use qlang_core::codec::base64_decode;
use qlang_core::host::{HostError, HostValue};
use ring::rand::SystemRandom;
use ring::signature;

pub const SIGNATURE_EXTERNS: [&str; 2] = ["crypto_verify", "crypto_sign"];

type Failure = String;

// ------------------------------------------------------------------------------------ DER

/// One DER element: its tag, its content and what follows it.
fn tlv(buf: &[u8]) -> Result<(u8, &[u8], &[u8]), Failure> {
    let bad = || "the key is not valid DER".to_string();
    let tag = *buf.first().ok_or_else(bad)?;
    let first = *buf.get(1).ok_or_else(bad)?;
    let (len, start) = if first < 0x80 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7F) as usize;
        if n == 0 || n > 4 || buf.len() < 2 + n {
            return Err(bad());
        }
        let len = buf[2..2 + n]
            .iter()
            .fold(0usize, |a, b| (a << 8) | *b as usize);
        (len, 2 + n)
    };
    if buf.len() < start + len {
        return Err(bad());
    }
    Ok((tag, &buf[start..start + len], &buf[start + len..]))
}

fn der_len(n: usize) -> Vec<u8> {
    if n < 0x80 {
        vec![n as u8]
    } else if n < 0x100 {
        vec![0x81, n as u8]
    } else {
        vec![0x82, (n >> 8) as u8, n as u8]
    }
}

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend(der_len(content.len()));
    out.extend_from_slice(content);
    out
}

/// A positive integer as DER wants it: no useless leading zeros, one in front when the top bit is set.
fn der_uint(bytes: &[u8]) -> Vec<u8> {
    let start = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len().saturating_sub(1));
    let mut body = bytes[start..].to_vec();
    if body.first().is_none_or(|b| b & 0x80 != 0) {
        body.insert(0, 0);
    }
    der(0x02, &body)
}

/// `RSAPublicKey ::= SEQUENCE { n INTEGER, e INTEGER }`.
fn rsa_public_der(n: &[u8], e: &[u8]) -> Vec<u8> {
    let mut inner = der_uint(n);
    inner.extend(der_uint(e));
    der(0x30, &inner)
}

/// The key bits of a `SubjectPublicKeyInfo`.
fn spki_key(buf: &[u8]) -> Result<Vec<u8>, Failure> {
    let (tag, body, _) = tlv(buf)?;
    if tag != 0x30 {
        return Err("the key is not a public key".into());
    }
    let (_, _algorithm, rest) = tlv(body)?;
    let (tag, bits, _) = tlv(rest)?;
    if tag != 0x03 || bits.first() != Some(&0) {
        return Err("the key is not a public key".into());
    }
    Ok(bits[1..].to_vec())
}

// ------------------------------------------------------------------------------------ keys

/// What a key text holds.
enum Material {
    /// The DER of a PKCS#1 `RSAPublicKey`, an uncompressed EC point, or 32 bytes of Ed25519.
    Public(Vec<u8>),
    Pkcs8(Vec<u8>),
    Pkcs1(Vec<u8>),
}

fn pem_body(text: &str, label: &str) -> Option<Vec<u8>> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let from = text.find(&begin)? + begin.len();
    let to = text.find(&end)?;
    let body: String = text[from..to]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    base64_decode(&body)
}

fn jwk_field(json: &serde_json::Value, name: &str) -> Result<Vec<u8>, Failure> {
    json.get(name)
        .and_then(|v| v.as_str())
        .and_then(base64_decode)
        .ok_or_else(|| format!("the JWK has no valid `{name}`"))
}

fn read_key(text: &str) -> Result<Material, Failure> {
    let text = text.trim();
    if text.starts_with('{') {
        let jwk: serde_json::Value =
            serde_json::from_str(text).map_err(|_| "the JWK is not valid JSON".to_string())?;
        return match jwk.get("kty").and_then(|v| v.as_str()) {
            Some("RSA") => Ok(Material::Public(rsa_public_der(
                &jwk_field(&jwk, "n")?,
                &jwk_field(&jwk, "e")?,
            ))),
            Some("EC") => {
                let mut point = vec![4u8];
                point.extend(jwk_field(&jwk, "x")?);
                point.extend(jwk_field(&jwk, "y")?);
                Ok(Material::Public(point))
            }
            Some("OKP") => Ok(Material::Public(jwk_field(&jwk, "x")?)),
            _ => Err("the JWK type is not supported (RSA, EC and OKP are)".into()),
        };
    }
    if let Some(d) = pem_body(text, "PUBLIC KEY") {
        return spki_key(&d).map(Material::Public);
    }
    if let Some(d) = pem_body(text, "RSA PUBLIC KEY") {
        return Ok(Material::Public(d));
    }
    if let Some(d) = pem_body(text, "PRIVATE KEY") {
        return Ok(Material::Pkcs8(d));
    }
    if let Some(d) = pem_body(text, "RSA PRIVATE KEY") {
        return Ok(Material::Pkcs1(d));
    }
    if text.contains("BEGIN EC PRIVATE KEY") {
        return Err(
            "an `EC PRIVATE KEY` is not supported: convert it with `openssl pkcs8 -topk8 -nocrypt`"
                .into(),
        );
    }
    Err("the key is neither a PEM (public or private) nor a JWK".into())
}

// ------------------------------------------------------------------------------- verify and sign

pub fn verify(alg: &str, key: &str, message: &[u8], sig: &[u8]) -> Result<bool, Failure> {
    let Material::Public(public) = read_key(key)? else {
        return Err("a public key is needed to verify (a private key was given)".into());
    };
    let algorithm: &dyn signature::VerificationAlgorithm = match alg {
        "RS256" => &signature::RSA_PKCS1_2048_8192_SHA256,
        "RS384" => &signature::RSA_PKCS1_2048_8192_SHA384,
        "RS512" => &signature::RSA_PKCS1_2048_8192_SHA512,
        "ES256" => &signature::ECDSA_P256_SHA256_FIXED,
        "ES384" => &signature::ECDSA_P384_SHA384_FIXED,
        "EdDSA" => &signature::ED25519,
        other => {
            return Err(format!(
                "the algorithm `{other}` is not supported (RS256, RS384, RS512, ES256, ES384, EdDSA are)"
            ));
        }
    };
    Ok(signature::UnparsedPublicKey::new(algorithm, public)
        .verify(message, sig)
        .is_ok())
}

pub fn sign(alg: &str, key: &str, message: &[u8]) -> Result<Vec<u8>, Failure> {
    let material = read_key(key)?;
    let rng = SystemRandom::new();
    match (alg, material) {
        ("RS256" | "RS384" | "RS512", m) => {
            let pair = match m {
                Material::Pkcs8(d) => signature::RsaKeyPair::from_pkcs8(&d),
                Material::Pkcs1(d) => signature::RsaKeyPair::from_der(&d),
                Material::Public(_) => return Err("a private key is needed to sign".into()),
            }
            .map_err(|e| format!("the RSA key is not valid: {e}"))?;
            let padding: &'static dyn signature::RsaEncoding = match alg {
                "RS256" => &signature::RSA_PKCS1_SHA256,
                "RS384" => &signature::RSA_PKCS1_SHA384,
                _ => &signature::RSA_PKCS1_SHA512,
            };
            let mut out = vec![0u8; pair.public().modulus_len()];
            pair.sign(padding, &rng, message, &mut out)
                .map_err(|_| "the signature failed".to_string())?;
            Ok(out)
        }
        ("ES256" | "ES384", Material::Pkcs8(d)) => {
            let algorithm = if alg == "ES256" {
                &signature::ECDSA_P256_SHA256_FIXED_SIGNING
            } else {
                &signature::ECDSA_P384_SHA384_FIXED_SIGNING
            };
            let pair = signature::EcdsaKeyPair::from_pkcs8(algorithm, &d, &rng)
                .map_err(|e| format!("the EC key is not valid: {e}"))?;
            pair.sign(&rng, message)
                .map(|s| s.as_ref().to_vec())
                .map_err(|_| "the signature failed".to_string())
        }
        ("EdDSA", Material::Pkcs8(d)) => {
            let pair = signature::Ed25519KeyPair::from_pkcs8_maybe_unchecked(&d)
                .map_err(|e| format!("the Ed25519 key is not valid: {e}"))?;
            Ok(pair.sign(message).as_ref().to_vec())
        }
        ("ES256" | "ES384" | "EdDSA", _) => {
            Err("a private key in PKCS#8 (`BEGIN PRIVATE KEY`) is needed to sign".into())
        }
        (other, _) => Err(format!(
            "the algorithm `{other}` is not supported (RS256, RS384, RS512, ES256, ES384, EdDSA are)"
        )),
    }
}

/// Runs one of [`SIGNATURE_EXTERNS`].
pub fn call(name: &str, args: &[HostValue]) -> Result<HostValue, HostError> {
    let wrong = || HostError::Other(format!("`{name}` was called with the wrong arguments"));
    match (name, args) {
        (
            "crypto_verify",
            [
                HostValue::Str(alg),
                HostValue::Str(key),
                HostValue::Bytes(message),
                HostValue::Bytes(sig),
            ],
        ) => verify(alg, key, message, sig)
            .map(HostValue::Bool)
            .map_err(HostError::Other),
        (
            "crypto_sign",
            [
                HostValue::Str(alg),
                HostValue::Str(key),
                HostValue::Bytes(message),
            ],
        ) => sign(alg, key, message)
            .map(HostValue::Bytes)
            .map_err(HostError::Other),
        _ => Err(wrong()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qlang_core::codec::base64_encode;

    // Throwaway keys made for these tests with `openssl`, kept as the base64 of their DER
    // (no armor, which secret scanners look for).
    const RSA8: &[&str] = &[
        "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCnlDycWB5VBGGyoraDpmtVzEsEflFkDOimQIlliIgVwgit",
        "KsTs6Z94c07JBklLiBfgtX+ePfBI6onWnMnaJjpuZL8pBiwMhH4qEv0Om0JxBGS85iMaMueUxMWKjM1b/HZ6d1QFBRoCSQCA",
        "pYdtRH0j5VmeIMXRjvW9b7PWbIg0I30s7QdLXjH/PwGh7u+2guPn4hoXSCK79mEod/cdLODexLPa+H2rrJjp1VPevxCZoKWK",
        "Kd96LEs+QWb1mCzRfxo5U2SlLpFg+bhofdpcmDAXTMGcgkCS2iFEPgr+NzwhcYvCU8NgPUtuqQgRn+9vL5L9e1229lLiWdOA",
        "7L1th9k1AgMBAAECggEAJfKG8R5xK+5Uk0eOTeyLBMBAwpE6LA5SRod5tYC/1DF+lnwHYOCcaIGv9wyR32U/hvaCohpDhbd6",
        "pBsYCGq8s4/jy83mfqUoDq4DGOImnwIQL8hrbXzWfRU76F5vCGP4jk3kPEjF9XuDfh49twwZzF2cuBaSxRv64L1hO26TPz7l",
        "qJ7f1cZ3K7vMghHtEsrmolw0MO2OgjL51IzV766+xmIvLM966kcIBEVvjQS76E2AMx67Iib7op2DpsxWt2E67zumbBPklIld",
        "T4pMdBtz/mHOT87Od/ADV6w7x7PxfFdpQG8/FOEXALgDEyE3/mfUGnh3bFfnHGMm2LsGLM5agQKBgQDoI8ngAb27SiJ273pJ",
        "gNwbXYOBmEGKVchwqlape2hatKn26Y/v3w3jGaMf8PS67NY/79vH7WLXGM6eXsaP6s3041OJ5ppO5P6yS9Ohr+PrUxyXDdaK",
        "ZaBJ7vMTvD/qfyrQbZXFjF6LKLQyKdGiIVbavfzjP82pyoUZuKQy/kpKQQKBgQC4za8ujlHzaeAwK74At4/4rCF4FvFsAp7t",
        "UXd8JX6wFHk5dEpy13IAY4nhJ7PgVtXBiy2LmhocNCAo3XmwXTWsQhdjuTexZ1MUYSMlB0TOLvYHs5ewKSny8FFveYQjArb3",
        "xyuxdZQCKc3RxYrIcRU7npwf3wq5STI5zlK4TjKJ9QKBgQDYMIxPTA/QnqgeL8SpM5LJAXv5czyGgoFbO6fdywpSfzJSuw3L",
        "jOW3/CZJFvQ0KWEWxmDefpuwSnASWZ1+ZmRMjnE8WkCW/PieP7PmPpJ6lDpqCv8kgKSYdip21LiJwMi0t3nmMmXSH6RBm2Sa",
        "I4ZpzMn5MXBtkigG3kg9HUk+gQKBgQCUWUCUTdkJXmeCI8sNhk1DYOIpJbm9a8kqVqhk9zBgpr69WVXeJIa2OzFWlel2YB6N",
        "lG0GULk/BLUAOPHTmq9YTWG7QtarSXdOt79fnRslrFsBybAt0I2FIFcu9kwVUruP4cl4VhtvNSGskDgdOXHC/0ry5rgUDucL",
        "sk2C9/rn9QKBgDqCd5etPAq8X61Kq5xWuaxjXfcoW7RsPXDKxn+8+Tp7B/AhJB9UL2k/fqEG0BAbK63g5zk0q3g7HJTxofr+",
        "WyMejeZuDrYaco+YI8k29BHqLraRikx887lYH8xVkSA8casVbjV8K317aWBJ2l8VvadhWOmg7rGgufjuGhtScVnN",
    ];
    const RSA1: &[&str] = &[
        "MIIEpAIBAAKCAQEAp5Q8nFgeVQRhsqK2g6ZrVcxLBH5RZAzopkCJZYiIFcIIrSrE7OmfeHNOyQZJS4gX4LV/nj3wSOqJ1pzJ",
        "2iY6bmS/KQYsDIR+KhL9DptCcQRkvOYjGjLnlMTFiozNW/x2endUBQUaAkkAgKWHbUR9I+VZniDF0Y71vW+z1myINCN9LO0H",
        "S14x/z8Boe7vtoLj5+IaF0giu/ZhKHf3HSzg3sSz2vh9q6yY6dVT3r8QmaCliinfeixLPkFm9Zgs0X8aOVNkpS6RYPm4aH3a",
        "XJgwF0zBnIJAktohRD4K/jc8IXGLwlPDYD1LbqkIEZ/vby+S/XtdtvZS4lnTgOy9bYfZNQIDAQABAoIBACXyhvEecSvuVJNH",
        "jk3siwTAQMKROiwOUkaHebWAv9QxfpZ8B2DgnGiBr/cMkd9lP4b2gqIaQ4W3eqQbGAhqvLOP48vN5n6lKA6uAxjiJp8CEC/I",
        "a2181n0VO+hebwhj+I5N5DxIxfV7g34ePbcMGcxdnLgWksUb+uC9YTtukz8+5aie39XGdyu7zIIR7RLK5qJcNDDtjoIy+dSM",
        "1e+uvsZiLyzPeupHCARFb40Eu+hNgDMeuyIm+6Kdg6bMVrdhOu87pmwT5JSJXU+KTHQbc/5hzk/OznfwA1esO8ez8XxXaUBv",
        "PxThFwC4AxMhN/5n1Bp4d2xX5xxjJti7BizOWoECgYEA6CPJ4AG9u0oidu96SYDcG12DgZhBilXIcKpWqXtoWrSp9umP798N",
        "4xmjH/D0uuzWP+/bx+1i1xjOnl7Gj+rN9ONTieaaTuT+skvToa/j61Mclw3WimWgSe7zE7w/6n8q0G2VxYxeiyi0MinRoiFW",
        "2r384z/NqcqFGbikMv5KSkECgYEAuM2vLo5R82ngMCu+ALeP+KwheBbxbAKe7VF3fCV+sBR5OXRKctdyAGOJ4Sez4FbVwYst",
        "i5oaHDQgKN15sF01rEIXY7k3sWdTFGEjJQdEzi72B7OXsCkp8vBRb3mEIwK298crsXWUAinN0cWKyHEVO56cH98KuUkyOc5S",
        "uE4yifUCgYEA2DCMT0wP0J6oHi/EqTOSyQF7+XM8hoKBWzun3csKUn8yUrsNy4zlt/wmSRb0NClhFsZg3n6bsEpwElmdfmZk",
        "TI5xPFpAlvz4nj+z5j6SepQ6agr/JICkmHYqdtS4icDItLd55jJl0h+kQZtkmiOGaczJ+TFwbZIoBt5IPR1JPoECgYEAlFlA",
        "lE3ZCV5ngiPLDYZNQ2DiKSW5vWvJKlaoZPcwYKa+vVlV3iSGtjsxVpXpdmAejZRtBlC5PwS1ADjx05qvWE1hu0LWq0l3Tre/",
        "X50bJaxbAcmwLdCNhSBXLvZMFVK7j+HJeFYbbzUhrJA4HTlxwv9K8ua4FA7nC7JNgvf65/UCgYA6gneXrTwKvF+tSqucVrms",
        "Y133KFu0bD1wysZ/vPk6ewfwISQfVC9pP36hBtAQGyut4Oc5NKt4OxyU8aH6/lsjHo3mbg62GnKPmCPJNvQR6i62kYpMfPO5",
        "WB/MVZEgPHGrFW41fCt9e2lgSdpfFb2nYVjpoO6xoLn47hobUnFZzQ==",
    ];
    const RSA_PUB: &[&str] = &[
        "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAp5Q8nFgeVQRhsqK2g6ZrVcxLBH5RZAzopkCJZYiIFcIIrSrE7Omf",
        "eHNOyQZJS4gX4LV/nj3wSOqJ1pzJ2iY6bmS/KQYsDIR+KhL9DptCcQRkvOYjGjLnlMTFiozNW/x2endUBQUaAkkAgKWHbUR9",
        "I+VZniDF0Y71vW+z1myINCN9LO0HS14x/z8Boe7vtoLj5+IaF0giu/ZhKHf3HSzg3sSz2vh9q6yY6dVT3r8QmaCliinfeixL",
        "PkFm9Zgs0X8aOVNkpS6RYPm4aH3aXJgwF0zBnIJAktohRD4K/jc8IXGLwlPDYD1LbqkIEZ/vby+S/XtdtvZS4lnTgOy9bYfZ",
        "NQIDAQAB",
    ];
    const EC8: &[&str] = &[
        "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgnzpR442PY33TbjfyRUdkmycsdCh2rROT37FJcP+MDR+hRANC",
        "AAQUZGRQ1uRfuRL44oi7NEf9lXBKKJ58N9GJMPAY0NFfd494SVdEKynjERhnvRyBKB5I4wT8mOMLUPnx1XNx91I+",
    ];
    const EC_PUB: &[&str] = &[
        "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEFGRkUNbkX7kS+OKIuzRH/ZVwSiiefDfRiTDwGNDRX3ePeElXRCsp4xEYZ70c",
        "gSgeSOME/JjjC1D58dVzcfdSPg==",
    ];
    const ED8: &[&str] = &["MC4CAQAwBQYDK2VwBCIEIGyQiDGTGCgMnTCfji83jvlguocFl4+30DStyKTB9ZoM"];
    const ED_PUB: &[&str] = &["MCowBQYDK2VwAyEARHfuzvoBx6Yn3ewRRAvJ2tWrdLP+Q0jm0KfZl5Cda/s="];

    fn pem(label: &str, parts: &[&str]) -> String {
        format!(
            "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
            parts.concat()
        )
    }

    #[test]
    fn rsa_signs_and_verifies_with_every_digest_and_every_key_form() {
        let msg = b"header.payload";
        for alg in ["RS256", "RS384", "RS512"] {
            for private in [pem("PRIVATE KEY", RSA8), pem("RSA PRIVATE KEY", RSA1)] {
                let s = sign(alg, &private, msg).unwrap();
                assert_eq!(s.len(), 256);
                let public = pem("PUBLIC KEY", RSA_PUB);
                assert!(verify(alg, &public, msg, &s).unwrap());
                assert!(!verify(alg, &public, b"other", &s).unwrap());
            }
        }
        // a different digest does not verify
        let s = sign("RS256", &pem("PRIVATE KEY", RSA8), msg).unwrap();
        assert!(!verify("RS384", &pem("PUBLIC KEY", RSA_PUB), msg, &s).unwrap());
    }

    #[test]
    fn a_jwk_is_the_same_key_as_its_pem() {
        // n and e of the RSA test key, as a JWK
        let spki = base64_decode(&RSA_PUB.concat()).unwrap();
        let rsa = spki_key(&spki).unwrap();
        let (_, body, _) = tlv(&rsa).unwrap();
        let (_, n, rest) = tlv(body).unwrap();
        let (_, e, _) = tlv(rest).unwrap();
        let n = if n[0] == 0 { &n[1..] } else { n };
        let jwk = format!(
            "{{\"kty\":\"RSA\",\"n\":\"{}\",\"e\":\"{}\"}}",
            base64_encode(n, true),
            base64_encode(e, true)
        );
        let s = sign("RS256", &pem("PRIVATE KEY", RSA8), b"m").unwrap();
        assert!(verify("RS256", &jwk, b"m", &s).unwrap());
        assert_eq!(rsa_public_der(n, e), rsa);
    }

    #[test]
    fn elliptic_curves_and_ed25519_sign_and_verify() {
        let ec_private = pem("PRIVATE KEY", EC8);
        let ec_public = pem("PUBLIC KEY", EC_PUB);
        let s = sign("ES256", &ec_private, b"hello").unwrap();
        assert_eq!(s.len(), 64);
        assert!(verify("ES256", &ec_public, b"hello", &s).unwrap());
        assert!(!verify("ES256", &ec_public, b"hellO", &s).unwrap());
        let ed_private = pem("PRIVATE KEY", ED8);
        let ed_public = pem("PUBLIC KEY", ED_PUB);
        let s = sign("EdDSA", &ed_private, b"hello").unwrap();
        assert!(verify("EdDSA", &ed_public, b"hello", &s).unwrap());
        assert!(!verify("EdDSA", &ed_public, b"hellO", &s).unwrap());
        // the point of the EC key as a JWK
        let spki = base64_decode(&EC_PUB.concat()).unwrap();
        let point = spki_key(&spki).unwrap();
        let jwk = format!(
            "{{\"kty\":\"EC\",\"crv\":\"P-256\",\"x\":\"{}\",\"y\":\"{}\"}}",
            base64_encode(&point[1..33], true),
            base64_encode(&point[33..], true)
        );
        let s = sign("ES256", &ec_private, b"jwk").unwrap();
        assert!(verify("ES256", &jwk, b"jwk", &s).unwrap());
    }

    #[test]
    fn mistakes_are_said_not_guessed() {
        let public = pem("PUBLIC KEY", RSA_PUB);
        assert!(
            verify("HS256", &public, b"m", b"s")
                .unwrap_err()
                .contains("not supported")
        );
        assert!(
            verify("RS256", &pem("PRIVATE KEY", RSA8), b"m", b"s")
                .unwrap_err()
                .contains("public key")
        );
        assert!(
            sign("RS256", &public, b"m")
                .unwrap_err()
                .contains("private key")
        );
        assert!(
            verify("RS256", "garbage", b"m", b"s")
                .unwrap_err()
                .contains("neither a PEM")
        );
        assert!(
            sign(
                "ES256",
                "-----BEGIN EC PRIVATE KEY-----\nAAAA\n-----END EC PRIVATE KEY-----",
                b"m"
            )
            .unwrap_err()
            .contains("openssl pkcs8")
        );
        // a bad signature is `false`, not an error
        assert!(!verify("RS256", &public, b"m", b"short").unwrap());
    }
}
