# readio configuration reference

Read this file only when the user asks to configure readio or troubleshoot its
settings, images, voice, language, pace, or output device.

## Preserve the current file

The default file is `~/.readio/config.yaml`; `readio --home <dir>` changes it to
`<dir>/config.yaml`. Inspect the real launch command first.

On Unix, create a timestamped sibling backup before editing:

```sh
readio_config="$HOME/.readio/config.yaml"
readio_backup="${readio_config}.bak-$(date +%Y%m%d-%H%M%S)"
cp -p "$readio_config" "$readio_backup"
```

On Windows:

```powershell
$config = "$HOME\.readio\config.yaml"
$backup = "$config.bak-$(Get-Date -Format yyyyMMdd-HHmmss)"
Copy-Item -LiteralPath $config -Destination $backup
```

Prefer precise edits over reserializing YAML, because reserialization erases
comments. Preserve unknown keys, custom engines, and per-book entries.

## Prefer commands over YAML edits

```text
/lang zh
/mode manual | auto | aloud
/effort minimal | low | medium | high | xhigh | max
/speed <4-4000>
/rate <0.5-3>
/voice
/device
```

`shift+tab` cycles Manual → Auto → Read-aloud; `^r` cycles effort. Higher
effort reads more slowly. Effort controls text reveal and audiobook playback,
so do not create contradictory pace settings.

Canonical common fields:

```yaml
language: zh

reading:
  speed: 46
  mode: manual # manual | auto | aloud

effort:
  level: high # minimal | low | medium | high | xhigh | max

images:
  enabled: true
  max_rows: 16
```

## Read-aloud

Readio ships no speech model and selects none on first install. Open `/voice`:
the left pane downloads and validates models; the right assigns a ready model
globally or to one book. Confirm scope before saving.

- `moss`: recommended Mandarin audiobook voice on Apple Silicon.
- `kokoro`: small, strong local voice, especially for English; supports
  explicit Mandarin and English choices.
- `espeak`: tiny and immediate, deliberately robotic.
- `piper`, `qwen`, `supertonic`: alternatives with different trade-offs.
- `openai`: external OpenAI-compatible speech endpoint; readio cannot install
  the service.

Keep engine, language, and voice consistent. A Chinese book needs explicit
`zh`; do not rely on an English default.

```yaml
voice:
  engine: moss
  name: ""
  language: zh
  params: ""
  prefetch: 8
```

Model downloads may install managed `uv`, Python, and packages in the platform
user cache. Let readio show estimated size, licence, free space, and commands;
obtain confirmation before download. Use built-in presets unless the user
explicitly supplies and trusts a custom engine command.

## Audio-output safety

```yaml
voice:
  output:
    allow:
      - "AirPods"
      - "bluetooth"
    query: ""
    poll: 2
    on_mismatch: silence
```

An empty `allow: []` is unrestricted. Rules may be substrings. Keep `silence`
as the safe mismatch behavior unless the user chooses otherwise. If readio
cannot identify the current device, `silence` treats it as disallowed. Windows
has no built-in output-device probe; a custom `query` command is required for a
working whitelist.

## Verify changes

1. Re-read the changed region and report exact before/after values.
2. Start readio in a real interactive terminal; confirm the config parses and
   the header reflects the chosen language/mode.
3. For images, open `/sample` or a cover. Kitty graphics, iTerm2 images, or
   Sixel render; another terminal may legitimately show a placeholder.
4. For voice, wait until the model is ready, synthesize a short sample, and
   confirm language, voice, speed, and output device.
5. On failure, show the error and restore the backup rather than layering more
   edits onto broken YAML.
