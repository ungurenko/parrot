#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON_BIN="${PYTHON_BIN:-/opt/homebrew/bin/python3.12}"
VENV_DIR="$ROOT_DIR/.qwen-mlx/venv"

if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "Python 3.12 not found at $PYTHON_BIN"
  echo "Install it with: brew install python@3.12"
  exit 1
fi

"$PYTHON_BIN" -m venv "$VENV_DIR"
"$VENV_DIR/bin/python" -m pip install --upgrade pip
"$VENV_DIR/bin/python" -m pip install "mlx-qwen3-asr[serve]"

HF_HOME="$ROOT_DIR/.qwen-mlx/cache" \
HF_HUB_DISABLE_XET=1 \
"$VENV_DIR/bin/mlx-qwen3-asr" --doctor

echo
echo "Qwen MLX environment is ready:"
echo "$VENV_DIR/bin/mlx-qwen3-asr"
