#!/usr/bin/env bash
# Run from any directory. Never launch awspick with the real HOME or AWS env.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
for tool in cargo vhs ttyd ffmpeg python3 git; do
  command -v "$tool" >/dev/null || { printf 'Missing tool: %s
' "$tool" >&2; exit 1; }
done
# Metadata only: never read the real rc, AWS config or shared profile contents.
real_home=$HOME
before=$(python3 - "$real_home" <<'PY'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
paths = ['.zshrc', '.aws/config', '.config/awspick/profile']
print(json.dumps({p: (root / p).stat().st_mtime_ns if (root / p).exists() else None for p in paths}, sort_keys=True))
PY
)
check_real_files() {
  local after
  after=$(python3 - "$real_home" <<'PY'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
paths = ['.zshrc', '.aws/config', '.config/awspick/profile']
print(json.dumps({p: (root / p).stat().st_mtime_ns if (root / p).exists() else None for p in paths}, sort_keys=True))
PY
)
  [[ "$before" == "$after" ]] || { printf 'STOP: real file metadata changed.
' >&2; return 1; }
  printf 'Real .zshrc, AWS config and shared profile mtimes unchanged (nanoseconds).
'
}
trap check_real_files EXIT
cargo build --release --locked
sandbox=$(mktemp -d "${TMPDIR:-/tmp}/awspick-demo.XXXXXXXX")
bash docs/demo/setup.sh "$sandbox" "$repo/target/release/awspick"
# VHS outputs are relative to its cwd; only this docs symlink points outside the sandbox.
ln -s "$repo/docs" "$sandbox/docs"
cd "$sandbox"
# env -i removes AWS credentials, override paths, ZDOTDIR, startup hooks and prompts.
# HOME is relative so the binary's real path output contains no personal username.
env -i PATH="$sandbox/bin:$PATH" HOME=./home SHELL=/bin/bash \
  TERM=xterm-256color LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 \
  AWS_CONFIG_FILE=./home/.aws/config AWS_SHARED_CREDENTIALS_FILE=./home/.aws/credentials \
  AWS_EC2_METADATA_DISABLED=true AWS_PROFILE=acme-dev-readonly \
  AWSPICK_GROUP_RULES='dev=dev;stage=stage;prod=prod' \
  vhs "$repo/docs/demo/awspick.tape"
printf 'Synthetic sandbox retained for inspection: %s
' "$sandbox"
