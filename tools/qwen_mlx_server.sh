#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
QWEN_BIN="${PARROT_QWEN_BIN:-${AUDIO_TO_TEXT_QWEN_BIN:-$ROOT_DIR/.qwen-mlx/venv/bin/mlx-qwen3-asr}}"
MODEL="${1:-Qwen/Qwen3-ASR-0.6B}"
HOST="${MLX_ASR_HOST:-127.0.0.1}"
PORT="${MLX_ASR_PORT:-8765}"
API_KEY="${PARROT_QWEN_API_KEY:-${AUDIO_TO_TEXT_QWEN_API_KEY:-local-qwen-dev}}"

if [[ ! -x "$QWEN_BIN" ]]; then
  echo "Qwen CLI not found: $QWEN_BIN"
  echo "Run: tools/setup_qwen_mlx.sh"
  exit 1
fi

export HF_HOME="$ROOT_DIR/.qwen-mlx/cache"
export HF_HUB_DISABLE_TELEMETRY=1
export HF_HUB_DISABLE_XET=1

echo "Starting Qwen MLX server"
echo "Model: $MODEL"
echo "URL: http://$HOST:$PORT"
echo "API key: $API_KEY"

exec "$QWEN_BIN" serve \
  --host "$HOST" \
  --port "$PORT" \
  --api-key "$API_KEY" \
  --model "$MODEL"
