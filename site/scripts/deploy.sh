#!/usr/bin/env bash
# Push the built site (and optionally the wasm demo bundle) to a static host,
# switch to it atomically, smoke-test it and roll back when the smoke fails.
#
#   site/scripts/deploy.sh [deploy]           ship the site as a new release
#   site/scripts/deploy.sh rollback [NAME]    switch back (previous, or NAME)
#   site/scripts/deploy.sh list               show releases and the live one
#
# All target values come from an env file (default site/.env.deploy, gitignored;
# override the path with DEPLOY_ENV_FILE); see site/.env.deploy.example. The
# remote layout is
#   $DEPLOY_REMOTE_ROOT/releases/<UTC timestamp>-<git sha>/
#   $DEPLOY_REMOTE_ROOT/current -> releases/<name>   (relative symlink)
# Requires rsync, ssh and curl locally, and a POSIX shell on the target.
#
# The demo bundle (DEPLOY_DEMO_DIST, a trunk build made with --public-url /demo/)
# is placed under demo/ in the release. A generated demo/_headers lists every
# demo file explicitly: hashed build files (a 16 hex digit hash in the name) as
# immutable for a year, everything else (the entry page) as revalidated.
#
# DEPLOY_DIST and DEPLOY_DEMO_DIST set in the calling environment win over the
# env file, so a CI job can point at its own build output.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${DEPLOY_ENV_FILE:-$HERE/.env.deploy}"
[ -f "$ENV_FILE" ] || { echo "missing $ENV_FILE (copy .env.deploy.example)" >&2; exit 2; }
pre_dist="${DEPLOY_DIST-}"; pre_demo="${DEPLOY_DEMO_DIST-}"
set -a; . "$ENV_FILE"; set +a
[ -z "$pre_dist" ] || DEPLOY_DIST="$pre_dist"
[ -z "$pre_demo" ] || DEPLOY_DEMO_DIST="$pre_demo"
: "${DEPLOY_SSH_TARGET:?set in the env file}" "${DEPLOY_REMOTE_ROOT:?set in the env file}"
abs() { case "$1" in /*) printf '%s' "$1" ;; *) printf '%s' "$HERE/$1" ;; esac; }
DIST="$(abs "${DEPLOY_DIST:-dist}")"
DEMO_DIST=""; [ -z "${DEPLOY_DEMO_DIST:-}" ] || DEMO_DIST="$(abs "$DEPLOY_DEMO_DIST")"
KEEP="${DEPLOY_KEEP:-5}"
# At least two, so the previous release survives the prune and a failed smoke can roll back to it.
[ "$KEEP" -ge 2 ] 2>/dev/null || KEEP=2
# shellcheck disable=SC2206
SSH_OPTS=(-o BatchMode=yes ${DEPLOY_SSH_OPTS:-})
rssh() { ssh "${SSH_OPTS[@]}" "$DEPLOY_SSH_TARGET" "$@"; }

# Re-points current at release $1 (a name under releases/) with an atomic rename.
switch_to() {
  rssh bash -s -- "$DEPLOY_REMOTE_ROOT" "$1" <<'REMOTE'
set -euo pipefail
root="$1"; want="$2"
[ -d "$root/releases/$want" ] || { echo "no such release: '$want'" >&2; exit 1; }
ln -sfn "releases/$want" "$root/.current.new"
mv -T "$root/.current.new" "$root/current"
REMOTE
}

# Fetches $1 and fails unless it answers 200. Prints the status line.
fetch_ok() {
  local code
  code="$(curl -s -L -o /dev/null -w '%{http_code}' --max-time 30 "$1" || true)"
  echo "check $1 -> HTTP $code"
  [ "$code" = 200 ]
}

# Site root answers 200; when a demo shipped, its page answers 200 and the
# hashed wasm and JS it names resolve (the wasm starts with the \0asm magic).
smoke() {
  [ -n "${DEPLOY_CHECK_URL:-}" ] || { echo "no DEPLOY_CHECK_URL; skipping smoke"; return 0; }
  local base="${DEPLOY_CHECK_URL%/}" page wasm js magic
  fetch_ok "$base/" || return 1
  [ "$1" = demo ] || return 0
  fetch_ok "$base/demo/" || return 1
  page="$(curl -s -L --max-time 30 "$base/demo/")"
  wasm="$(printf '%s' "$page" | grep -oE "[A-Za-z0-9_./-]+_bg\.wasm" | head -1 || true)"
  js="$(printf '%s' "$page" | grep -oE "[A-Za-z0-9_./-]+\.js" | grep -v webgpu-check | head -1 || true)"
  [ -n "$wasm" ] || { echo "demo page names no wasm file" >&2; return 1; }
  url_of() { case "$1" in http*) echo "$1" ;; /*) echo "${base}$1" ;; *) echo "$base/demo/${1#./}" ;; esac; }
  fetch_ok "$(url_of "$wasm")" || return 1
  magic="$(curl -s -L --max-time 30 -r 0-3 "$(url_of "$wasm")" | head -c 4 | od -An -tx1 | tr -d ' \n')"
  [ "$magic" = 0061736d ] || { echo "demo wasm is not WebAssembly (magic '$magic')" >&2; return 1; }
  if [ -n "$js" ]; then fetch_ok "$(url_of "$js")" || return 1; fi
  echo "wasm cache-control: $(curl -s -L -I --max-time 30 "$(url_of "$wasm")" | tr -d '\r' | grep -i '^cache-control' || echo none)"
}

cmd="${1:-deploy}"; shift || true
shipped=site
case "$cmd" in
deploy)
  [ -f "$DIST/index.html" ] || { echo "no built site at $DIST (run the site build first)" >&2; exit 2; }
  if [ -n "$DEMO_DIST" ]; then
    [ -f "$DEMO_DIST/index.html" ] || { echo "no demo bundle at $DEMO_DIST (trunk build --public-url /demo/)" >&2; exit 2; }
    ls "$DEMO_DIST"/*_bg.wasm >/dev/null 2>&1 || { echo "demo bundle has no *_bg.wasm" >&2; exit 2; }
    shipped=demo
  fi
  sha="${DEPLOY_VERSION:-$(git -C "$HERE" rev-parse --short=12 HEAD 2>/dev/null || echo nogit)}"
  name="$(date -u +%Y%m%dT%H%M%SZ)-$sha"
  stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
  cp -a "$DIST/." "$stage/"
  if [ -n "$DEMO_DIST" ]; then
    rm -rf "$stage/demo"; mkdir -p "$stage/demo"
    cp -a "$DEMO_DIST/." "$stage/demo/"
    # Explicit per-file cache rules for hosts that read a _headers file.
    {
      echo "# Generated by deploy.sh. Hashed build files never change; the entry page is revalidated."
      (cd "$stage/demo" && find . -type f ! -name _headers | sort) | while read -r f; do
        f="${f#./}"
        echo "/demo/$f"
        if printf '%s' "$f" | grep -qE -- '[-_.][0-9a-f]{16}([-_.]|$)'; then
          echo "  Cache-Control: public, max-age=31536000, immutable"
        else
          echo "  Cache-Control: no-cache"
        fi
      done
    } > "$stage/demo/_headers"
  fi
  printf '%s\n' "$sha" > "$stage/version.txt"
  # Precompressed copies for gzip_static.
  find "$stage" -type f \( -name '*.html' -o -name '*.css' -o -name '*.js' -o -name '*.mjs' \
    -o -name '*.json' -o -name '*.svg' -o -name '*.wasm' -o -name '*.xml' -o -name '*.txt' \) \
    -size +256c ! -name '*.gz' -exec gzip -9 -k -f {} +
  # Manifest so the target can verify what arrived.
  (cd "$stage" && find . -type f ! -name MANIFEST.sha256 -print0 | sort -z | xargs -0 sha256sum > MANIFEST.sha256)
  prev="$(rssh bash -s -- "$DEPLOY_REMOTE_ROOT" <<'REMOTE' || true
basename "$(readlink "$1/current" 2>/dev/null || true)"
REMOTE
)"
  [ "$prev" != "." ] && [ "$prev" != "/" ] || prev=""
  rssh "mkdir -p '$DEPLOY_REMOTE_ROOT/releases/$name'"
  rsync -a --delete -e "ssh ${SSH_OPTS[*]}" "$stage/" "$DEPLOY_SSH_TARGET:$DEPLOY_REMOTE_ROOT/releases/$name/"
  # Verify on the target, refuse the swap on any mismatch, then swap and prune.
  rssh bash -s -- "$DEPLOY_REMOTE_ROOT" "$name" "$KEEP" <<'REMOTE'
set -euo pipefail
root="$1"; name="$2"; keep="$3"; rel="$root/releases/$name"
cd "$rel" && sha256sum --quiet -c MANIFEST.sha256 || { echo "verify failed; not switching" >&2; rm -rf "$rel"; exit 1; }
chmod -R a+rX "$rel"
ln -sfn "releases/$name" "$root/.current.new"
mv -T "$root/.current.new" "$root/current"
cd "$root/releases" && ls -1 | sort -r | tail -n +"$((keep + 1))" | while read -r old; do rm -rf -- "$old"; done
REMOTE
  echo "live: $name"
  if ! smoke "$shipped"; then
    if [ -n "$prev" ] && [ "$prev" != "$name" ]; then
      echo "smoke failed; rolling back to $prev" >&2
      switch_to "$prev"
      rssh "rm -rf -- '$DEPLOY_REMOTE_ROOT/releases/$name'"
      echo "live: $prev (rolled back from $name)" >&2
    else
      echo "smoke failed and there is no previous release to roll back to" >&2
    fi
    exit 1
  fi
  ;;
list)
  rssh bash -s -- "$DEPLOY_REMOTE_ROOT" <<'REMOTE'
root="$1"; live="$(readlink "$root/current" 2>/dev/null || true)"
echo "live: ${live:-none}"; ls -1 "$root/releases" | sort -r | sed 's/^/  /'
REMOTE
  ;;
rollback)
  target="${1:-}"
  rssh bash -s -- "$DEPLOY_REMOTE_ROOT" "${target:--}" <<'REMOTE'
set -euo pipefail
root="$1"; want="$2"; [ "$want" = - ] && want=""
live="$(basename "$(readlink "$root/current" 2>/dev/null || echo none)")"
if [ -z "$want" ]; then
  want="$(ls -1 "$root/releases" | sort -r | grep -v -x "$live" | awk -v l="$live" '$0 < l' | head -1 || true)"
fi
[ -n "$want" ] && [ -d "$root/releases/$want" ] || { echo "no such release: '$want'" >&2; exit 1; }
ln -sfn "releases/$want" "$root/.current.new"
mv -T "$root/.current.new" "$root/current"
echo "live: $want (was $live)"
REMOTE
  # A rollback target may predate the demo; check the demo only when it has one.
  if rssh "[ -d '$DEPLOY_REMOTE_ROOT/current/demo' ]"; then shipped=demo; fi
  smoke "$shipped" || { echo "site check failed after rollback" >&2; exit 1; }
  ;;
*) echo "usage: $0 [deploy|rollback [NAME]|list]" >&2; exit 2 ;;
esac

if [ -n "${DEPLOY_CONTAINER:-}" ] && [ "$cmd" != list ]; then
  rssh "docker inspect -f '{{.State.Health.Status}}' '$DEPLOY_CONTAINER'" | sed 's/^/container health: /'
fi
