package importer

import (
	"slices"
	"testing"
	"unicode/utf8"
)

func FuzzNativeHistory(f *testing.F) {
	f.Add("#123\necho ok\n", uint8(0))
	f.Add(": 1:2;echo ok\\\nnext\n", uint8(1))
	f.Add("- cmd: printf a\\nb\n  when: 123\n", uint8(2))
	f.Fuzz(func(t *testing.T, input string, shell uint8) {
		if !utf8.ValidString(input) {
			t.Skip()
		}
		source := []string{"bash", "zsh", "fish"}[int(shell)%3]
		result, err := Parse(source, input)
		if err != nil {
			t.Fatal(err)
		}
		var keys []string
		for _, record := range result.Records {
			if len(record.Key) != 64 || !utf8.ValidString(record.Command) {
				t.Fatalf("invalid imported record: %+v", record)
			}
			keys = append(keys, record.Key)
		}
		slices.Sort(keys)
		if len(keys) != len(slices.Compact(keys)) {
			t.Fatal("distinct occurrences must have distinct import keys")
		}
	})
}
