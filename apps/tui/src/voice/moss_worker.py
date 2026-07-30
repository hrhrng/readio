"""readio's resident MOSS-TTS-Nano worker.

MOSS is the recommended Mandarin engine, but it is never silently selected or
downloaded: the reader chooses it globally or for one book first. The model
then stays in this child process and receives one JSON request per sentence.

The voice reference is not re-encoded at runtime. A Spark audiobook sample was
encoded once into 118 frames × 16 codec tokens; the compressed token matrix
below is only 3.9 KB. It conditions every request without keeping or processing
the reference wav.
"""

import argparse
import base64
import json
import os
import shutil
import shlex
import sys
import wave
import zlib

DEFAULT_MODEL = "mlx-community/MOSS-TTS-Nano-100M"
DEFAULT_CODEC = "mlx-community/MOSS-Audio-Tokenizer-Nano"
GIB = 1024**3
DISK_RESERVE = 5 * GIB

# uint16 little-endian, shape (118, 16), zlib level 9 then base64.
# sha256 of the source npz:
# 714a225c4d4b65f293709d5173b22911ee2cc8664c125cec8b5249a7a0f2d574
VOICE_TOKENS = (
    "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"
)


def claim_stdout():
    """Reserve stdout for the line-delimited JSON protocol."""
    private = os.dup(1)
    os.dup2(2, 1)
    channel = os.fdopen(private, "w", buffering=1, encoding="utf-8")
    sys.stdout = sys.stderr
    return channel


OUT = None


def reply(**fields):
    OUT.write(json.dumps(fields) + "\n")
    OUT.flush()


def prompt_audio_codes():
    import numpy as np

    packed = zlib.decompress(base64.b64decode(VOICE_TOKENS))
    return np.frombuffer(packed, dtype="<u2").astype("int32").reshape(118, 16)


def ensure_download_space(sources):
    """Check all missing Hugging Face files before either download starts."""
    from huggingface_hub import HfApi, try_to_load_from_cache
    from huggingface_hub.constants import HF_HUB_CACHE

    missing = 0
    remote = []
    for source in sources:
        if os.path.exists(os.path.expanduser(source)):
            continue
        remote.append(source)
        info = HfApi().model_info(source, files_metadata=True)
        for sibling in info.siblings:
            size = getattr(sibling, "size", None)
            if size is None:
                raise RuntimeError(
                    f"cannot check disk space for {source}: Hugging Face did "
                    f"not report the size of {sibling.rfilename}"
                )
            cached = try_to_load_from_cache(
                source, sibling.rfilename, cache_dir=HF_HUB_CACHE
            )
            if not isinstance(cached, (str, os.PathLike)) or not os.path.isfile(cached):
                missing += int(size)

    if missing == 0:
        return

    probe = os.path.expanduser(HF_HUB_CACHE)
    while not os.path.exists(probe):
        parent = os.path.dirname(probe)
        if parent == probe:
            break
        probe = parent
    free = shutil.disk_usage(probe).free
    required = missing * 2 + DISK_RESERVE
    if free < required:
        names = ", ".join(remote)
        raise RuntimeError(
            f"not enough disk space for {names}: {free / GIB:.1f} GiB free, "
            f"{required / GIB:.1f} GiB required ({missing / GIB:.1f} GiB "
            "download plus temporary space and a 5.0 GiB reserve)"
        )


def generation_params(raw):
    values = {
        "max_tokens": 375,
        "temperature": 0.8,
        "top_p": 0.95,
        "top_k": 25,
        "repetition_penalty": 1.2,
    }
    for token in shlex.split(raw or ""):
        if "=" not in token:
            raise ValueError(f"model parameter must be key=value: {token}")
        key, value = token.split("=", 1)
        if key not in values:
            raise ValueError(f"unsupported MOSS parameter: {key}")
        values[key] = int(value) if key in {"max_tokens", "top_k"} else float(value)
    if not 0 < values["temperature"] <= 2:
        raise ValueError("temperature must be in (0, 2]")
    if not 0 < values["top_p"] <= 1:
        raise ValueError("top_p must be in (0, 1]")
    return values


def synthesize(model, text, codes, codec, raw_params=""):
    params = generation_params(raw_params)
    results = list(
        model.generate(
            text,
            prompt_audio_codes=codes,
            mode="voice_clone",
            max_tokens=params["max_tokens"],
            do_sample=True,
            audio_temperature=params["temperature"],
            audio_top_p=params["top_p"],
            audio_top_k=params["top_k"],
            audio_repetition_penalty=params["repetition_penalty"],
            audio_tokenizer_device="cpu",
            audio_tokenizer_source=codec,
        )
    )
    if not results:
        raise ValueError("the model returned no audio")
    return results


def write_wav(path, results):
    import numpy as np

    rate = int(results[0].sample_rate)
    chunks = []
    channels = None
    for result in results:
        if int(result.sample_rate) != rate:
            raise ValueError("the model changed sample rate between segments")
        audio = np.asarray(result.audio, dtype="float32")
        if audio.ndim == 1:
            audio = audio[:, None]
        if audio.ndim != 2:
            raise ValueError(f"expected [samples, channels], got {audio.shape}")
        channels = channels or int(audio.shape[1])
        if int(audio.shape[1]) != channels:
            raise ValueError("the model changed channel count between segments")
        chunks.append(audio)
    pcm = np.concatenate(chunks, axis=0)
    pcm = (np.clip(pcm, -1.0, 1.0) * 32767.0).astype("<i2")
    with wave.open(path, "wb") as out:
        out.setnchannels(channels)
        out.setsampwidth(2)
        out.setframerate(rate)
        out.writeframes(pcm.tobytes())


def main():
    global OUT

    ap = argparse.ArgumentParser(description="readio resident MOSS-TTS-Nano worker")
    ap.add_argument("--model", default=DEFAULT_MODEL, help="model id or local path")
    ap.add_argument("--codec", default=DEFAULT_CODEC, help="codec id or local path")
    args = ap.parse_args()

    OUT = claim_stdout()
    try:
        ensure_download_space([args.model, args.codec])
        from mlx_audio.tts.utils import load_model

        model = load_model(args.model)
        codes = prompt_audio_codes()
        # Load the codec and compile the first generation before declaring the
        # worker ready. No reader waits on this private punctuation clip.
        synthesize(model, "。", codes, args.codec)
    except Exception as err:  # noqa: BLE001 - protocol returns the useful text
        reply(ready=False, error=f"{type(err).__name__}: {err}")
        return 1

    reply(ready=True)
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            job = json.loads(line)
            results = synthesize(
                model, job["text"], codes, args.codec, job.get("params", "")
            )
            write_wav(job["out"], results)
            reply(ok=True)
        except Exception as err:  # noqa: BLE001
            reply(ok=False, error=f"{type(err).__name__}: {err}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
