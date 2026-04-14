#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_QWEN_BIN = ROOT / ".qwen-mlx" / "venv" / "bin" / "mlx-qwen3-asr"
DEFAULT_CACHE = ROOT / ".qwen-mlx" / "cache"
DEFAULT_RESULTS = ROOT / "qwen-results"

MODELS = {
    "0.6b": "Qwen/Qwen3-ASR-0.6B",
    "1.7b": "Qwen/Qwen3-ASR-1.7B",
}


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run a local Qwen3-ASR MLX transcription test."
    )
    parser.add_argument("input", help="Local audio/video file or YouTube URL")
    parser.add_argument(
        "--model",
        default="0.6b",
        choices=sorted(MODELS),
        help="Qwen model to test",
    )
    parser.add_argument("--language", help="Optional forced language, e.g. Russian")
    parser.add_argument("--qwen-bin", default=str(DEFAULT_QWEN_BIN))
    parser.add_argument("--cache-dir", default=str(DEFAULT_CACHE))
    parser.add_argument("--results-dir", default=str(DEFAULT_RESULTS))
    parser.add_argument("--keep-wav", action="store_true")
    args = parser.parse_args()

    qwen_bin = Path(args.qwen_bin)
    if not qwen_bin.exists():
        print(f"Qwen CLI not found: {qwen_bin}", file=sys.stderr)
        print("Run: tools/setup_qwen_mlx.sh", file=sys.stderr)
        return 1

    results_dir = Path(args.results_dir)
    work_dir = results_dir / "_work"
    results_dir.mkdir(parents=True, exist_ok=True)
    work_dir.mkdir(parents=True, exist_ok=True)

    try:
        wav_path, source_label = normalize_input(args.input, work_dir)
        text, metrics = run_qwen(
            qwen_bin=qwen_bin,
            wav_path=wav_path,
            model=MODELS[args.model],
            language=args.language,
            cache_dir=Path(args.cache_dir),
        )
    except Exception as exc:
        print(f"Qwen probe failed: {exc}", file=sys.stderr)
        return 1

    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    short_hash = hashlib.sha1(args.input.encode("utf-8")).hexdigest()[:8]
    base_name = f"{stamp}-qwen-{args.model}-{short_hash}"
    text_path = results_dir / f"{base_name}.txt"
    json_path = results_dir / f"{base_name}.json"

    text_path.write_text(text.strip() + "\n", encoding="utf-8")
    metadata = {
        "input": args.input,
        "sourceLabel": source_label,
        "model": MODELS[args.model],
        "language": args.language,
        "normalizedWav": str(wav_path),
        "transcriptPath": str(text_path),
        **metrics,
    }
    json_path.write_text(
        json.dumps(metadata, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    if not args.keep_wav:
        try:
            wav_path.unlink()
        except OSError:
            pass

    print(f"Transcript: {text_path}")
    print(f"Metrics:    {json_path}")
    print(f"Time:       {metrics['elapsedSeconds']:.2f}s")
    if metrics["peakRssMb"] is not None:
        print(f"Peak RSS:   {metrics['peakRssMb']:.0f} MB")
    return 0


def normalize_input(raw_input: str, work_dir: Path) -> tuple[Path, str]:
    source = download_youtube(raw_input, work_dir) if is_url(raw_input) else Path(raw_input)
    if not source.exists():
        raise FileNotFoundError(source)

    wav_path = work_dir / f"normalized-{hashlib.sha1(str(source).encode()).hexdigest()[:10]}.wav"
    ffmpeg = find_binary(
        "ffmpeg",
        [
            ROOT / "src-tauri" / "binaries" / "ffmpeg-aarch64-apple-darwin",
            Path("/opt/homebrew/bin/ffmpeg"),
        ],
    )
    run_checked(
        [
            str(ffmpeg),
            "-y",
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            str(source),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            str(wav_path),
        ]
    )
    return wav_path, str(source)


def download_youtube(url: str, work_dir: Path) -> Path:
    yt_dlp = find_binary(
        "yt-dlp",
        [
            ROOT / "src-tauri" / "binaries" / "yt-dlp-aarch64-apple-darwin",
            Path("/opt/homebrew/bin/yt-dlp"),
        ],
    )
    template = work_dir / "youtube-%(id)s.%(ext)s"
    run_checked(
        [
            str(yt_dlp),
            "--ignore-config",
            "-f",
            "bestaudio/best",
            "-N",
            "4",
            "--no-playlist",
            "-o",
            str(template),
            url,
        ]
    )
    candidates = sorted(
        p for p in work_dir.glob("youtube-*") if p.is_file() and p.suffix != ".part"
    )
    if not candidates:
        raise RuntimeError("YouTube audio was not downloaded")
    return candidates[-1]


def run_qwen(
    *,
    qwen_bin: Path,
    wav_path: Path,
    model: str,
    language: str | None,
    cache_dir: Path,
) -> tuple[str, dict]:
    cache_dir.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["HF_HOME"] = str(cache_dir)
    env["HF_HUB_DISABLE_TELEMETRY"] = "1"
    env["HF_HUB_DISABLE_XET"] = "1"

    cmd = [
        str(qwen_bin),
        str(wav_path),
        "--model",
        model,
        "--stdout-only",
        "--no-progress",
    ]
    if language:
        cmd.extend(["--language", language])

    started = time.perf_counter()
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )
    peak_rss_kb: int | None = None
    while proc.poll() is None:
        rss = read_rss_kb(proc.pid)
        if rss is not None:
            peak_rss_kb = max(peak_rss_kb or 0, rss)
        time.sleep(0.5)
    stdout, stderr = proc.communicate()
    elapsed = time.perf_counter() - started

    if proc.returncode != 0:
        raise RuntimeError(stderr.strip() or f"exit code {proc.returncode}")

    return stdout.strip(), {
        "elapsedSeconds": elapsed,
        "peakRssMb": None if peak_rss_kb is None else peak_rss_kb / 1024,
        "returnCode": proc.returncode,
        "stderrTail": "\n".join(stderr.strip().splitlines()[-20:]),
    }


def read_rss_kb(pid: int) -> int | None:
    result = subprocess.run(
        ["ps", "-o", "rss=", "-p", str(pid)],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        check=False,
    )
    value = result.stdout.strip()
    if not value:
        return None
    try:
        return int(value)
    except ValueError:
        return None


def find_binary(name: str, candidates: list[Path]) -> Path:
    for candidate in candidates:
        if candidate.exists():
            return candidate
    for part in os.environ.get("PATH", "").split(os.pathsep):
        path = Path(part) / name
        if path.exists():
            return path
    raise FileNotFoundError(name)


def run_checked(cmd: list[str]) -> None:
    result = subprocess.run(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f"failed: {' '.join(cmd)}")


def is_url(value: str) -> bool:
    parsed = urlparse(value)
    return parsed.scheme in {"http", "https"}


if __name__ == "__main__":
    raise SystemExit(main())
