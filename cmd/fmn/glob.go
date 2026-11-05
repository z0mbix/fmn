package main

import (
	"errors"
	"fmt"
	"regexp"
	"strings"
)

// Globset matches UTF-8 bytes, so a question mark consumes one byte.
// Go's regexp matches runes. Map each byte to a rune with the same value.
func globBytes(command string) string {
	var bytes strings.Builder
	bytes.Grow(len(command))
	for i := 0; i < len(command); i++ {
		bytes.WriteRune(rune(command[i]))
	}
	return bytes.String()
}

// compileIgnoreGlob follows globset's Unix defaults. Stars can cross slashes
// and newlines, while ** next to a slash can match zero directory components.
func compileIgnoreGlob(pattern string) (*regexp.Regexp, error) {
	const (
		recursivePrefix = "(?:/?|.*/)"
		recursiveSuffix = "/.*"
	)
	if pattern == "**" {
		return regexp.Compile("(?s)^.*$")
	}

	type group struct {
		outer []string
		parts []string
	}
	var groups []group
	var tokens []string
	runes := []rune(pattern)
	for i := 0; i < len(runes); i++ {
		var token string
		switch c := runes[i]; c {
		case '?':
			token = "."
		case '*':
			token = ".*"
			if i+1 < len(runes) && runes[i+1] == '*' {
				i++
				end := i+1 == len(runes)
				slash := !end && runes[i+1] == '/'
				if len(tokens) == 0 && (slash || end) {
					token = recursivePrefix
					if slash {
						i++
					}
				} else if i >= 2 && runes[i-2] == '/' && len(tokens) > 0 &&
					(slash || end || len(groups) > 0 && (runes[i+1] == ',' || runes[i+1] == '}')) {
					token = tokens[len(tokens)-1]
					tokens = tokens[:len(tokens)-1]
					if token != recursivePrefix && token != recursiveSuffix {
						token = recursiveSuffix
						if slash {
							token = "(?:/|/.*/)"
						}
					}
					if slash {
						i++
					}
				}
			}
		case '\\':
			i++
			if i == len(runes) {
				return nil, errors.New("dangling escape")
			}
			token = regexp.QuoteMeta(globBytes(string(runes[i])))
		case '[':
			var err error
			token, i, err = globClass(runes, i+1)
			if err != nil {
				return nil, err
			}
		case '{':
			groups = append(groups, group{outer: tokens})
			tokens = nil
			continue
		case ',':
			if len(groups) != 0 {
				group := &groups[len(groups)-1]
				if part := strings.Join(tokens, ""); part != "" {
					group.parts = append(group.parts, part)
				}
				tokens = nil
				continue
			}
			token = ","
		case '}':
			if len(groups) == 0 {
				return nil, errors.New("unexpected closing brace")
			}
			group := groups[len(groups)-1]
			groups = groups[:len(groups)-1]
			if part := strings.Join(tokens, ""); part != "" {
				group.parts = append(group.parts, part)
			}
			tokens = group.outer
			if len(group.parts) != 0 {
				token = "(?:" + strings.Join(group.parts, "|") + ")"
			}
		default:
			token = regexp.QuoteMeta(globBytes(string(c)))
		}
		tokens = append(tokens, token)
	}
	if len(groups) != 0 {
		return nil, errors.New("unclosed brace")
	}
	return regexp.Compile("(?s)^" + strings.Join(tokens, "") + "$")
}

func globClass(pattern []rune, start int) (string, int, error) {
	var class strings.Builder
	class.WriteByte('[')
	if start < len(pattern) && (pattern[start] == '!' || pattern[start] == '^') {
		class.WriteByte('^')
		start++
	}
	var ranges [][2]rune
	inRange := false
	for i := start; i < len(pattern); i++ {
		c := pattern[i]
		if c == ']' && i > start {
			if inRange {
				ranges = append(ranges, [2]rune{'-', '-'})
			}
			for _, pair := range ranges {
				writeClassRune(&class, pair[0])
				if pair[0] != pair[1] {
					class.WriteByte('-')
					writeClassRune(&class, pair[1])
				}
			}
			class.WriteByte(']')
			return class.String(), i, nil
		}
		switch {
		case inRange:
			pair := &ranges[len(ranges)-1]
			if pair[0] > c {
				return "", 0, errors.New("reversed character range")
			}
			pair[1] = c
			inRange = false
		case c == '-' && i > start:
			inRange = true
		default:
			ranges = append(ranges, [2]rune{c, c})
		}
	}
	return "", 0, errors.New("unclosed character class")
}

func writeClassRune(class *strings.Builder, r rune) {
	for _, b := range []byte(string(r)) {
		fmt.Fprintf(class, "\\x%02x", b)
	}
}
