#!/bin/bash
cd /mnt/shared/REMoAINTER || exit 1
exec /mnt/shared/REMoAINTER/build/remoainter "$@"
