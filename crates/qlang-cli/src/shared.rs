//! `Shared`: whole numbers that every worker of the process sees and changes atomically, for
//! counters that must be exact without going through Redis. They live as long as the process.

use qlang_core::host::{HostError, HostValue};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const SHARED_EXTERNS: &[&str] = &["shared_add", "shared_get", "shared_set", "shared_cas"];

const MAX_NAMES: usize = 10_000;
const MAX_NAME_LEN: usize = 128;

fn table() -> &'static Mutex<HashMap<String, i64>> {
    static T: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

fn fail(message: impl Into<String>) -> HostError {
    HostError::Other(message.into())
}

fn check_name(name: &str, t: &HashMap<String, i64>) -> Result<(), HostError> {
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return Err(fail(format!(
            "a shared name is from 1 to {MAX_NAME_LEN} bytes"
        )));
    }
    if !t.contains_key(name) && t.len() >= MAX_NAMES {
        return Err(fail(format!("at most {MAX_NAMES} shared names")));
    }
    Ok(())
}

pub fn call(name: &str, args: &[HostValue]) -> Result<HostValue, HostError> {
    let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
    match (name, args) {
        ("shared_add", [HostValue::Str(key), HostValue::Int(delta)]) => {
            check_name(key, &t)?;
            let slot = t.entry(key.clone()).or_insert(0);
            *slot = slot
                .checked_add(*delta)
                .ok_or_else(|| fail("the shared number overflows"))?;
            Ok(HostValue::Int(*slot))
        }
        ("shared_get", [HostValue::Str(key)]) => {
            Ok(HostValue::Int(t.get(key).copied().unwrap_or(0)))
        }
        ("shared_set", [HostValue::Str(key), HostValue::Int(value)]) => {
            check_name(key, &t)?;
            t.insert(key.clone(), *value);
            Ok(HostValue::Int(*value))
        }
        // true when it held `expected` (a name never written holds 0) and now holds `new`
        (
            "shared_cas",
            [
                HostValue::Str(key),
                HostValue::Int(expected),
                HostValue::Int(new),
            ],
        ) => {
            check_name(key, &t)?;
            let slot = t.entry(key.clone()).or_insert(0);
            if *slot == *expected {
                *slot = *new;
                Ok(HostValue::Bool(true))
            } else {
                Ok(HostValue::Bool(false))
            }
        }
        _ => Err(fail(format!(
            "`{name}` was called with the wrong arguments"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> HostValue {
        HostValue::Str(x.to_string())
    }

    #[test]
    fn numbers_add_set_and_compare_exactly_across_threads() {
        let key = "test.counter";
        call("shared_set", &[s(key), HostValue::Int(0)]).unwrap();
        let threads: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(move || {
                    for _ in 0..1000 {
                        call("shared_add", &[s("test.counter"), HostValue::Int(1)]).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert!(matches!(
            call("shared_get", &[s(key)]).unwrap(),
            HostValue::Int(8000)
        ));
        assert!(matches!(
            call(
                "shared_cas",
                &[s(key), HostValue::Int(8000), HostValue::Int(1)]
            )
            .unwrap(),
            HostValue::Bool(true)
        ));
        assert!(matches!(
            call(
                "shared_cas",
                &[s(key), HostValue::Int(8000), HostValue::Int(2)]
            )
            .unwrap(),
            HostValue::Bool(false)
        ));
        assert!(matches!(
            call("shared_get", &[s("test.never")]).unwrap(),
            HostValue::Int(0)
        ));
        assert!(call("shared_add", &[s(""), HostValue::Int(1)]).is_err());
        assert!(call("shared_add", &[s(key), HostValue::Int(i64::MAX)]).is_err());
    }
}
