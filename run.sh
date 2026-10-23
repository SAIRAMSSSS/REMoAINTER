#!/usr/bin/env bash
# REMoAINTER v2.0.0 Launcher
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

exec "$DIR/rust/target/release/remoainter" "$@"
