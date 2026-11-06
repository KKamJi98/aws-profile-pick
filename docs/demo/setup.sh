#!/usr/bin/env bash
# Create only synthetic fixtures. The destination must be a fresh empty directory.
set -euo pipefail
root=${1:?Usage: setup.sh EMPTY_DIRECTORY AWSPICK_BINARY}
binary=${2:?Usage: setup.sh EMPTY_DIRECTORY AWSPICK_BINARY}
[[ -d "$root" && -f "$binary" ]] || exit 1
for entry in "$root"/* "$root"/.[!.]* "$root"/..?*; do
  [[ ! -e "$entry" && ! -L "$entry" ]] || { printf 'Destination must be empty.\n' >&2; exit 1; }
done
# These repositories are synthetic too; awspick never uses Git or the network.
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
git init --quiet --bare "$root/remote.git"
git init --quiet "$root/repo"
(
  cd "$root/repo"
  git config user.name 'Demo User'
  git config user.email 'demo@example.invalid'
  git remote add origin ../remote.git
)
mkdir -p "$root/home/.aws" "$root/home/.ssh" "$root/home/.config/fish" "$root/bin"
cp "$binary" "$root/bin/awspick"
for group in dev stage prod; do
  for role in admin deploy ops readonly; do
    printf '[profile acme-%s-%s]
region = us-east-1

' "$group" "$role"
  done
done > "$root/home/.aws/config"
# No access keys, account IDs, SSO URLs or real hosts.
: > "$root/home/.aws/credentials"
printf 'Host demo-host
  HostName example.invalid
' > "$root/home/.ssh/config"
printf 'export AWS_PROFILE="acme-dev-readonly"
' > "$root/home/.bashrc"
cp "$root/home/.bashrc" "$root/home/.zshrc"
printf 'set -gx AWS_PROFILE "acme-dev-readonly"
' > "$root/home/.config/fish/config.fish"
# Keep the generated wrapper literal; it expands only when the demo shell runs it.
# shellcheck disable=SC2016
printf 'ap() {
  local output
  output=$(command awspick "$@") || return $?
  eval "$output"
}
' > "$root/home/wrapper.bash"
