package main

import "testing"

func TestIgnoreGlobCompatibility(t *testing.T) {
	cases := []struct {
		pattern, command string
		match            bool
	}{
		{"secret *", "secret /root/value\nsecond line", true},
		{"secret *", "echo secret value", false},
		{"**/secret", "secret", true},
		{"**/secret", "/root/secret", true},
		{"**/secret", "nosecret", false},
		{"a/**/secret", "a/secret", true},
		{"a/**/secret", "a/b/c/secret", true},
		{"a/**", "a", false},
		{"**/**/secret", "secret", true},
		{"{do,{hide,private}} *", "private stuff", true},
		{"private{,x}", "private", false},
		{"s[!a-z]cret", "s3cret", true},
		{"s[^a-z]cret", "secret", false},
		{"[a-b-f]", "d", true},
		{"[]-]", "-", true},
		{"echo \\*", "echo *", true},
		{"echo ?", "echo é", false},
		{"echo ??", "echo é", true},
		{"echo café", "echo café", true},
	}
	for _, tc := range cases {
		matcher, err := compileIgnoreGlob(tc.pattern)
		if err != nil {
			t.Fatalf("compile %q: %v", tc.pattern, err)
		}
		if match := matcher.MatchString(globBytes(tc.command)); match != tc.match {
			t.Errorf("%q matching %q = %v, want %v", tc.pattern, tc.command, match, tc.match)
		}
	}
}

func TestInvalidIgnoreGlobDisablesCapture(t *testing.T) {
	var cfg config
	cfg.History.IgnorePatterns = []string{"secret *", "["}
	if record, err := cfg.shouldRecord("echo ok"); record || err == nil {
		t.Fatalf("shouldRecord = %v, %v; want false and a pattern error", record, err)
	}
}
