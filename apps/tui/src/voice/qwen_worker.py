"""readio's resident Qwen3-TTS worker.

Qwen3-TTS is the best Chinese voice readio can drive, and through its command
line it is also the slowest. Measured on an M-series laptop, one sentence:

    via `qwen3-tts speak`   10.0 s for 4.7 s of audio   2.12x realtime
    with the model resident  4.1 s for 3.8 s of audio   1.08x realtime

The difference is not the model. It is a fresh Python, an `mlx_audio` import,
and 3.5 seconds of loading 0.6B parameters of weights — paid again for every
sentence, including a two-word heading. An engine slower than speech can never
be caught up with: the prefetch queue drains faster than it fills, so the
reading stalls at every sentence boundary no matter how deep the buffer.

Kept resident, it renders slightly faster than it speaks. That margin is thin —
Kokoro has far more of it — so this is the engine to choose when the voice
matters more than the certainty of never waiting.

    in   {"text": "...", "out": "/path/utt-3.wav", "voice": "serena",
          "lang": "chinese", "speed": 1.0}
    out  {"ok": true}
    out  {"ok": false, "error": "what went wrong"}

The first line it ever writes is {"ready": true}, after the model is loaded and
warmed, so that readio knows the wait is over rather than guessing at it.

MLX means Apple Silicon. On any other machine the install fails at the
dependency, which is the honest place for it to fail, and readio's engine list
says so before anyone presses enter.
"""

import argparse
import json
import os
import shutil
import sys
import wave

# The API takes the speaker as `voice`; `speaker` is a ValueError. Worth stating
# because the CLI this replaces spells the same thing `--speaker`.
DEFAULT_MODEL = "mlx-community/Qwen3-TTS-12Hz-0.6B-CustomVoice-bf16"
DEFAULT_VOICE = "serena"
GIB = 1024**3
DISK_RESERVE = 5 * GIB


def claim_stdout():
    """Take sole ownership of stdout, and point everyone else at stderr.

    This is not defensive tidiness; without it the engine does not work at all.
    `mlx_audio` prints progress of its own — "Initialized encoder codebooks",
    a tokenizer path — on *stdout*, which here is the protocol channel. readio
    reads the first line expecting `{"ready": true}` and gets a sentence of
    library chatter, so every utterance fails with the model's own log line as
    its error message.

    Reassigning `sys.stdout` is not enough: the noise comes from inside a native
    extension writing to file descriptor 1, which never consults Python. So the
    descriptor itself is moved — 1 is duplicated somewhere private, then stderr
    is dup'd over 1. Anything that writes to fd 1 afterwards, from any language,
    lands on stderr where it is merely informative; readio's protocol goes out
    through the private copy.

    Returns the file object to write replies to.
    """
    private = os.dup(1)
    os.dup2(2, 1)
    # Line buffered: a reply that sits in a buffer is a worker readio waits on
    # forever, which is the one failure mode with no message attached.
    channel = os.fdopen(private, "w", buffering=1, encoding="utf-8")
    sys.stdout = sys.stderr
    return channel


OUT = None


def reply(**fields):
    """One JSON object, one line, flushed. Buffering here is a silent hang."""
    OUT.write(json.dumps(fields) + "\n")
    OUT.flush()


def write_wav(path, chunks, rate):
    """Write float samples as a 16-bit mono wav.

    Deliberately the standard library rather than soundfile or scipy: this runs
    inside whichever environment happens to hold qwen3-tts, and the fewer things
    that have to be true there, the fewer ways read-aloud has to fail.

    The model emits float32 at 24 kHz; readio measures a clip's duration by
    parsing its header, so what matters is that the header is honest.
    """
    import numpy as np

    if not chunks:
        raise ValueError("the model returned no audio")
    pcm = np.concatenate([np.asarray(chunk, dtype="float32").reshape(-1) for chunk in chunks])
    pcm = np.clip(pcm, -1.0, 1.0)
    pcm = (pcm * 32767.0).astype("<i2")
    with wave.open(path, "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(int(rate))
        out.writeframes(pcm.tobytes())


def ensure_model_download_space(model):
    """Refuse a remote model download that could leave the disk full.

    Hugging Face downloads into its cache and may temporarily hold both the
    incoming bytes and the completed snapshot. Five GiB remains untouched for
    the OS and the book/audio files readio is actually here to create.
    """
    if os.path.exists(os.path.expanduser(model)):
        return

    from huggingface_hub import HfApi, try_to_load_from_cache
    from huggingface_hub.constants import HF_HUB_CACHE

    info = HfApi().model_info(model, files_metadata=True)
    missing = 0
    for sibling in info.siblings:
        size = getattr(sibling, "size", None)
        if size is None:
            raise RuntimeError(
                f"cannot check disk space for {model}: Hugging Face did not "
                f"report the size of {sibling.rfilename}"
            )
        cached = try_to_load_from_cache(
            model,
            sibling.rfilename,
            cache_dir=HF_HUB_CACHE,
        )
        if not isinstance(cached, (str, os.PathLike)) or not os.path.isfile(cached):
            missing += int(size)

    if missing == 0:
        return

    cache = os.path.expanduser(HF_HUB_CACHE)
    probe = cache
    while not os.path.exists(probe):
        parent = os.path.dirname(probe)
        if parent == probe:
            break
        probe = parent
    free = shutil.disk_usage(probe).free
    required = missing * 2 + DISK_RESERVE
    if free < required:
        raise RuntimeError(
            f"not enough disk space for {model}: "
            f"{free / GIB:.1f} GiB free, {required / GIB:.1f} GiB required "
            f"({missing / GIB:.1f} GiB download plus temporary space and "
            f"a 5.0 GiB reserve)"
        )


def synthesize(model, text, voice, lang, speed):
    """One sentence in, a list of float chunks and a sample rate out."""
    chunks = []
    rate = 24000
    for segment in model.generate(
        text=text,
        voice=voice,
        lang_code=lang,
        speed=speed,
        verbose=False,
    ):
        chunks.append(segment.audio)
        # Per segment rather than assumed: a model readio has not seen may not
        # be a 24 kHz one, and a wrong rate in the header is a clip readio
        # thinks is the wrong length — which paces the text to the wrong clock.
        rate = getattr(segment, "sample_rate", rate) or rate
    return chunks, rate


def main():
    global OUT

    ap = argparse.ArgumentParser(description="readio resident Qwen3-TTS worker")
    ap.add_argument("--model", default=DEFAULT_MODEL, help="model id or local path")
    ap.add_argument("--voice", default=DEFAULT_VOICE, help="speaker, and the warm-up voice")
    ap.add_argument(
        "--warm",
        default="chinese",
        help="language to prepare at startup, so no sentence pays for the first",
    )
    args = ap.parse_args()

    # Before the model is imported, because the import is one of the things that
    # writes to stdout uninvited.
    OUT = claim_stdout()

    try:
        ensure_model_download_space(args.model)

        from mlx_audio.tts.utils import load_model

        model = load_model(args.model)
        # The first inference is far slower than every one after it: MLX compiles
        # kernels on the way through. Spending it on a syllable nobody hears is
        # the difference between read-aloud starting promptly and eventually.
        synthesize(model, "。", args.voice, args.warm, 1.0)
    except Exception as err:  # noqa: BLE001 - the reader gets the text, not a trace
        reply(ready=False, error=f"{type(err).__name__}: {err}")
        return 1

    reply(ready=True)

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            job = json.loads(line)
            # readio resolves one language for the configured scope before the
            # job reaches this worker. An empty value uses the preset's warm
            # language; sentence content never changes it.
            chunks, rate = synthesize(
                model,
                job["text"],
                job.get("voice") or args.voice,
                job.get("lang") or args.warm,
                float(job.get("speed", 1.0)),
            )
            write_wav(job["out"], chunks, rate)
            reply(ok=True)
        except Exception as err:  # noqa: BLE001
            # One sentence failing is not the worker failing: report it and stay
            # up, so a single odd line does not end the chapter.
            reply(ok=False, error=f"{type(err).__name__}: {err}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
