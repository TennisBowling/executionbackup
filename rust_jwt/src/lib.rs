use hex;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

const DEFAULT_ALGORITHM: Algorithm = Algorithm::HS256;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Claims {
    /// issued-at claim. Represented as seconds passed since UNIX_EPOCH.
    iat: i64,
    /// Optional unique identifier for the CL node.
    id: String,
    /// Optional client version for the CL node.
    clv: String,
}

#[no_mangle]
pub extern "C" fn make_jwt(secretcstring: *const c_char, timestamp: &i64) -> *mut c_char {
    let secret = unsafe { CStr::from_ptr(secretcstring).to_string_lossy().into_owned() };

    let claim_inst = Claims {
        iat: timestamp.to_owned(),
        id: "1".to_owned(),
        clv: "1".to_owned(),
    };

    let decoded = &hex::decode(secret).unwrap();

    let key = &EncodingKey::from_secret(decoded);

    let header = Header::new(DEFAULT_ALGORITHM);
    let token = encode(&header, &claim_inst, &key).unwrap();
    CString::new(token).unwrap().into_raw()
}
