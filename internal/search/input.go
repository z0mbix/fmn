package search

import (
	"strings"
	"unicode"
	"unicode/utf8"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"github.com/clipperhouse/uax29/v2/graphemes"
)

func updateInput(input textinput.Model, msg tea.Msg) (textinput.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.PasteMsg:
		// A paste is text even if it contains '?' or a shortcut.
		pasted := strings.Map(func(r rune) rune {
			if unicode.IsSpace(r) {
				return ' '
			}
			if unicode.IsControl(r) {
				return -1
			}
			return r
		}, msg.Content)
		before, after := splitInput(input)
		input.SetValue(before + pasted + after)
		input.SetCursor(utf8.RuneCountInString(before + pasted))
		return input, nil
	case tea.KeyPressMsg:
		switch msg.String() {
		case "left", "right", "backspace", "delete":
			return graphemeEdit(input, msg.String()), nil
		}
	}
	return input.Update(msg)
}

func splitInput(input textinput.Model) (string, string) {
	value := []rune(input.Value())
	position := min(input.Position(), len(value))
	return string(value[:position]), string(value[position:])
}

func graphemeEdit(input textinput.Model, key string) textinput.Model {
	before, after := splitInput(input)
	if key == "left" || key == "backspace" {
		iterator := graphemes.FromString(before)
		last := 0
		for iterator.Next() {
			last = iterator.Start()
		}
		if key == "backspace" {
			input.SetValue(before[:last] + after)
		}
		input.SetCursor(utf8.RuneCountInString(before[:last]))
	} else {
		iterator := graphemes.FromString(after)
		if iterator.Next() {
			cluster := iterator.Value()
			if key == "delete" {
				input.SetValue(before + after[len(cluster):])
			} else {
				input.SetCursor(utf8.RuneCountInString(before + cluster))
			}
		}
	}
	return input
}
