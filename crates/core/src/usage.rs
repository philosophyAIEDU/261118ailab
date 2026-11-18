//! 클라우드 AI(Gemini) 하루 사용량 기록.
//!
//! Gemini API는 남은 무료 사용량을 알려 주지 않으므로, 이 프로그램이 보낸 요청을 **이 컴퓨터에서 직접 센다**.
//! 하루 경계는 Google 무료 한도 초기화 기준인 태평양 시간(America/Los_Angeles) 자정이다.
//! 같은 API 키(프로젝트)를 다른 곳에서도 쓰면 실제 사용량은 이 숫자보다 많을 수 있다.
use crate::error::AppResult;
use crate::gemini::{default_limits, Limits};
use crate::llm::UsageEvent;
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::America::Los_Angeles;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::BTreeMap;

pub fn pacific_day(now: DateTime<Utc>) -> String {
    now.with_timezone(&Los_Angeles).format("%Y-%m-%d").to_string()
}

/// 다음 초기화 시각 (UTC)
pub fn next_reset(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&Los_Angeles);
    let tomorrow = local.date_naive().succ_opt().unwrap();
    Los_Angeles
        .from_local_datetime(&tomorrow.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now + chrono::Duration::hours(24))
}

pub fn record(conn: &Connection, ev: &UsageEvent, now: DateTime<Utc>) -> AppResult<()> {
    conn.execute(
        "INSERT INTO ai_usage(day, provider, model, requests, input_tokens, output_tokens, errors) VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(day, provider, model) DO UPDATE SET requests=requests+excluded.requests, input_tokens=input_tokens+excluded.input_tokens,
         output_tokens=output_tokens+excluded.output_tokens, errors=errors+excluded.errors",
        params![pacific_day(now), ev.provider, ev.model, ev.requests as i64, ev.input_tokens as i64, ev.output_tokens as i64, ev.error as i64],
    )?;
    Ok(())
}

/// 지금 시각 기준으로 기록
pub fn record_now(conn: &Connection, ev: &UsageEvent) -> AppResult<()> {
    record(conn, ev, Utc::now())
}

pub fn today_now(conn: &Connection, models: &[&str], limits: &BTreeMap<String, Limits>) -> AppResult<TodayUsage> {
    today(conn, models, limits, Utc::now())
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub errors: i64,
    pub limits: Limits,
    /// 하루 한도 대비 남은 요청 (추정)
    pub remaining: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayUsage {
    pub day: String,
    pub reset_at: String,
    pub models: Vec<ModelUsage>,
    /// 최근 7일 (날짜, 요청 수)
    pub history: Vec<(String, i64)>,
}

pub fn today(conn: &Connection, models: &[&str], limits: &BTreeMap<String, Limits>, now: DateTime<Utc>) -> AppResult<TodayUsage> {
    let day = pacific_day(now);
    let mut out = Vec::new();
    for m in models {
        let (r, i, o, e): (i64, i64, i64, i64) = conn
            .query_row("SELECT requests, input_tokens, output_tokens, errors FROM ai_usage WHERE day=?1 AND provider='gemini' AND model=?2", params![day, m], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap_or((0, 0, 0, 0));
        let lim = limits.get(*m).cloned().unwrap_or_else(|| default_limits(m));
        out.push(ModelUsage { model: m.to_string(), requests: r, input_tokens: i, output_tokens: o, errors: e, remaining: (lim.rpd as i64 - r).max(0), limits: lim });
    }
    let mut st = conn.prepare("SELECT day, sum(requests) FROM ai_usage WHERE provider='gemini' GROUP BY day ORDER BY day DESC LIMIT 7")?;
    let history = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    Ok(TodayUsage { day, reset_at: next_reset(now).to_rfc3339(), models: out, history })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::lib;

    #[test]
    fn pacific_day_boundary() {
        // 한국 시간 2026-11-19 16:30 = UTC 07:30 = 태평양(PST) 2026-11-18 23:30
        let t = Utc.with_ymd_and_hms(2026, 11, 19, 7, 30, 0).unwrap();
        assert_eq!(pacific_day(t), "2026-11-18");
        let t2 = Utc.with_ymd_and_hms(2026, 11, 19, 8, 30, 0).unwrap();
        assert_eq!(pacific_day(t2), "2026-11-19");
        assert_eq!(next_reset(t), Utc.with_ymd_and_hms(2026, 11, 19, 8, 0, 0).unwrap());
    }

    #[test]
    fn records_and_reports() {
        let (_d, _p, c) = lib();
        let now = Utc.with_ymd_and_hms(2026, 11, 18, 20, 0, 0).unwrap();
        let ev = |r, e| UsageEvent { provider: "gemini".into(), model: "gemini-3.8-flash".into(), requests: r, input_tokens: 100, output_tokens: 50, error: e };
        record(&c, &ev(1, false), now).unwrap();
        record(&c, &ev(2, true), now).unwrap();
        let t = today(&c, &["gemini-3.8-flash", "gemini-3.5-flash-lite"], &BTreeMap::new(), now).unwrap();
        assert_eq!(t.models[0].requests, 3);
        assert_eq!(t.models[0].errors, 1);
        assert_eq!(t.models[0].input_tokens, 200);
        assert_eq!(t.models[0].limits.rpd, 20);
        assert_eq!(t.models[0].remaining, 17);
        assert_eq!(t.models[1].requests, 0);
        assert_eq!(t.models[1].remaining, 500);
        // 다음 날은 0부터
        let t2 = today(&c, &["gemini-3.8-flash"], &BTreeMap::new(), now + chrono::Duration::days(1)).unwrap();
        assert_eq!(t2.models[0].requests, 0);
        assert_eq!(t2.history.len(), 1);
    }
}
