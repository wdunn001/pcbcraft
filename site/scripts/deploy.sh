#!/usr/bin/env bash
# Push the built site to a static host and switch to it atomically.
#
#   site/scripts/deploy.sh [deploy]           ship site/dist as a new release
#   site/scripts/deploy.sh rollback [NAME]    switch back (previous, or NAME)
#   site/scripts/deploy.sh list               show releases and the live one
#
# All target values come from site/.env.deploy (gitignored); see
# site/.env.deploy.example. The remote layout is
#   $DEPLOY_REMOTE_ROOT/releases/<UTC timestamp>-<git sha>/
#   $DEPLOY_REMOTE_ROOT/current -> releases/<name>   (relative symlink)
# Requires rsync and ssh locally, and a POSIX shell on the target.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${DEPLOY_ENV_FILE:-$HERE/.env.deploy}"
[ -f "$ENV_FILE" ] || { echo "missing $ENV_FILE (copy .env.deploy.example)" >&2; exit 2; }
set -a; . "$ENV_FILE"; set +a
: "${DEPLOY_SSH_TARGET:?set in .env.deploy}" "${DEPLOY_REMOTE_ROOT:?set in .env.deploy}"
DIST="${DEPLOY_DIST:-dist}"; case "$DIST" in /*) ;; *) DIST="$HERE/$DIST" ;; esac
KEEP="${DEPLOY_KEEP:-5}"
# shellcheck disable=SC2206
SSH_OPTS=(-o BatchMode=yes ${DEPLOY_SSH_OPTS:-})
rssh() { ssh "${SSH_OPTS[@]}" "$DEPLOY_SSH_TARGET" "$@"; }

cmd="${1:-deploy}"; shift || true
case "$cmd" in
deploy)
  [ -f "$DIST/index.html" ] || { echo "no built site at $DIST (run the site build first)" >&2; exit 2; }
  sha="$(git -C "$HERE" rev-parse --short=12 HEAD 2>/dev/null || echo nogit)"
  name="$(date -u +%Y%m%dT%H%M%SZ)-$sha"
  stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
  cp -a "$DIST/." "$stage/"
  printf '%s\n' "$sha" > "$stage/version.txt"
  # Precompressed copies for gzip_static.
  find "$stage" -type f \( -name '*.html' -o -name '*.css' -o -name '*.js' -o -name '*.mjs' \
    -o -name '*.json' -o -name '*.svg' -o -name '*.wasm' -o -name '*.xml' -o -name '*.txt' \) \
    -size +256c ! -name '*.gz' -exec gzip -9 -k -f {} +
  # Manifest so the target can verify what arrived.
  (cd "$stage" && find . -type f ! -name MANIFEST.sha256 -print0 | sort -z | xargs -0 sha256sum > MANIFEST.sha256)
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
  ;;
*) echo "usage: $0 [deploy|rollback [NAME]|list]" >&2; exit 2 ;;
esac

if [ -n "${DEPLOY_CHECK_URL:-}" ] && [ "$cmd" != list ]; then
  code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 20 "$DEPLOY_CHECK_URL/" || true)"
  echo "check $DEPLOY_CHECK_URL -> HTTP $code"
  [ "$code" = 200 ] || { echo "site check failed; consider: $0 rollback" >&2; exit 1; }
fi
if [ -n "${DEPLOY_CONTAINER:-}" ] && [ "$cmd" != list ]; then
  rssh "docker inspect -f '{{.State.Health.Status}}' '$DEPLOY_CONTAINER'" | sed 's/^/container health: /'
fi
