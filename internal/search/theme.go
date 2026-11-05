package search

import (
	"image/color"
	"sort"
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

type theme struct {
	name, label string
	palette     palette
}

type palette struct {
	background, panel, text, muted, border, accent   color.Color
	selected, selectionText, success, failure, match color.Color
}

func rgb(v uint32) color.Color {
	return color.RGBA{uint8(v >> 16), uint8(v >> 8), uint8(v), 255}
}

func rgbPalette(values [9]uint32, selection, match uint32) palette {
	if selection == 0 {
		selection = values[2]
	}
	if match == 0 {
		match = values[5]
	}
	return palette{
		background: rgb(values[0]), panel: rgb(values[1]), text: rgb(values[2]),
		muted: rgb(values[3]), border: rgb(values[4]), accent: rgb(values[5]),
		selected: rgb(values[6]), selectionText: rgb(selection), success: rgb(values[7]),
		failure: rgb(values[8]), match: rgb(match),
	}
}

var terminalPalette = palette{
	muted: ansi.BasicColor(7), border: ansi.BasicColor(8), accent: ansi.BasicColor(6),
	selected: ansi.BasicColor(4), success: ansi.BasicColor(2), failure: ansi.BasicColor(1),
	match: ansi.BasicColor(6),
}

func (p palette) selection() palette {
	p.background = p.selected
	if p.selectionText != p.text {
		p.text, p.muted, p.accent = p.selectionText, p.selectionText, p.selectionText
		p.match, p.success, p.failure = p.selectionText, p.selectionText, p.selectionText
	}
	return p
}

func (p palette) style(fg color.Color) lipgloss.Style {
	return lipgloss.NewStyle().Foreground(fg).Background(p.background)
}

func themeIndex(name string) int {
	if name == "" {
		return 0
	}
	for i, item := range themes {
		if item.name == name {
			return i
		}
	}
	return -1
}

func ValidTheme(name string) bool { return name != "" && themeIndex(name) >= 0 }

type themeMatch struct {
	index   int
	indices []int
	score   float64
}

func matchThemes(query string) []themeMatch {
	query = strings.ReplaceAll(strings.ToLower(strings.TrimSpace(query)), "-", " ")
	pattern := parsePattern(query)
	var matches []themeMatch
	for index, item := range themes {
		score, indices, ok := matchPattern(pattern, item.label)
		configName := strings.ReplaceAll(item.name, "-", " ")
		words := strings.Fields(configName)
		var initials strings.Builder
		for _, word := range words {
			initials.WriteByte(word[0])
		}
		if query == configName || query == initials.String() {
			score += 10000
			ok = true
		}
		if ok {
			matches = append(matches, themeMatch{index, indices, score})
		}
	}
	sort.SliceStable(matches, func(i, j int) bool { return matches[i].score > matches[j].score })
	return matches
}
