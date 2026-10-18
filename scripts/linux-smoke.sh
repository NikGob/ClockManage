#!/usr/bin/env bash
# End-to-end check of the Linux package on a real system (CI runs it on ubuntu-22.04): install the
# .deb, run the root step by hand and through pkexec, then start the app headless and drive it
# over MCP. A 1-minute focus lock must close a blocked app and put a site into hosts, and the app
# must take the site out when the lock ends. Removing the package must leave nothing blocked.
#
#   scripts/linux-smoke.sh target/release/bundle/deb/ClockManage_<version>_amd64.deb
#
# Needs sudo (or root), xvfb-run, dbus-run-session, curl and jq. It changes /etc/hosts and the
# browser policy folders while it runs: use a throwaway machine or container.
set -euo pipefail

deb=$(realpath "$1")
sudo=sudo
[ "$(id -u)" = 0 ] && sudo=
fail() { echo "FAIL: $*" >&2; exit 1; }
# No `cmd | grep -q` checks: grep quits early and pipefail then fails on the writer's SIGPIPE.
blocked() { case $(getent ahostsv4 "$1") in 0.0.0.0\ *) return 0 ;; *) return 1 ;; esac; }
alive() { s=$(cut -d' ' -f3 "/proc/$1/stat" 2>/dev/null) && [ "$s" != Z ]; }

echo "== install the package"
$sudo apt-get install -y "$deb" >/dev/null
test -x /usr/bin/clockmanage || fail "no /usr/bin/clockmanage"
# polkitd notices a new action file a moment after it lands. (pkaction's exit code says nothing.)
action() { [[ $(pkaction --action-id com.nikgob.clockmanage.block-sites --verbose 2>/dev/null) == *'exec.path -> /usr/bin/clockmanage'* ]]; }
for _ in $(seq 20); do action && break; sleep 0.5; done
action || fail "polkit doesn't know the root step"
test -f /usr/share/applications/ClockManage.desktop || fail "no desktop entry"

echo "== the root step by hand"
$sudo clockmanage --block-sites x.com youtube.com/shorts
blocked x.com || fail "x.com is not blocked in hosts"
grep -q youtube.com/shorts /etc/opt/chrome/policies/managed/clockmanage.json || fail "no Chrome policy"
grep -q youtube.com/shorts /etc/firefox/policies/policies.json || fail "no Firefox policy"
$sudo clockmanage --block-sites
blocked x.com && fail "x.com is still blocked"
test -e /etc/opt/chrome/policies/managed/clockmanage.json && fail "the Chrome policy was left behind"
if $sudo clockmanage --block-sites 'x.com 0.0.0.0'; then fail "an invalid site was accepted"; fi

if [ -n "$sudo" ]; then
  echo "== the root step through pkexec"
  # The runner has no desktop session: let this user run the root step as an active user would.
  printf '[ClockManage smoke test]\nIdentity=unix-user:%s\nAction=com.nikgob.clockmanage.block-sites\nResultAny=yes\nResultInactive=yes\nResultActive=yes\n' "$(id -un)" |
    sudo tee /etc/polkit-1/localauthority/50-local.d/clockmanage-smoke.pkla >/dev/null
  sleep 2
  pkexec /usr/bin/clockmanage --block-sites x.com
  blocked x.com || fail "pkexec: x.com is not blocked"
  pkexec /usr/bin/clockmanage --block-sites
  blocked x.com && fail "pkexec: x.com is still blocked"
  # Under pkexec the binary does nothing but the root step.
  if pkexec /usr/bin/clockmanage --cleanup; then fail "pkexec ran --cleanup as root"; fi
fi

echo "== the app, headless, over MCP"
data=${XDG_DATA_HOME:-$HOME/.local/share}/com.nikgob.clockmanage
rm -rf "$data"
log=$(mktemp)
dbus-run-session -- xvfb-run -a clockmanage --background >"$log" 2>&1 &
trap 'pkill -x clockmanage || true; echo "--- app output"; cat "$log"' EXIT
port=
for _ in $(seq 60); do
  port=$(jq -r '.mcp_port // empty' "$data/config.json" 2>/dev/null || true)
  [ -n "$port" ] && curl -s -o /dev/null "http://127.0.0.1:$port/mcp" && break
  sleep 1
done
[ -n "$port" ] || fail "the app didn't start its MCP server"
mcp() { curl -sf -H 'Content-Type: application/json' -d "$1" "http://127.0.0.1:$port/mcp"; }
call() { mcp "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"$1\",\"arguments\":$2}}"; }
mcp '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | jq -e '[.result.tools[].name] | index("start_focus_lock")' >/dev/null ||
  fail "tools/list has no start_focus_lock"
test -f "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/clockmanage.service" || fail "no autostart service"

target=$(mktemp -d)/cm-smoke-target
cp /bin/sleep "$target"
victim=$(sh -c "$target 600 >/dev/null 2>&1 & echo \$!")
call add_to_blocklist '{"sites":["x.com"],"apps":["cm-smoke-target"]}' | jq -e '.result.isError | not' >/dev/null ||
  fail "add_to_blocklist refused"
call start_focus_lock '{"minutes":1,"reason":"CI smoke test"}' | jq -e '.result.isError | not' >/dev/null ||
  fail "start_focus_lock refused"
for _ in $(seq 20); do alive "$victim" || break; sleep 0.5; done
alive "$victim" && fail "lock on: the blocked app is still running"
for _ in $(seq 20); do blocked x.com && break; sleep 0.5; done
blocked x.com || fail "lock on: x.com is not in hosts"
echo "lock on: the app was closed, x.com is blocked; waiting for the lock to end"
for _ in $(seq 100); do blocked x.com || break; sleep 1; done
blocked x.com && fail "lock over: x.com is still in hosts"
grep -qi panicked "$log" && fail "the app panicked"
pkill -x clockmanage || true

echo "== removing the package takes the block out"
$sudo clockmanage --block-sites x.com
blocked x.com || fail "x.com is not blocked before the removal"
$sudo apt-get remove -y "$(dpkg-deb -f "$deb" Package)" >/dev/null
blocked x.com && fail "the removed package left x.com blocked"
[ -n "$sudo" ] && sudo rm -f /etc/polkit-1/localauthority/50-local.d/clockmanage-smoke.pkla
trap - EXIT
echo "OK: the Linux package works"
