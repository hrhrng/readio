"""readio's resident speech worker.

A model that has to be loaded again for every sentence is not a slow model, it
is a slow way of asking. Driving Kokoro through its command line costs about
eight seconds a sentence on a laptop CPU, of which the inference is one and a
half: the rest is a fresh interpreter, librosa and scipy imported inside the
synthesis call, and a progress spinner nobody asked for. Said aloud, that
sentence lasts four seconds. An engine slower than speech can never be caught
up with — no amount of rendering ahead helps, because the queue drains faster
than it fills — so read-aloud stalls at every sentence boundary.

This script is the same model with the setup paid once. readio starts it, keeps
it, and talks to it in lines of JSON: one request per line in, one reply per
line out. It is deliberately small and deliberately boring, because it is
started by a reader who wants to hear a book rather than debug a daemon.

The second cost, and on short lines the larger one, is the phonemizer: see
`cache_the_phonemizer` below.

    in   {"text": "...", "out": "/path/utt-3.wav", "voice": "zf_xiaoxiao",
          "lang": "cmn", "speed": 1.0}
    out  {"ok": true}
    out  {"ok": false, "error": "what went wrong"}

The first line it ever writes is {"ready": true}, after the model is loaded and
warmed, so that readio knows the wait is over rather than guessing at it.

Nothing here is Kokoro-specific except two imports and one call; another ONNX
model would be the same file with those changed.
"""

import argparse
import json
import sys
import threading
import wave


def reply(**fields):
    """One JSON object, one line, flushed. Buffering here is a silent hang."""
    sys.stdout.write(json.dumps(fields) + "\n")
    sys.stdout.flush()


def cache_the_phonemizer():
    """Build the espeak backend once per language instead of once per sentence.

    This is the single biggest cost in a warm worker, and it is not the model.
    `phonemizer.phonemize` is a convenience function that constructs an entire
    backend on every call — on macOS that means dlopen'ing libespeak eight
    times, because phonemizer copies the library to a fresh temporary file each
    time to get around espeak's global state — and kokoro-onnx calls it once per
    sentence. Measured on an M4: 2.2 seconds of every 2.7, against 0.5 seconds
    of actual inference. A seven-character line took longer to prepare than to
    say.

    phonemizer's own docstring says to minimise calls for exactly this reason;
    it simply has no cache. So readio keeps the backends and hands the same one
    back for the same language.

    Any failure here is not fatal. The unpatched path is slow, not broken, and a
    reader with an unfamiliar phonemizer version should get a slow book rather
    than no book.
    """
    try:
        import phonemizer
        from phonemizer.backend import EspeakBackend
        from phonemizer.separator import default_separator
        from phonemizer.utils import list2str, str2list

        backends = {}
        guard = threading.Lock()
        original = phonemizer.phonemize

        def phonemize(
            text,
            language="en-us",
            backend="espeak",
            separator=default_separator,
            strip=False,
            preserve_punctuation=False,
            with_stress=False,
            **rest,
        ):
            # Anything readio does not recognise goes back to phonemizer, which
            # knows its own defaults better than this wrapper does.
            if backend != "espeak" or rest:
                return original(
                    text,
                    language=language,
                    backend=backend,
                    separator=separator,
                    strip=strip,
                    preserve_punctuation=preserve_punctuation,
                    with_stress=with_stress,
                    **rest,
                )
            key = (language, preserve_punctuation, with_stress)
            engine = backends.get(key)
            if engine is None:
                # Under the lock because the languages are prepared on a
                # background thread: without it, a sentence arriving mid-warm-up
                # would build a second backend and pay the cost this exists to
                # avoid.
                with guard:
                    engine = backends.get(key)
                    if engine is None:
                        engine = EspeakBackend(
                            language,
                            preserve_punctuation=preserve_punctuation,
                            with_stress=with_stress,
                        )
                        backends[key] = engine
            spoken = engine.phonemize(str2list(text), separator=separator, strip=strip)
            return list2str(spoken) if isinstance(text, str) else spoken

        phonemizer.phonemize = phonemize

        def prime(language):
            """Pay a language's backend cost at startup, where nobody hears it."""
            try:
                phonemize("a", language, preserve_punctuation=True, with_stress=True)
            except Exception:  # noqa: BLE001
                pass

        return prime
    except Exception:  # noqa: BLE001 - slow is survivable, dead is not
        return None


def write_wav(path, samples, rate):
    """Write float samples as a 16-bit mono wav.

    Deliberately the standard library rather than soundfile or scipy: this runs
    inside whichever environment happens to hold kokoro-onnx, and the fewer
    things that have to be true there, the fewer ways read-aloud has to fail.
    """
    import numpy as np

    pcm = np.clip(np.asarray(samples, dtype="float32"), -1.0, 1.0)
    pcm = (pcm * 32767.0).astype("<i2")
    with wave.open(path, "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(int(rate))
        out.writeframes(pcm.tobytes())


def main():
    ap = argparse.ArgumentParser(description="readio resident Kokoro worker")
    ap.add_argument("--model", required=True)
    ap.add_argument("--voices", required=True)
    ap.add_argument("--voice", default="af_heart", help="voice for the warm-up")
    ap.add_argument(
        "--warm",
        default="en-us,cmn",
        help="languages to prepare at startup, so no sentence pays for the first",
    )
    args = ap.parse_args()

    try:
        from kokoro_onnx import Kokoro

        # kokoro-onnx imports these inside its synthesis call, which would make
        # the reader's first sentence pay two seconds that have nothing to do
        # with it. Paid here instead, while nobody is listening yet.
        import librosa  # noqa: F401
        import scipy.signal  # noqa: F401

        prime = cache_the_phonemizer()
        model = Kokoro(args.model, args.voices)
        # The first inference is slower than every one after it. Spending it on
        # a syllable nobody hears is the difference between read-aloud starting
        # promptly and read-aloud starting eventually.
        model.create("a", voice=args.voice, speed=1.0, lang="en-us")
    except Exception as err:  # noqa: BLE001 - the reader gets the text, not a trace
        reply(ready=False, error=f"{type(err).__name__}: {err}")
        return 1

    # The first sentence in a language also pays for that language's espeak
    # backend — two seconds, whether the sentence is a chapter or a heading. So
    # both of readio's languages are prepared here, but *after* saying ready and
    # on a thread of their own: readio waits for the word, and a reader who is
    # still choosing a book should not be waiting for a language they may not
    # even be reading in.
    if prime:
        languages = [name.strip() for name in args.warm.split(",") if name.strip()]
        threading.Thread(
            target=lambda: [prime(name) for name in languages],
            daemon=True,
        ).start()

    reply(ready=True)

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            job = json.loads(line)
            samples, rate = model.create(
                job["text"],
                voice=job.get("voice") or args.voice,
                speed=float(job.get("speed", 1.0)),
                lang=job.get("lang") or "en-us",
            )
            write_wav(job["out"], samples, rate)
            reply(ok=True)
        except Exception as err:  # noqa: BLE001
            # One sentence failing is not the worker failing: report it and
            # stay up, so a single odd line does not end the chapter.
            reply(ok=False, error=f"{type(err).__name__}: {err}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
