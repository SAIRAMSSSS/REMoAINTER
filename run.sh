#!/usr/bin/env bash
# REMoAINTER v2.0.0 Launcher
SOURCE="${BASH_SOURCE[0]}"
while [ -h "$SOURCE" ]; do
  DIR="$( cd -P "$( dirname "$SOURCE" )" >/dev/null 2>&1 && pwd )"
  SOURCE="$(readlink "$SOURCE")"
  [[ $SOURCE != /* ]] && SOURCE="$DIR/$SOURCE"
done
DIR="$( cd -P "$( dirname "$SOURCE" )" >/dev/null 2>&1 && pwd )"

if [[ "$1" == "--vulkan" || "$1" == "--cpp" ]]; then
  shift
  exec "$DIR/build/remoainter" "$@"
else
  exec "$DIR/rust/target/release/remoainter" "$@"
fi
