fuser -k /dev/ttyACM0 2>/dev/null || true
cargo +stable run -p siger-cli --target x86_64-unknown-linux-gnu -- /dev/ttyACM0
