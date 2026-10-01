#!/bin/sh
# devdraw wrapper for p9draw capture (install as $PLAN9/bin/devdraw).
#
# Install (PLAN9 = plan9port root):
#   cargo build --release -p p9draw-server
#   cp target/release/p9draw-server $PLAN9/bin/   # or: export P9DRAW_SERVER=...
#   cp $PLAN9/bin/devdraw $PLAN9/bin/devdraw.real
#   cp scripts/devdraw-tee.sh $PLAN9/bin/devdraw
#   chmod +x $PLAN9/bin/devdraw
#
# How plan9port clients reach devdraw (SPEC.md section 2.1, docs/research.md):
#   * legacy (default, no $wsysid): the CLIENT pipe()+fork()s, dup2()s the
#     pipe onto fds 0/1 and execl($DEVDRAW || "devdraw", ...). There is no
#     9pserve/post9pservice in this tree, so wrapping THIS binary puts the
#     MITM exactly on that pipe (capture-pipe below). Keep $DEVDRAW unset
#     (or pointing here), otherwise the client bypasses the wrapper.
#   * server mode ($wsysid="name/id"): the client dials
#     unix!$NAMESPACE/name DIRECTLY. The wrapper only sees that traffic
#     when the session manager starts `devdraw -s NAME` through it; the
#     branch below then listens on $NAMESPACE/NAME and forwards raw bytes
#     to the real server on $NAMESPACE/NAME.cap.
#
# Behavior:
#   P9DRAW_CAPTURE unset or 0 -> `exec devdraw.real "$@"` (zero change).
#   P9DRAW_CAPTURE=1          -> MITM; dumps+log in $P9DRAW_CAPTURE_DIR
#                                (default ${TMPDIR:-/tmp}/p9draw-capture-$$).
#
# OPEN (SPEC.md section 8, OPEN-1): nobody inside plan9port sets $wsysid;
# the lifecycle of `devdraw -s` (who starts it, when it exits) belongs to
# an external session manager. The -s branch assumes (a) the NAME in argv
# is the socket name the client will dial and (b) an extra NAME.cap socket
# in the namespace is acceptable. If either assumption is wrong, use
# passthrough (leave P9DRAW_CAPTURE unset) -- it always works. Set
# $NAMESPACE explicitly if the automatic /tmp/ns.<user>.<display> guess
# may disagree with the server's own getns().
#
# Only the literal `devdraw -s NAME ...` form is intercepted; other flag
# orders (e.g. `devdraw -x -s NAME`) fall through to the legacy branch.

set -u

DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 1
REAL="$DIR/devdraw.real"

# Passthrough: nothing here may alter behavior, not even diagnostics.
if [ "${P9DRAW_CAPTURE:-0}" != "1" ]; then
    exec "$REAL" "$@"
fi

die() {
    echo "devdraw wrapper: $*" >&2
    exit 1
}

[ -x "$REAL" ] || die "real binary missing: $REAL (see install steps in the header)"

SERVER="${P9DRAW_SERVER:-}"
if [ -z "$SERVER" ]; then
    SERVER="$DIR/p9draw-server"
fi
if [ ! -x "$SERVER" ]; then
    SERVER=$(command -v p9draw-server 2>/dev/null) || true
fi
if [ -z "${SERVER:-}" ] || [ ! -x "$SERVER" ]; then
    die "p9draw-server not found; build it and set P9DRAW_SERVER"
fi

CAPDIR="${P9DRAW_CAPTURE_DIR:-${TMPDIR:-/tmp}/p9draw-capture-$$}"
mkdir -p "$CAPDIR" || die "cannot create capture dir: $CAPDIR"
echo "devdraw wrapper: capture mode, artifacts in $CAPDIR" >&2

# --- server mode: we were started as `devdraw -s NAME ...` ---------------
if [ "${1:-}" = "-s" ] && [ "$#" -ge 2 ]; then
    NAME=$2
    shift 2

    NS_USER=$(id -un 2>/dev/null) || NS_USER=unknown
    NS_DISP=$(printf '%s' "${DISPLAY:-:0.0}" | sed -e 's/\.[0-9][0-9]*$//' -e 's,/,_,g')
    NS="${NAMESPACE:-/tmp/ns.$NS_USER.$NS_DISP}"
    LISTEN="$NS/$NAME"
    UPSTREAM="$NS/$NAME.cap"

    CAP_PID=
    REAL_PID=
    cleanup() {
        status=$?
        [ -n "$REAL_PID" ] && kill "$REAL_PID" 2>/dev/null
        [ -n "$CAP_PID" ] && kill "$CAP_PID" 2>/dev/null
        rm -f "$LISTEN" "$UPSTREAM"
        exit "$status"
    }
    trap cleanup INT TERM EXIT

    "$SERVER" capture \
        --listen "$LISTEN" \
        --upstream "$UPSTREAM" \
        --dump-dir "$CAPDIR" \
        --log-file "$CAPDIR/capture.log" &
    CAP_PID=$!

    # The client may dial the moment the socket file appears.
    i=0
    while [ ! -S "$LISTEN" ] && [ "$i" -lt 200 ]; do
        sleep 0.05
        i=$((i + 1))
    done
    [ -S "$LISTEN" ] || die "capture socket $LISTEN did not appear"

    "$REAL" -s "$NAME.cap" "$@" &
    REAL_PID=$!
    wait "$REAL_PID"
    status=$?
    kill "$CAP_PID" 2>/dev/null
    trap - EXIT INT TERM
    rm -f "$LISTEN" "$UPSTREAM"
    exit "$status"
fi

# --- legacy mode: the client talks to us over stdin/stdout pipes ----------
# capture-pipe spawns devdraw.real behind our stdio; nothing in capture
# mode may write to stdout: it carries the protocol bytes.
exec "$SERVER" capture-pipe \
    --dump-dir "$CAPDIR" \
    --log-file "$CAPDIR/capture.log" \
    -- "$REAL" "$@"
