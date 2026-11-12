
#!/bin/sh
set -eu

runner=$(realpath "$1")
work=$(mktemp -d)
server=
terminal=
macro=
cleanup() {
    for process in "$macro" "$terminal" "$server"; do
        if [ -n "$process" ]; then
            kill "$process" 2>/dev/null || :
            wait "$process" 2>/dev/null || :
        fi
    done
    rm -rf -- "$work"
}
trap cleanup EXIT
trap 'exit 1' INT TERM

await() {
    attempts=0
    until "$@"; do
        attempts=$((attempts + 1))
        if [ "$attempts" -ge 100 ]; then
            cat "$work/"*.log >&2
            return 1
        fi
        sleep 0.05
    done
}

unset WAYLAND_DISPLAY XDG_SESSION_TYPE
Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$work/display" >"$work/x11.log" 2>&1 &
server=$!
await test -s "$work/display"
export DISPLAY=":$(cat "$work/display")"

xterm -T LuaHotkeys-test -geometry 70x20+0+0 \
    -e sh -c 'stty -echo; cat > "$1"' sh "$work/input" >"$work/xterm.log" 2>&1 &
terminal=$!
has_terminal() {
    xdotool search --onlyvisible --name '^LuaHotkeys-test$' >"$work/window"
}
await has_terminal
xdotool windowfocus --sync "$(head -n 1 "$work/window")"

cat >"$work/test.lhk" <<'LUA'
type TestShortcut = Combo
local shortcut: TestShortcut = "Ctrl+Alt+J"
hotkey(shortcut, function()
    move_mouse(70, 80)
    local x, y = mouse_pos()
    assert(x == 70 and y == 80)
    click("left")
    type_text("LuaHotkeys Linux!")
    press("Enter")
    log("callback ran")
end)
LUA
"$runner" --run "$work/test.lhk" >"$work/macro.log" 2>&1 &
macro=$!
await grep -q '^Bound Ctrl+Alt+J$' "$work/macro.log"

if timeout 5 "$runner" --run "$work/test.lhk" >"$work/conflict.log" 2>&1; then
    echo 'Expected an error for a shortcut registered by another process.' >&2
    exit 1
fi
grep -q "Couldn't register Ctrl+Alt+J" "$work/conflict.log"

xdotool key ctrl+alt+j
await grep -q '^callback ran$' "$work/macro.log"
await grep -q '^LuaHotkeys Linux!$' "$work/input"
printf '%s\n' 'X11 shortcut, typed text, keypress, click, and pointer checks passed.'
