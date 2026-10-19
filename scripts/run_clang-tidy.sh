#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

git ls-files -z -- '*.cpp' '*.cc' '*.c' '*.h' '*.hpp' ':!:3rdparty/**' | xargs -0 -r clang-format -i
