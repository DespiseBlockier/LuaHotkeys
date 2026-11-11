-- Run this script, then switch to a text editor and press Ctrl+Alt+J.
hotkey("Ctrl+Alt+J", function()
    type_text("Hello from LuaHotkeys!")
    press("Enter")
end)

-- Open Notepad from anywhere.
hotkey("Ctrl+Alt+N", function()
    run("notepad.exe")
end)

-- Record the pointer's coordinates in the Activity panel.
hotkey("Ctrl+Alt+Shift+M", function()
    local x, y = mouse_pos()
    log(string.format("Pointer at %d, %d", x, y))
end)
