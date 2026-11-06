# Reproduce with: bash docs/demo/record.sh (not a bare vhs invocation).
Output docs/demo/awspick.gif
Output docs/demo/awspick.mp4
Require awspick
Set Shell bash
Set Width 1100
Set Height 690
Set FontSize 21
Set FontFamily "Menlo"
Set Theme { "name": "Awspick Light", "black": "#0f172a", "red": "#b91c1c", "green": "#15803d", "yellow": "#92400e", "blue": "#1d4ed8", "magenta": "#6d28d9", "cyan": "#0e7490", "white": "#334155", "brightBlack": "#475569", "brightRed": "#b91c1c", "brightGreen": "#15803d", "brightYellow": "#92400e", "brightBlue": "#1d4ed8", "brightMagenta": "#6d28d9", "brightCyan": "#0e7490", "brightWhite": "#0f172a", "background": "#ffffff", "foreground": "#0f172a", "selection": "#dbeafe", "cursor": "#0f172a" }
Set Padding 24
Set Framerate 12
Set TypingSpeed 45ms
Set CursorBlink false
Set WindowBar Colorful
Set WindowBarSize 30
Hide
Type@1ms "PS1='$ '; source ./home/wrapper.bash; clear"
Enter
Sleep 500ms
Show
Type "awspick"
Enter
Sleep 5s
Type "q"
Enter
Sleep 500ms
Ctrl+L
Type "ap --regex -f '^acme-prod-.*readonly$'"
Enter
Sleep 3s
Type "1"
Sleep 500ms
Enter
Sleep 3s
Type 'echo "$AWS_PROFILE"'
Enter
Sleep 5s
