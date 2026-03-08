# mp4lint

A Rust validator for ISOBMFF/MP4 files against the ISO 14496-12 specification.

`mp4lint` provides both a library API and a command-line tool for validating MP4 files.

## CLI Usage

```bash
# Validate an MP4 file
mp4lint video.mp4

# Output as JSON
mp4lint video.mp4 --format json
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | File is valid |
| 1 | Validation errors found |
| 2 | I/O or processing error |

## Library Usage

```rust
use mp4lint::{Validator, ValidationOptions};
use std::fs::File;
use std::io::BufReader;

let file = File::open("video.mp4").unwrap();
let reader = BufReader::new(file);

let validator = Validator::new();
let report = validator.validate(reader).unwrap();

if report.is_valid() {
    println!("File is valid!");
} else {
    for error in report.errors() {
        println!("{}", error);
    }
}
```
