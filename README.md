# FastDupFinder

Fast duplicate file finder using SHA-256 hashing.

## Features

- Two-pass detection: size pre-filter, then SHA-256 hash
- Configurable max/min file size
- Interactive or automatic deletion
- Dry-run mode
- Human-readable output

## Build & Run

```bash
cargo build --release

# Scan current directory
./target/release/fastdupfinder .

# Scan with size limits
./target/release/fastdupfinder /home/user -m 104857600 --min-size 1024

# Dry run delete
./target/release/fastdupfinder . --delete --dry-run

# Actually delete duplicates (keeps first copy)
./target/release/fastdupfinder . --delete
```

## License

MIT
