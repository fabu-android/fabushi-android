use std::collections::BTreeSet;
use chrono::{Datelike, Local, TimeZone, Timelike, Utc};
use chrono_tz::Tz;

pub const MAX_CRON_SEARCH_MINUTES:i64=366*24*60;
const MINUTE_MS:i64=60_000;
#[derive(Debug,Clone,PartialEq,Eq)]pub struct CronMatcher{minute:BTreeSet<u32>,hour:BTreeSet<u32>,day_of_month:BTreeSet<u32>,month:BTreeSet<u32>,day_of_week:BTreeSet<u32>,day_of_month_restricted:bool,day_of_week_restricted:bool,time_zone:Option<Tz>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]struct WallClock{minute:u32,hour:u32,month:u32,day_of_month:u32,day_of_week:u32}
pub fn normalize_schedule(raw:&str)->String{raw.split_whitespace().collect::<Vec<_>>().join(" ")}
pub fn parse_every_interval_ms(schedule:&str)->Option<i64>{let mut p=schedule.trim().split_whitespace();if p.next()?.to_ascii_lowercase()!="@every"{return None;}let token=p.next()?;if p.next().is_some(){return None;}let split=token.find(|c:char|!c.is_ascii_digit())?;let amount=token[..split].parse::<i64>().ok()?;if amount<=0{return None;}let unit=match token[split..].to_ascii_lowercase().as_str(){"s"=>1_000,"m"=>60_000,"h"=>3_600_000,"d"=>86_400_000,_=>return None};amount.checked_mul(unit)}
fn split_zone(schedule:&str)->(String,Option<String>){let n=normalize_schedule(schedule);for prefix in ["CRON_TZ=","TZ="]{if let Some(rest)=n.strip_prefix(prefix){if let Some((z,s))=rest.split_once(' '){return(s.to_string(),Some(z.to_string()));}}}(n,None)}
fn alias(schedule:&str)->&str{match schedule.to_ascii_lowercase().as_str(){"@hourly"=>"0 * * * *","@daily"|"@midnight"=>"0 0 * * *","@weekly"=>"0 0 * * 0","@monthly"=>"0 0 1 * *","@yearly"|"@annually"=>"0 0 1 1 *",_=>schedule}}
fn field(raw:&str,min:u32,max:u32)->Option<BTreeSet<u32>>{let mut out=BTreeSet::new();for part in raw.split(','){let p=part.split('/').collect::<Vec<_>>();if p.len()>2{return None;}let step=if p.len()==2{p[1].parse::<u32>().ok()?}else{1};if step==0{return None;}let range=p[0];let(start,end)=if range.is_empty()||range=="*"{(min,max)}else if let Some((a,b))=range.split_once('-'){(a.parse().ok()?,b.parse().ok()?)}else{let a=range.parse::<u32>().ok()?;(a,if p.len()==2{max}else{a})};if start<min||end>max||start>end{return None;}let mut v=start;while v<=end{out.insert(v);let Some(n)=v.checked_add(step)else{break};if n<=v{break}v=n;}}(!out.is_empty()).then_some(out)}
pub fn compile_cron_matcher(schedule:&str)->Option<CronMatcher>{let(s,z)=split_zone(schedule);let e=alias(&s);let f=e.split_whitespace().collect::<Vec<_>>();if f.len()!=5{return None;}let minute=field(f[0],0,59)?;let hour=field(f[1],0,23)?;let day_of_month=field(f[2],1,31)?;let month=field(f[3],1,12)?;let day_of_week=field(f[4],0,7)?.into_iter().map(|d|if d==7{0}else{d}).collect();let time_zone=match z{Some(z)=>Some(z.parse::<Tz>().ok()?),None=>None};Some(CronMatcher{minute,hour,day_of_month,month,day_of_week,day_of_month_restricted:f[2]!="*",day_of_week_restricted:f[4]!="*",time_zone})}
fn wall(ms:i64,zone:Option<Tz>)->Option<WallClock>{let utc=Utc.timestamp_millis_opt(ms).single()?;let(minute,hour,month,day,week)=if let Some(z)=zone{let d=utc.with_timezone(&z);(d.minute(),d.hour(),d.month(),d.day(),d.weekday().num_days_from_sunday())}else{let d=utc.with_timezone(&Local);(d.minute(),d.hour(),d.month(),d.day(),d.weekday().num_days_from_sunday())};Some(WallClock{minute,hour,month,day_of_month:day,day_of_week:week})}
fn day(m:&CronMatcher,w:WallClock)->bool{if !m.month.contains(&w.month){return false;}let dom=m.day_of_month.contains(&w.day_of_month);let dow=m.day_of_week.contains(&w.day_of_week);match(m.day_of_month_restricted,m.day_of_week_restricted){(true,true)=>dom||dow,(true,false)=>dom,(false,true)=>dow,(false,false)=>true}}
fn matches(m:&CronMatcher,w:WallClock)->bool{m.minute.contains(&w.minute)&&m.hour.contains(&w.hour)&&day(m,w)}
pub fn compute_next_run_at(schedule:&str,after_ms:f64,fallback_time_zone:Option<&str>)->Option<f64>{if !after_ms.is_finite(){return None;}let n=normalize_schedule(schedule);if let Some(i)=parse_every_interval_ms(&n){return Some(after_ms+i as f64);}let m=compile_cron_matcher(&n)?;let zone=m.time_zone.or_else(||fallback_time_zone.and_then(|z|z.parse::<Tz>().ok()));let after=after_ms.floor()as i64;let mut cursor=after.div_euclid(MINUTE_MS)*MINUTE_MS+MINUTE_MS;let deadline=cursor.checked_add(MAX_CRON_SEARCH_MINUTES.checked_mul(MINUTE_MS)?)?;while cursor<deadline{if matches(&m,wall(cursor,zone)?){return Some(cursor as f64);}cursor=cursor.checked_add(MINUTE_MS)?;}None}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutomationScheduleSummary {
    pub scheduled_fires_next_7_days: usize,
    pub fires_on_weekend: bool,
    pub fires_overnight: bool,
}

pub fn summarize_schedule_next_7_days(
    schedule: &str,
    fallback_time_zone: Option<&str>,
    start_ms: f64,
) -> AutomationScheduleSummary {
    const WINDOW_MS: i64 = 7 * 24 * 60 * 60_000;
    if !start_ms.is_finite() {
        return AutomationScheduleSummary {
            scheduled_fires_next_7_days: 0,
            fires_on_weekend: false,
            fires_overnight: false,
        };
    }
    let start = start_ms.floor() as i64;
    let deadline = start.saturating_add(WINDOW_MS);
    let normalized = normalize_schedule(schedule);
    let interval_ms = parse_every_interval_ms(&normalized);
    let cron_matcher = if interval_ms.is_none() {
        compile_cron_matcher(&normalized)
    } else {
        None
    };
    let interval_fire_count = interval_ms.map(|interval| {
        if interval <= 0 {
            0
        } else {
            ((deadline - start).max(0) / interval) as usize
        }
    });
    if interval_ms.is_some_and(|interval| interval < MINUTE_MS) {
        let count = interval_fire_count.unwrap_or_default();
        return AutomationScheduleSummary {
            scheduled_fires_next_7_days: count,
            fires_on_weekend: count > 0,
            fires_overnight: count > 0,
        };
    }
    let zone = cron_matcher
        .as_ref()
        .and_then(|matcher| matcher.time_zone)
        .or_else(|| fallback_time_zone.and_then(|value| value.parse::<Tz>().ok()));
    let mut cursor = start;
    let mut count = 0usize;
    let mut fires_on_weekend = false;
    let mut fires_overnight = false;
    loop {
        let next = if let Some(interval) = interval_ms {
            cursor.checked_add(interval)
        } else if cron_matcher.is_some() {
            compute_next_run_at(&normalized, cursor as f64, fallback_time_zone)
                .filter(|value| value.is_finite())
                .map(|value| value.floor() as i64)
        } else {
            None
        };
        let Some(next) = next else { break; };
        if next > deadline || next <= cursor { break; }
        let Some(clock) = wall(next, zone) else { break; };
        count = count.saturating_add(1);
        fires_on_weekend |= clock.day_of_week == 0 || clock.day_of_week == 6;
        fires_overnight |= clock.hour < 7 || clock.hour >= 22;
        cursor = next;
    }
    AutomationScheduleSummary {
        scheduled_fires_next_7_days: interval_fire_count.unwrap_or(count),
        fires_on_weekend,
        fires_overnight,
    }
}
