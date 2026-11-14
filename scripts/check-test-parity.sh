#!/usr/bin/env bash
# The UI tests and screenshot previews must use the same libraries, with the
# same features, as the real app. Compares the two builds and fails on any
# difference, except accesskit: the test framework needs it to find widgets
# by their labels, and it doesn't change what is drawn.
#
# Checked for macOS, where the tests and previews run (here and on GitHub).
# On Linux they couldn't match anyway: the app draws with OpenGL (glow), the
# test framework only with wgpu.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || source "$HOME/.cargo/env"

# "name version [features]" for every library. Features that only accesskit
# switches on are left out: its own, and on macOS its bridge to AppKit's
# accessibility API (NSAccessibility…, NSText).
libraries() {
    cargo tree --target "$1" -e "$2" -f '{p} [{f}]' --prefix none |
        sed -E 's/ \(\*\)//' |
        awk 'NF == 0 { next }  # the blank line between the two crate trees
        {
            n = split(substr($0, index($0, "[") + 1), parts, /[],]/)
            kept = ""
            for (i = 1; i <= n; i++) {
                f = parts[i]
                if (f == "" || f == "accesskit" || f ~ /^NSAccessibility/ || f == "NSText") continue
                kept = kept (kept == "" ? "" : ",") f
            }
            print $1, $2, "[" kept "]"
        }' | sort -u
}

status=0
for target in aarch64-apple-darwin; do
    app=$(libraries "$target" normal)
    # The same libraries in the test build (which has test-only ones besides).
    names=$(cut -d' ' -f1,2 <<<"$app")
    test=$(libraries "$target" normal,dev | while read -r name version features; do
        if grep -qxF "$name $version" <<<"$names"; then echo "$name $version $features"; fi
    done)
    if [[ "$app" != "$test" ]]; then
        echo "On $target the tests build these differently (< app, > tests):"
        diff <(echo "$app") <(echo "$test") | grep '^[<>]' | sed 's/^/  /'
        status=1
    fi
done
if [[ $status == 0 ]]; then
    echo "Tests use the same libraries and features as the app."
fi
exit $status
