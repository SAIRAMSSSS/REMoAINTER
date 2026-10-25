#!/usr/bin/env bash
# REMoAINTER v0.2 Launcher
SOURCE="${BASH_SOURCE[0]}"
while [ -h "$SOURCE" ]; do
  DIR="$( cd -P "$( dirname "$SOURCE" )" >/dev/null 2>&1 && pwd )"
  SOURCE="$(readlink "$SOURCE")"
  [[ $SOURCE != /* ]] && SOURCE="$DIR/$SOURCE"
done
DIR="$( cd -P "$( dirname "$SOURCE" )" >/dev/null 2>&1 && pwd )"

# Ensure valid UTF-8 locale for xkbcommon
export LC_ALL="C.UTF-8"
export LANG="C.UTF-8"

if [ -x "$DIR/bin/remoainter" ]; then
  exec "$DIR/bin/remoainter" "$@"
elif [ -x "$DIR/remoainter" ]; then
  exec "$DIR/remoainter" "$@"
elif [ -x "$DIR/rust/target/release/remoainter" ]; then
  exec "$DIR/rust/target/release/remoainter" "$@"
elif [ -x "$DIR/rust/target/debug/remoainter" ]; then
  exec "$DIR/rust/target/debug/remoainter" "$@"
elif [ -f "$DIR/bin/remoainter" ]; then
  chmod +x "$DIR/bin/remoainter" && exec "$DIR/bin/remoainter" "$@"
elif [ -f "$DIR/remoainter" ]; then
  chmod +x "$DIR/remoainter" && exec "$DIR/remoainter" "$@"
elif [ -f "$DIR/rust/target/release/remoainter" ]; then
  chmod +x "$DIR/rust/target/release/remoainter" && exec "$DIR/rust/target/release/remoainter" "$@"
elif [ -f "$DIR/rust/target/debug/remoainter" ]; then
  chmod +x "$DIR/rust/target/debug/remoainter" && exec "$DIR/rust/target/debug/remoainter" "$@"
else
  echo "Error: remoainter binary not found. Run 'cargo build --release --manifest-path rust/Cargo.toml' first." >&2
  exit 1
fi
