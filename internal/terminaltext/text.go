package terminaltext

import (
	"fmt"
	"strings"
	"unicode"
)

// Escape keeps history commands from changing terminal state when displayed.
func Escape(value string) string {
	var text strings.Builder
	for _, r := range value {
		switch r {
		case '\n':
			text.WriteString("\\n")
		case '\r':
			text.WriteString("\\r")
		case '\t':
			text.WriteString("\\t")
		default:
			if unicode.IsControl(r) {
				fmt.Fprintf(&text, "\\u{%x}", r)
			} else {
				text.WriteRune(r)
			}
		}
	}
	return text.String()
}
