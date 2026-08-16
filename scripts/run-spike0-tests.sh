#!/bin/sh
# Prefer the full packaged-gate runner.
exec sh "$(cd "$(dirname "$0")" && pwd)/validate-spike0.sh"
