package search

import (
	"slices"
	"strings"
	"unicode"
	"unicode/utf8"

	"golang.org/x/text/unicode/norm"
)

type atomKind int

const (
	fuzzyAtom atomKind = iota
	substringAtom
	prefixAtom
	suffixAtom
	exactAtom
)

type atom struct {
	needle                string
	kind                  atomKind
	negative, fold, strip bool
}

func parsePattern(query string) []atom {
	var parts []string
	escaped, start := false, 0
	for i, r := range query {
		if r == ' ' && !escaped {
			parts = append(parts, query[start:i])
			start = i + 1
		}
		escaped = r == '\\'
	}
	parts = append(parts, query[start:])

	var atoms []atom
	for _, word := range parts {
		a := atom{fold: true, strip: true}
		word, a.negative = strings.CutPrefix(word, "!")
		switch {
		case strings.HasPrefix(word, "^"):
			word, a.kind = word[1:], prefixAtom
		case strings.HasPrefix(word, "'"):
			word, a.kind = word[1:], substringAtom
		case strings.HasPrefix(word, `\^`), strings.HasPrefix(word, `\'`), !a.negative && strings.HasPrefix(word, `\!`):
			word = word[1:]
		}
		if raw, ok := strings.CutSuffix(word, `\$`); ok {
			word = raw + "$"
		} else if raw, ok := strings.CutSuffix(word, "$"); ok {
			word = raw
			if a.kind == fuzzyAtom {
				a.kind = suffixAtom
			} else {
				a.kind = exactAtom
			}
		}
		if a.negative && a.kind == fuzzyAtom {
			a.kind = substringAtom
		}
		if word == "" {
			continue
		}
		word = strings.ReplaceAll(word, `\ `, " ")
		a.needle = word
		for _, r := range word {
			a.fold = a.fold && !unicode.IsUpper(r)
			a.strip = a.strip && stripAccent(r) == r && !unicode.IsMark(r)
		}
		if a.fold {
			a.needle = strings.ToLower(a.needle)
		}
		atoms = append(atoms, a)
	}
	return atoms
}

func stripAccent(r rune) rune {
	if r < utf8.RuneSelf {
		return r
	}
	d := norm.NFD.String(string(r))
	first, size := utf8.DecodeRuneInString(d)
	if size < len(d) && strings.IndexFunc(d[size:], func(r rune) bool { return !unicode.IsMark(r) }) < 0 {
		return first
	}
	return r
}

type searchable struct {
	original, text []rune
	offsets        []int
}

func searchableText(command string, a atom) searchable {
	var s searchable
	for i, r := range command {
		if a.strip && unicode.IsMark(r) {
			continue
		}
		s.offsets = append(s.offsets, i)
		s.original = append(s.original, r)
		if a.strip {
			r = stripAccent(r)
		}
		if a.fold {
			r = unicode.ToLower(r)
		}
		s.text = append(s.text, r)
	}
	return s
}

func matchPattern(pattern []atom, command string) (float64, []int, bool) {
	if len(pattern) == 0 {
		return 1, nil, true
	}
	var score int
	var indices []int
	var cache [4]searchable
	var computed [4]bool
	for _, a := range pattern {
		key := 0
		if a.fold {
			key++
		}
		if a.strip {
			key += 2
		}
		if !computed[key] {
			cache[key] = searchableText(command, a)
			computed[key] = true
		}
		s := cache[key]
		value, match := s.match(a)
		if (match != nil) == a.negative {
			return 0, nil, false
		}
		if !a.negative {
			score += value
			for _, i := range match {
				indices = append(indices, s.offsets[i])
			}
		}
	}
	slices.Sort(indices)
	return float64(score), slices.Compact(indices), true
}

func (s searchable) match(a atom) (int, []int) {
	needle := []rune(a.needle)
	if len(s.text) < len(needle) {
		return 0, nil
	}
	start, end := 0, len(s.text)
	if a.kind == prefixAtom || a.kind == exactAtom {
		if !unicode.IsSpace(needle[0]) {
			for start < end && unicode.IsSpace(s.text[start]) {
				start++
			}
		}
	}
	if a.kind == suffixAtom || a.kind == exactAtom {
		if !unicode.IsSpace(needle[len(needle)-1]) {
			for end > start && unicode.IsSpace(s.text[end-1]) {
				end--
			}
		}
	}
	if end-start < len(needle) || a.kind == exactAtom && end-start != len(needle) {
		return 0, nil
	}
	first, last := start, end-len(needle)
	switch a.kind {
	case prefixAtom, exactAtom:
		last = first
	case suffixAtom:
		first = last
	}
	var best []int
	bestScore := 0
	for i := first; i <= last; i++ {
		if s.text[i] != needle[0] || !slices.Equal(s.text[i:i+len(needle)], needle) {
			continue
		}
		positions := make([]int, len(needle))
		for j := range positions {
			positions[j] = i + j
		}
		score := s.score(positions)
		if score > bestScore {
			bestScore, best = score, positions
		}
		// A contiguous start-of-command match cannot be improved upon.
		if i == 0 {
			break
		}
	}
	if best != nil || a.kind != fuzzyAtom {
		return bestScore, best
	}
	positions := make([]int, 0, len(needle))
	for i, r := range s.text {
		if r == needle[len(positions)] {
			positions = append(positions, i)
			if len(positions) == len(needle) {
				break
			}
		}
	}
	if len(positions) != len(needle) {
		return 0, nil
	}
	// Tighten the first subsequence from its end to reduce gaps.
	k := len(positions) - 1
	for i := positions[k]; i >= 0 && k >= 0; i-- {
		if s.text[i] == needle[k] {
			positions[k] = i
			k--
		}
	}
	return s.score(positions), positions
}

func (s searchable) score(positions []int) int {
	score, previous, runBonus := 0, -1, 0
	for _, i := range positions {
		bonus := s.boundary(i)
		switch {
		case previous < 0:
			runBonus = bonus
			bonus *= 2
		case previous == i-1:
			if bonus >= 8 {
				runBonus = max(runBonus, bonus)
			}
			bonus = max(bonus, runBonus, 4)
		default:
			score = max(0, score-(i-previous+1))
			runBonus = bonus
		}
		score += 16 + bonus
		previous = i
	}
	return score
}

func (s searchable) boundary(i int) int {
	r := s.original[i]
	if unicode.IsSpace(r) || !unicode.IsLetter(r) && !unicode.IsNumber(r) && r != '/' {
		return 8
	}
	if r == '/' {
		return 0
	}
	if i == 0 || s.original[i-1] == '/' {
		return 9
	}
	previous := s.original[i-1]
	if !unicode.IsLetter(previous) && !unicode.IsNumber(previous) {
		return 8
	}
	if unicode.IsLower(previous) && unicode.IsUpper(r) || !unicode.IsNumber(previous) && unicode.IsNumber(r) {
		return 5
	}
	return 0
}
