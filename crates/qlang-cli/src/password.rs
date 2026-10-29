//! `Password.hash`, `Password.verify` and `Password.needs_rehash`: Argon2id with the salt and
//! the parameters inside the hash (the PHC string format), so a hash can be checked later with
//! whatever cost it was made with.

use argon2::password_hash::{
    Error as HashError, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};

/// Memory in KiB, passes and lanes: OWASP's minimum for Argon2id.
const MEMORY: u32 = 19_456;
const PASSES: u32 = 2;
const LANES: u32 = 1;

fn current() -> Argon2<'static> {
    let params = Params::new(MEMORY, PASSES, LANES, None).expect("fixed Argon2 parameters");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// A new hash of `plain`, with a fresh random salt.
pub fn hash(plain: &str) -> Result<String, String> {
    let mut raw = [0u8; 16];
    getrandom::fill(&mut raw).map_err(|e| format!("no source of randomness: {e}"))?;
    let salt = SaltString::encode_b64(&raw).map_err(|e| e.to_string())?;
    current()
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Whether `plain` is what `hash` was made from. `Err` when `hash` is not a hash at all.
pub fn verify(plain: &str, hash: &str) -> Result<bool, String> {
    let parsed = PasswordHash::new(hash).map_err(|_| "not a password hash".to_string())?;
    match Argon2::default().verify_password(plain.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(HashError::Password) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

/// Whether `hash` was made with other settings than today's (hash again after a login).
pub fn needs_rehash(hash: &str) -> Result<bool, String> {
    let parsed = PasswordHash::new(hash).map_err(|_| "not a password hash".to_string())?;
    let same = parsed.algorithm.as_str() == "argon2id"
        && Params::try_from(&parsed)
            .is_ok_and(|p| p.m_cost() == MEMORY && p.t_cost() == PASSES && p.p_cost() == LANES);
    Ok(!same)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_checks_the_password_it_came_from() {
        let h = hash("correct horse").unwrap();
        assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert_eq!(verify("correct horse", &h), Ok(true));
        assert_eq!(verify("wrong", &h), Ok(false));
        assert_ne!(
            h,
            hash("correct horse").unwrap(),
            "each hash has its own salt"
        );
        assert_eq!(needs_rehash(&h), Ok(false));
    }

    #[test]
    fn other_settings_or_junk_are_noticed() {
        assert!(verify("x", "not a hash").is_err());
        let weak =
            "$argon2id$v=19$m=4096,t=1,p=1$c29tZXNhbHRzb21l$RdescudvJCsgt3ub+b+dWRWJTmaaJObG";
        assert_eq!(needs_rehash(weak), Ok(true));
    }
}
