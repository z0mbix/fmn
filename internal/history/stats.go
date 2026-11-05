package history

import (
	"context"
	"fmt"
	"time"
)

type Stats struct {
	Total, Unique, Completed, Failed int64
	Average                          *float64
	Activity                         [30]int64
	Top, Failures, Slow              []CommandStats
}

type CommandStats struct {
	Command         string
	Count, Failures int64
	Average         *float64
}

func (s *Store) Stats(c SearchContext, scope Scope, now int64) (Stats, error) {
	var stats Stats
	ctx := context.Background()
	filter, args := c.predicate(scope)
	// Read all aggregates from one consistent snapshot without holding the
	// writer lock that capture transactions acquire.
	conn, err := s.db.Conn(ctx)
	if err != nil {
		return stats, err
	}
	defer conn.Close()
	if _, err := conn.ExecContext(ctx, "BEGIN DEFERRED"); err != nil {
		return stats, err
	}
	defer conn.ExecContext(ctx, "ROLLBACK")
	query := func(sql string, extra ...any) string {
		return fmt.Sprintf(sql, append([]any{filter}, extra...)...)
	}
	err = conn.QueryRowContext(ctx, query(`
		SELECT COUNT(*), COUNT(DISTINCT command_id), COUNT(exit_code),
		       COALESCE(SUM(exit_code != 0), 0), AVG(duration_ms)
		FROM entries WHERE %s`), args...).Scan(&stats.Total, &stats.Unique, &stats.Completed, &stats.Failed, &stats.Average)
	if err != nil {
		return stats, err
	}

	const day = int64(24 * time.Hour / time.Millisecond)
	today := time.UnixMilli(now).UTC().Truncate(24 * time.Hour).UnixMilli()
	start := today - 29*day
	activity := fmt.Sprintf(`
		SELECT (started_at - ?%d) / 86400000, COUNT(*)
		FROM entries WHERE %s AND started_at >= ?%d AND started_at < ?%d GROUP BY 1`,
		len(args)+1, filter, len(args)+1, len(args)+2)
	rows, err := conn.QueryContext(ctx, activity, append(args, start, today+day)...)
	if err != nil {
		return stats, err
	}
	for rows.Next() {
		var index, count int64
		if err := rows.Scan(&index, &count); err != nil {
			rows.Close()
			return stats, err
		}
		stats.Activity[index] = count
	}
	if err := rows.Err(); err != nil {
		rows.Close()
		return stats, err
	}
	rows.Close()

	for _, report := range []struct {
		target        *[]CommandStats
		having, order string
	}{
		{&stats.Top, "", "COUNT(*) DESC, MAX(started_at) DESC, c.command"},
		{&stats.Failures, "HAVING failures > 0", "failures DESC, COUNT(*) DESC, c.command"},
		{&stats.Slow, "HAVING average IS NOT NULL", "average DESC, COUNT(*) DESC, c.command"},
	} {
		list := query(`
			SELECT c.command, COUNT(*), COALESCE(SUM(exit_code != 0), 0) failures, AVG(duration_ms) average
			FROM entries e JOIN commands c ON c.id = e.command_id WHERE %s
			GROUP BY c.id %s ORDER BY %s LIMIT 5`, report.having, report.order)
		rows, err := conn.QueryContext(ctx, list, args...)
		if err != nil {
			return stats, err
		}
		for rows.Next() {
			var item CommandStats
			if err := rows.Scan(&item.Command, &item.Count, &item.Failures, &item.Average); err != nil {
				rows.Close()
				return stats, err
			}
			*report.target = append(*report.target, item)
		}
		if err := rows.Err(); err != nil {
			rows.Close()
			return stats, err
		}
		rows.Close()
	}
	return stats, nil
}
