fuser -k /dev/ttyACM0 2>/dev/null || true
TARGET=x86_64-unknown-linux-gnu
DEVICE=/dev/ttyACM0

if [[ "$OSTYPE" == "darwin"* ]]; then
  TARGET=aarch64-apple-darwin
  DEVICE=$(ls /dev/tty.usbmodem* 2>/dev/null | head -1)
  if [ -z "$DEVICE" ]; then
    DEVICE=$(ls /dev/cu.usbmodem* 2>/dev/null | head -1)
  fi
  if [ -z "$DEVICE" ]; then
    echo "No USB serial device found"
    exit 1
  fi
fi

cargo +nightly run -p siger-cli --target $TARGET -- $DEVICE
