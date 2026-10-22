use rusqlite::params;

use super::{Store, map_entry};
use crate::Result;
use crate::model::{CommandStats, Entry, HistoryStats, SearchCandidate};
use crate::scope::{SearchContext, SearchScope};

impl Store {
    pub fn scoped_search_candidates(
        &self,
        context: &SearchContext,
        scope: SearchScope,
    ) -> Result<Vec<SearchCandidate>> {
        if scope == SearchScope::Global {
            return self.search_candidates(&context.cwd);
        }
        let (filter, value, physical) = context.filter(scope);
        let sql = format!(
            "WITH scoped AS (
                SELECT e.* FROM entries e WHERE {filter}
             ), summary AS (
                SELECT command_id, MIN(started_at) first_seen, MAX(started_at) last_seen,
                       COUNT(*) uses, SUM(CASE WHEN cwd = ?3 THEN 1 ELSE 0 END) cwd_uses,
                       MAX(CASE WHEN cwd = ?3 THEN started_at END) cwd_last
                FROM scoped GROUP BY command_id
             )
             SELECT c.id, c.command, s.first_seen, s.last_seen, s.uses,
                    s.cwd_uses, s.cwd_last, newest.cwd, newest.exit_code
             FROM summary s JOIN commands c ON c.id = s.command_id
             JOIN scoped newest ON newest.id = (
                 SELECT id FROM scoped WHERE command_id = c.id
                 ORDER BY started_at DESC, id DESC LIMIT 1
             ) ORDER BY s.last_seen DESC, c.id DESC"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params![value, physical, context.cwd], |row| {
            Ok(SearchCandidate {
                id: row.get(0)?,
                command: row.get(1)?,
                first_seen_at: row.get(2)?,
                last_seen_at: row.get(3)?,
                use_count: row.get(4)?,
                cwd_use_count: row.get(5)?,
                cwd_last_seen_at: row.get(6)?,
                latest_cwd: row.get(7)?,
                latest_exit_code: row.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn scoped_entries_for_command(
        &self,
        command_id: i64,
        limit: i64,
        context: &SearchContext,
        scope: SearchScope,
    ) -> Result<Vec<Entry>> {
        let (filter, value, physical) = context.filter(scope);
        let sql = format!(
            "SELECT e.id, e.command_id, c.command, e.started_at, e.finished_at,
                e.duration_ms, e.exit_code, e.cwd, e.shell, e.session_id, e.source
             FROM entries e JOIN commands c ON c.id = e.command_id
             WHERE {filter} AND e.command_id = ?3
             ORDER BY e.started_at DESC, e.id DESC LIMIT ?4"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params![value, physical, command_id, limit], map_entry)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn history_stats(
        &self,
        context: &SearchContext,
        scope: SearchScope,
        now_millis: i64,
    ) -> Result<HistoryStats> {
        // Release this read snapshot before returning to the interactive UI so
        // open statistics never hold up capture or deletion/WAL cleanup.
        let transaction = self.connection.unchecked_transaction()?;
        let (filter, value, physical) = context.filter(scope);
        let mut stats = transaction.query_row(
            &format!(
                "SELECT COUNT(*), COUNT(DISTINCT e.command_id),
                COUNT(e.exit_code), COALESCE(SUM(e.exit_code != 0), 0), AVG(e.duration_ms)
             FROM entries e WHERE {filter}"
            ),
            params![value, physical],
            |row| {
                Ok(HistoryStats {
                    total: row.get(0)?,
                    unique: row.get(1)?,
                    completed: row.get(2)?,
                    failed: row.get(3)?,
                    average_duration_ms: row.get(4)?,
                    ..HistoryStats::default()
                })
            },
        )?;
        const DAY: i64 = 86_400_000;
        let today = now_millis.div_euclid(DAY);
        let first_day = today.saturating_sub(29);
        {
            let mut statement = transaction.prepare(&format!("SELECT
                    e.started_at / 86400000 - (e.started_at < 0 AND e.started_at % 86400000 != 0), COUNT(*)
                 FROM entries e WHERE {filter} AND e.started_at >= ?3 AND e.started_at < ?4
                 GROUP BY 1 ORDER BY 1"))?;
            let rows = statement.query_map(
                params![
                    value,
                    physical,
                    first_day.saturating_mul(DAY),
                    today.saturating_add(1).saturating_mul(DAY)
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )?;
            for row in rows {
                let (day, count) = row?;
                if let Ok(index) = usize::try_from(day.saturating_sub(first_day))
                    && let Some(slot) = stats.activity.get_mut(index)
                {
                    *slot = count as u64;
                }
            }
        }
        for (order, target) in [
            (
                "COUNT(*) DESC, MAX(e.started_at) DESC, c.command",
                &mut stats.top_commands,
            ),
            (
                "failures DESC, COUNT(*) DESC, c.command",
                &mut stats.failed_commands,
            ),
            (
                "average_duration DESC, COUNT(*) DESC, c.command",
                &mut stats.slow_commands,
            ),
        ] {
            let having = if order.starts_with("failures") {
                "HAVING failures > 0"
            } else if order.starts_with("average") {
                "HAVING average_duration IS NOT NULL"
            } else {
                ""
            };
            let mut statement = transaction.prepare(&format!(
                "SELECT c.command, COUNT(*),
                    COALESCE(SUM(e.exit_code != 0), 0) failures, AVG(e.duration_ms) average_duration
                 FROM entries e JOIN commands c ON c.id = e.command_id
                 WHERE {filter} GROUP BY c.id {having} ORDER BY {order} LIMIT 5"
            ))?;
            let rows = statement.query_map(params![value, physical], |row| {
                Ok(CommandStats {
                    command: row.get(0)?,
                    count: row.get(1)?,
                    failures: row.get(2)?,
                    average_duration_ms: row.get(3)?,
                })
            })?;
            *target = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        }
        transaction.commit()?;
        Ok(stats)
    }
}
