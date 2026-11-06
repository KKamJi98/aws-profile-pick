# Reproduce the awspick demo

The GIF and MP4 are recorded by [VHS](https://github.com/charmbracelet/vhs) from the actual Rust binary. The tape does not simulate CLI output.

## Record

Requirements: Rust/Cargo, Git, VHS 0.12.1 or compatible, ttyd, ffmpeg, Python 3, bash, and a monospace font. The tape uses Menlo; change `Set FontFamily` if it is unavailable on your system. `gifsicle` is optional if you need to optimize a larger recording.

From the repository root:

```bash
bash docs/demo/record.sh
```

Do not run the tape directly with `vhs`: `record.sh` supplies its isolated environment and the real binary. The script builds with `cargo build --release --locked` before recording.

Outputs:

- `docs/demo/awspick.gif` - embedded in the README.
- `docs/demo/awspick.mp4` - linked video.

The committed recording is 20.04 seconds at 1100 x 690. Timing and compressed size can vary slightly with the VHS/ffmpeg version. The target is 20-30 seconds and a GIF below 3 MB.

## Isolation and fixtures

`setup.sh` accepts a fresh empty directory and a binary path. It refuses a nonempty destination. It creates an isolated Git repository with a local bare remote, 12 synthetic `acme-{dev,stage,prod}-{admin,deploy,ops,readonly}` profiles, empty AWS credentials, fake shell rc files, a fictional SSH host, and an `ap()` wrapper. It does not copy any real configuration.

`record.sh` uses `env -i` to clear inherited credentials, AWS override paths, `ZDOTDIR`, shell startup hooks, and prompt configuration. It passes only the recording paths and explicit synthetic values. HOME is `./home` relative to the sandbox, so the binary's unedited status output contains no personal home path. The prompt is `$ `.

The demo sets `AWSPICK_GROUP_RULES='dev=dev;stage=stage;prod=prod'`. `stage` is a custom group, not a built-in alias for `stg`. No AWS CLI or AWS API request runs. No tokens, account IDs, access keys, host IPs, or real profile names are needed.

Before and after recording, Python checks only the nanosecond mtime metadata of the real `.zshrc`, `.aws/config`, and `.config/awspick/profile`. Their contents are never read. A mismatch fails the script. The sandbox is retained at the printed path for inspection; remove only that specific directory when you no longer need it.

For local work, you can choose where the sandbox is created:

```bash
TMPDIR=/path/to/your/scratch bash docs/demo/record.sh
```

The selected scratch directory must already exist. This does not change the on-screen relative HOME.

## Review before publishing

1. Confirm `record.sh` exits 0 and reports unchanged real-file mtimes.
2. Use `ffprobe` to measure the final duration, dimensions, and size.
3. Extract multiple points from the **final GIF**, including the full table, regex menu, backup/update lines, and final `echo "$AWS_PROFILE"` result.
4. Open the PNGs and check legibility, clipping, and absence of real data. Do not assume synthetic fixtures alone prove the recording is safe.
5. If you optimize or re-record, repeat the frame review on the new final GIF.

No terminal output is edited, replaced, or redacted after recording. Fix fixtures or rendering and record again instead.

The flow diagram is hand-authored at `docs/assets/how-it-works.svg`. Render it to PNG and inspect its full viewBox before committing; a cropped thumbnail is not a full-diagram check.
