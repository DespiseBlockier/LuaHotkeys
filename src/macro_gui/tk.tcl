namespace eval lhk {
    variable values
    variable updating 0
    variable capturing -1

    proc decode {value} {
        encoding convertfrom utf-8 [binary decode base64 $value]
    }

    proc encode {value} {
        binary encode base64 -maxlen 0 [encoding convertto utf-8 $value]
    }

    proc send {event args} {
        puts [join [linsert $args 0 $event] " "]
        flush stdout
    }

    proc title {value} {
        wm title . [decode $value]
    }

    proc visible {value} {
        if {$value} {
            wm deiconify .
            raise .
        } else {
            wm withdraw .
        }
    }

    proc changed {kind id args} {
        variable values
        variable updating
        if {$updating} { return }
        set value $values($id)
        if {$kind eq "text"} { set value [encode $value] }
        send $kind $id $value
    }

    proc add {kind id label args} {
        variable values
        set widget .canvas.content.w$id
        set label [decode $label]
        switch -- $kind {
            label { ttk::label $widget -text $label }
            button {
                ttk::button $widget -text $label -command [list lhk::send pressed $id]
                bind $widget <Enter> [list lhk::send hovered $id]
                bind $widget <Leave> [list lhk::send unhovered $id]
            }
            slider {
                lassign $args min max values($id)
                scale $widget -label $label -from $min -to $max -resolution 1 \
                    -orient horizontal -highlightthickness 0 -variable ::lhk::values($id)
                trace add variable values($id) write [list lhk::changed number $id]
            }
            toggle {
                set values($id) [lindex $args 0]
                ttk::checkbutton $widget -text $label -variable ::lhk::values($id)
                trace add variable values($id) write [list lhk::changed boolean $id]
            }
            input {
                set values($id) [decode [lindex $args 0]]
                ttk::label $widget-label -text $label
                pack $widget-label -fill x -pady {4 0}
                ttk::entry $widget -textvariable ::lhk::values($id)
                trace add variable values($id) write [list lhk::changed text $id]
            }
            keybind {
                ttk::button $widget -text $label -command [list lhk::send record $id]
                bind $widget <KeyPress> [list lhk::key $id %k %s key]
                bind $widget <KeyRelease> [list lhk::key $id %k %s release]
                bind $widget <FocusOut> [list lhk::unfocus $id %m]
            }
        }
        pack $widget -fill x -pady 4
    }

    proc set_value {kind id value} {
        variable values
        variable updating
        if {$kind eq "label"} {
            .canvas.content.w$id configure -text [decode $value]
            return
        }
        set updating 1
        try {
            if {$kind eq "text"} { set value [decode $value] }
            set values($id) $value
        } finally {
            set updating 0
        }
    }

    proc set_keybind {id caption recording} {
        variable capturing
        set widget .canvas.content.w$id
        $widget configure -text [decode $caption]
        if {$recording} {
            set capturing $id
            focus $widget
        } elseif {$capturing == $id} {
            set capturing -1
        }
    }

    proc key {id code modifiers event} {
        variable capturing
        if {$capturing == $id} {
            send $event $id $code $modifiers
            return -code break
        }
    }

    proc unfocus {id mode} {
        variable capturing
        if {$capturing == $id && $mode ni {NotifyGrab NotifyUngrab}} { send cancel $id }
    }

    proc layout {} {
        update idletasks
        set height [expr {max(130, min(600, [winfo reqheight .canvas.content]))}]
        wm geometry . 460x$height
    }

    proc receive {} {
        while {[gets stdin line] >= 0} {
            if {[catch {{*}$line} message]} {
                send error [encode $message]
                exit 1
            }
        }
        if {[eof stdin]} { exit }
    }
}

wm withdraw .
wm minsize . 240 110
wm protocol . WM_DELETE_WINDOW {lhk::visible 0; lhk::send close}
canvas .canvas -highlightthickness 0 -yscrollcommand {.scroll set}
ttk::scrollbar .scroll -orient vertical -command {.canvas yview}
ttk::frame .canvas.content -padding 12
.canvas create window 0 0 -anchor nw -window .canvas.content -tags content
pack .scroll -side right -fill y
pack .canvas -side left -fill both -expand 1
bind .canvas <Configure> {.canvas itemconfigure content -width %w}
bind .canvas.content <Configure> {.canvas configure -scrollregion [.canvas bbox content]}
bind . <Button-4> {.canvas yview scroll -2 units}
bind . <Button-5> {.canvas yview scroll 2 units}
fconfigure stdin -blocking 0 -translation lf -encoding utf-8
fconfigure stdout -translation lf -encoding utf-8 -buffering line
fileevent stdin readable lhk::receive
lhk::send ready
