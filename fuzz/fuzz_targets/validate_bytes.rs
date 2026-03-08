#![no_main]

use libfuzzer_sys::fuzz_target;
use mp4lint::Validator;

fuzz_target!(|data: &[u8]| {
    let validator = Validator::new();
    // We don't care about the result - just that it doesn't panic
    let _ = validator.validate_bytes(data);
});
