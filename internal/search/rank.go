package search

import (
	"math"
	"sort"

	"github.com/z0mbix/fmn/internal/history"
)

type ranked struct {
	index   int
	indices []int
	score   float64
}

func rank(candidates []history.Candidate, query string, now int64) []ranked {
	pattern := parsePattern(query)
	maxScore, maxUses := 1.0, int64(1)
	var results []ranked
	for i, candidate := range candidates {
		score, indices, ok := matchPattern(pattern, candidate.Command)
		if !ok {
			continue
		}
		maxScore = max(maxScore, score)
		maxUses = max(maxUses, candidate.UseCount)
		results = append(results, ranked{i, indices, score})
	}

	for i := range results {
		item := &results[i]
		candidate := candidates[item.index]
		age := max(0, float64(now)-float64(candidate.LastSeenAt)) / 86_400_000
		item.score = 0.70*item.score/maxScore + 0.15*math.Exp2(-age/30) +
			0.10*math.Log1p(float64(candidate.UseCount))/math.Log1p(float64(maxUses))
		if candidate.CwdUses > 0 {
			item.score += 0.05
		}
	}
	sort.Slice(results, func(i, j int) bool {
		left, right := results[i], results[j]
		if left.score != right.score {
			return left.score > right.score
		}
		a, b := candidates[left.index], candidates[right.index]
		if a.LastSeenAt != b.LastSeenAt {
			return a.LastSeenAt > b.LastSeenAt
		}
		return a.ID < b.ID
	})
	return results
}
