-- Scrivener AX helpers for the "Weave UX" research window.
-- Usage:
--   osascript tools/scrivener_ax.applescript rows
--   osascript tools/scrivener_ax.applescript select <row-index-1-based>
--   osascript tools/scrivener_ax.applescript selected
--   osascript tools/scrivener_ax.applescript menu <MenuName> <ItemName>
--   osascript tools/scrivener_ax.applescript tabtitle

on run argv
	if (count of argv) is 0 then return "no command"
	set cmd to item 1 of argv

	if cmd is "rows" then
		tell application "System Events"
			tell process "Scrivener"
				set elems to entire contents of window 1
				set out to ""
				set i to 0
				repeat with e in elems
					set i to i + 1
					try
						if role of e is "AXRow" then
							set nm to ""
							repeat with c in UI elements of e
								try
									if role of c is "AXCell" then
										repeat with tf in UI elements of c
											if role of tf is "AXTextField" then
												set nm to value of tf
											end if
										end repeat
									end if
								end try
							end repeat
							set sel to " "
							try
								set sel to (value of attribute "AXSelected" of e) as text
							on error
								set sel to "?"
							end try
							set out to out & i & "=" & nm & "[" & sel & "] | "
						end if
					end try
				end repeat
				return out
			end tell
		end tell
	else if cmd is "select" then
		set targetIdx to (item 2 of argv) as integer
		tell application "System Events"
			tell process "Scrivener"
				set elems to entire contents of window 1
				set i to 0
				repeat with e in elems
					set i to i + 1
					if i is targetIdx then
						set value of attribute "AXSelected" of e to true
						return "selected element " & i
					end if
				end repeat
				return "index not found"
			end tell
		end tell
	else if cmd is "selected" then
		tell application "System Events"
			tell process "Scrivener"
				set elems to entire contents of window 1
				set out to ""
				repeat with e in elems
					try
						if role of e is "AXRow" then
							if (value of attribute "AXSelected" of e) is true then
								repeat with c in UI elements of e
									try
										if role of c is "AXCell" then
											repeat with tf in UI elements of c
												if role of tf is "AXTextField" then
													set out to out & (value of tf) & " | "
												end if
											end repeat
										end if
									end try
								end repeat
							end if
						end if
					end try
				end repeat
				return out
			end tell
		end tell
	else if cmd is "menu" then
		tell application "System Events"
			tell process "Scrivener"
				click menu item (item 3 of argv) of menu 1 of menu bar item (item 2 of argv) of menu bar 1
				return "clicked " & (item 2 of argv) & " > " & (item 3 of argv)
			end tell
		end tell
	else if cmd is "tabtitle" then
		tell application "System Events"
			tell process "Scrivener"
				set elems to entire contents of window 1
				repeat with e in elems
					try
						if role of e is "AXStaticText" then
							return value of e
						end if
					end try
				end repeat
				return "(no static text)"
			end tell
		end tell
	else
		return "unknown command: " & cmd
	end if
end run
