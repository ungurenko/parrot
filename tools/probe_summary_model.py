#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

MODELS = {
    "qwen3-4b": {
        "repo": "mlx-community/Qwen3-4B-Instruct-2507-4bit",
        "runtime": "mlx_lm",
    },
    "gemma4-e2b": {
        "repo": "mlx-community/gemma-4-e2b-it-4bit",
        "runtime": "mlx_vlm",
    },
}

SYSTEM_PROMPT = "Ты создаешь краткий структурированный конспект на русском языке."


def python_bin() -> Path:
    env = os.environ.get("PARROT_QWEN_PYTHON")
    if env:
        return Path(env)
    return Path(".qwen-mlx/venv/bin/python")


def cache_dir() -> Path:
    return Path(os.environ.get("HF_HOME", ".qwen-mlx/cache"))


def build_command(model_id: str, transcript: str) -> list[str]:
    spec = MODELS[model_id]
    prompt = (
        "Сделай краткий конспект в Markdown: краткое резюме, темы, тезисы, действия.\n\n"
        f"Транскрипт:\n---\n{transcript}\n---"
    )

    if spec["runtime"] == "mlx_vlm":
        return [
            str(python_bin()),
            "-m",
            "mlx_vlm",
            "generate",
            "--model",
            spec["repo"],
            "--system",
            SYSTEM_PROMPT,
            "--prompt",
            prompt,
            "--max-tokens",
            "700",
            "--temperature",
            "0.3",
            "--verbose",
        ]

    return [
        str(python_bin()),
        "-m",
        "mlx_lm",
        "generate",
        "--model",
        spec["repo"],
        "--system-prompt",
        SYSTEM_PROMPT,
        "--prompt",
        prompt,
        "--max-tokens",
        "700",
        "--temp",
        "0.3",
        "--top-p",
        "0.9",
        "--verbose",
        "False",
    ]


def run_model(model_id: str, transcript: str) -> dict:
    env = os.environ.copy()
    env.setdefault("HF_HOME", str(cache_dir()))
    env.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
    env.setdefault("HF_HUB_DISABLE_XET", "1")

    started = time.perf_counter()
    proc = subprocess.run(
        build_command(model_id, transcript),
        text=True,
        capture_output=True,
        env=env,
    )
    elapsed = time.perf_counter() - started
    return {
        "model": model_id,
        "ok": proc.returncode == 0,
        "seconds": round(elapsed, 2),
        "stdout_chars": len(proc.stdout),
        "stderr_tail": "\n".join(proc.stderr.splitlines()[-10:]),
        "preview": proc.stdout.strip()[:700],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", choices=MODELS.keys(), required=True)
    parser.add_argument("--transcript", type=Path, required=True)
    args = parser.parse_args()

    transcript = args.transcript.read_text(encoding="utf-8")
    result = run_model(args.model, transcript)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
