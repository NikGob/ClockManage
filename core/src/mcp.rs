//! Minimal MCP server (JSON-RPC 2.0 over Streamable HTTP, JSON responses only).
//!
//! The transport lives in the app; this module only maps requests to [`McpHost`] calls.

use serde_json::{json, Value};

use crate::clock;
use crate::config::{DayKind, MAX_NIGHT_HOLD_SEC, MAX_NIGHT_UNTIL_MIN, WEEKDAYS};
use crate::day::{PlanBlock, MAX_FOCUS_MIN};
use crate::i18n::tr;
use crate::tr;

pub const SERVER_NAME: &str = "clockmanage";
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

pub trait McpHost {
    fn session_state(&self) -> Value;
    fn day_stats(&self, date: Option<&str>) -> Result<Value, String>;
    /// Journal hours of the week containing `date` (default: this week).
    fn week_stats(&self, date: Option<&str>) -> Result<Value, String>;
    /// Today's plan, the templates and the week; with `date` also the plan of that day.
    fn get_plan(&self, date: Option<&str>) -> Result<Value, String>;
    /// `date` None (or today) = today's plan; a later date = the plan that day starts with.
    fn set_plan(&self, req: PlanRequest) -> Result<Value, String>;
    /// Template of a profile, any day.
    fn set_template(&self, profile: DayKind, plan: Vec<PlanBlock>) -> Result<Value, String>;
    /// Profile of some weekdays (0 = Monday).
    fn set_week_schedule(&self, days: Vec<(usize, DayKind)>) -> Result<Value, String>;
    fn set_day_end(&self, time: &str, reason: Option<&str>) -> Result<Value, String>;
    /// Without a token: preview + one-time token. With the token: close the block.
    fn finish_block(&self, name: Option<&str>, confirm_token: Option<&str>) -> Result<Value, String>;
    /// End-of-block line; `name` None = the latest closed block.
    fn set_block_note(&self, name: Option<&str>, note: &str) -> Result<Value, String>;
    /// The block list and the lock right now.
    fn get_blocklist(&self) -> Value;
    /// Add sites and apps, or take them out (outside the lock only).
    fn edit_blocklist(&self, remove: bool, sites: Vec<String>, apps: Vec<String>) -> Result<Value, String>;
    /// Block the whole list from now for `minutes`, any day; a running focus lock only moves later.
    fn start_focus_lock(&self, minutes: u32, reason: Option<&str>) -> Result<Value, String>;
    /// The night start guard; an empty request only reads it.
    fn set_night_start(&self, req: NightStartRequest) -> Result<Value, String>;
    /// "Автоблокировка с …" of a study profile: minutes after midnight, `None` = off.
    fn set_autolock(&self, profile: DayKind, min: Option<u32>) -> Result<Value, String>;
}

/// Arguments of `set_night_start`: only the given fields change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NightStartRequest {
    pub enabled: Option<bool>,
    pub until_min: Option<u32>,
    pub after_day_end: Option<bool>,
    pub hold_sec: Option<u32>,
}

/// Arguments of `set_plan`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanRequest {
    pub plan: Vec<PlanBlock>,
    pub save_as_template: bool,
    /// YYYY-MM-DD; None = today.
    pub date: Option<String>,
    /// Drop the plan set ahead for `date`: that day starts with its profile's template again.
    pub use_template: bool,
    /// Per plan item: close this started block as it is (today only).
    pub close: Vec<bool>,
}

fn plan_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "A subject, e.g. \"Math\", or a segment (\"Lunch\", \"Nap\")" },
            "minutes": { "type": "integer", "minimum": 1 },
            "hours": { "type": "number", "exclusiveMinimum": 0, "description": "Instead of minutes: 1.5 = 90 min" },
            "type": { "type": "string", "enum": ["study", "break"], "description": "break — a non-study segment (lunch, nap, walk); study by default" }
        },
        "required": ["name"]
    })
}

/// A plan item of today's `set_plan`: also "close as is".
fn today_item_schema() -> Value {
    let mut s = plan_item_schema();
    s["properties"]["close"] = json!({
        "type": "boolean",
        "description": "Today only, and only for a started, not yet closed study block: close it as is — the block's plan becomes the time actually worked (whole minutes, rounded down), the rest leaves the plan, and the day goes on as after the end of a block. minutes are not needed then. If less than a minute was worked, the block is simply removed."
    });
    s
}

fn profile_schema() -> Value {
    json!({ "type": "string", "enum": ["full", "light", "off"] })
}

fn blocklist_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "sites": { "type": "array", "maxItems": 50, "items": { "type": "string" }, "description": "Domains (reddit.com — with its subdomains) or a domain with a path (youtube.com/shorts)" },
            "apps": { "type": "array", "maxItems": 50, "items": { "type": "string" }, "description": "Exe names: Telegram.exe, Steam (.exe is added, a path is dropped)" }
        },
        "additionalProperties": false
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "get_session_state",
            "title": "Timer state now",
            "description": "What is going on now: which block and part, work/break/segment/pause/waiting (phase: work | break | segment | await | idle | done), time left, whether the lock is on (blocking: active, day_lock, reason — including focus, focus_until — the end of a focus lock). blocks[].done_min — time worked in whole minutes, rounded down: the same number is the lowest set_plan may cut a started block to. During a segment — segment {type, planned_min, elapsed_min, overrun_min, alarm, prep, prep_min, stopwatch, queue}: prep — getting ready (for a nap: coffee, getting to bed), no countdown yet, waiting for \"In bed\" — elapsed_min/overrun_min are about the preparation then; stopwatch — an untimed segment, a stopwatch counting up, overrun_min still counts from planned_min. In blocks, plan segments are marked type: \"break\"; taken — already taken (running, done or queued; also when taken by hand ahead of its place — then it won't start a second time). plan_forecast counts segments too (with preparation): the running one, the queue and the planned ones. Times come as milliseconds and ready-made strings; text fields are in the app's language.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_today_stats",
            "title": "Day stats",
            "description": "Stats of a day. blocks[] — study blocks only: planned_min, actual_min, journal_hours (actual_min rounded down to 0.25 h), note (the user's line \"what was boring / what distracted me\"); journal_total — the sum of journal_hours. breaks[] — non-study segments (lunch, nap, walk, custom): {type, planned_min, actual_min, overrun_min, start, end, prep_min, prep_planned_min, lay_at, stopwatch}; they don't count towards study hours or journal_hours. For a segment with preparation (a nap) actual_min is only the nap itself from \"In bed\" (lay_at), prep_min is the preparation separately (null — there was none); ended before \"In bed\" — actual_min 0. stopwatch: true — ran untimed (stopwatch), overrun_min still from planned_min. Also: pauses (shorter than 10 s are not recorded; access on pause only on days before 0.5.0), emergency accesses, day end shifts (day_end_changes), focus locks (focus_locks: at, until, minutes, reason, by), the autolock (autolock: time — its time from the settings, at — when it went on, day_started_at, start_delay_min — the start delay: whole minutes from it going on to \"Start day\", null while the day hasn't started; autolock null — it didn't go on: the day started earlier, a day off or it's off). No argument — today.",
            "inputSchema": {
                "type": "object",
                "properties": { "date": { "type": "string", "description": "YYYY-MM-DD, today by default (in the app's time zone)" } },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_week_stats",
            "title": "Week for the journal",
            "description": "Journal hours for the week (Mon–Sun) containing date: subjects[] — a subject and its journal_hours per day (blocks with the same name add up; each block is rounded down to 0.25 h), day_totals, total, actual_min (the unrounded fact). tsv — the same table as tab-separated text to paste into a spreadsheet. Segments (lunch, nap) are not included.",
            "inputSchema": {
                "type": "object",
                "properties": { "date": { "type": "string", "description": "Any day of the week, YYYY-MM-DD; the current week by default" } },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_plan",
            "title": "Day plan",
            "description": "Today's plan in order (today: study blocks and non-study segments with type: \"break\"), today's profile (profile) and its template (template), today's day end (day_end) and the one from the settings (day_end_default), segment types (segment_types: name, minutes, alarm), today's autolock (autolock: time — HH:MM, at, on — it holds the lock right now, start_delay_min — whole minutes from it going on to \"Start day\"; null — none today: off for this profile, a day off, or the day started before its time). Also: templates — the templates of all profiles (full/light/off: plan, blocking — whether the lock turns on, autolock — HH:MM or null when off, see set_autolock); week — the profile of each weekday (mon…sun); upcoming — the next 7 days: date, weekday, profile, plan and source (\"date\" — a plan set for that date with set_plan + date, \"template\" — the profile's template). With date — also day: the plan and profile of that day.",
            "inputSchema": {
                "type": "object",
                "properties": { "date": { "type": "string", "description": "YYYY-MM-DD — which day to show in day (today or later)" } },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "set_plan",
            "title": "Set the day plan",
            "description": "Without date — replaces today's plan with a list of items in the order of the day. Items are matched with the old ones by name (case-insensitive) and type in any order: everything can be reordered — blocks not started and started, segments; worked time travels with its block. A study block: {name, minutes|hours}. A non-study segment (lunch, nap, walk…): {name, minutes, type: \"break\"} — it stands in its place: when the block before it closes, this segment starts instead of the break between blocks (a nap first waits for \"In bed\" with no countdown, the alarm counts from \"In bed\"). A segment taken by hand ahead of its place cancels the planned one of the same type. Segments don't count towards study hours or journal_hours, but are counted in plan_forecast (with preparation). Rules. Not started — add, lengthen, shorten, remove. Segments, even past ones, can be removed — their record in breaks[] stays. A started block can't be removed or renamed — except a block with less than 1 minute worked (an accidental start): it is removed. While the lock is on, a started block can't be cut below the time worked; the limit is whole minutes rounded down, exactly done_min from get_session_state (10 min 40 s → 10 is fine). To close a started block as is — an item {name, close: true}: the block's plan = time worked, the rest is dropped, the day goes on; do this only when the user asks (for the current block without an explicit request use finish_block, it asks). All or nothing: on an error the plan doesn't change. Worked time is never erased. The reply has changes (added | removed | shortened | lengthened | moved | closed) and plan_forecast. With a date after today — a plan for that day only: it starts with this plan instead of its profile's template; today's plan and the templates don't change (up to 60 days ahead; the day's profile follows the week schedule, see get_plan → week). use_template: true with date — drop the plan set for that date (the day starts with the template).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "blocks": { "type": "array", "minItems": 1, "maxItems": 16, "items": today_item_schema() },
                    "date": { "type": "string", "description": "YYYY-MM-DD; today by default. After today — the plan for that day" },
                    "save_as_template": { "type": "boolean", "description": "Also make this plan the template of that day's profile (today's or date's): Full/Light/Day off" },
                    "use_template": { "type": "boolean", "description": "Only with date: drop the plan set for that date; blocks are not needed then" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_template",
            "title": "Set a profile template",
            "description": "Replaces the template of a day profile — the plan every day of this profile starts with. Any day, even during the study day: a started day doesn't change. If today has this profile, the day hasn't started and its plan matched the old template, today's plan becomes the new one too (like on the Plan screen; today_plan_updated in the reply). Dates with their own plan (set_plan with date) start with that plan — they are listed in overridden_dates. blocks have the same format as in set_plan, including type: \"break\" segments; an empty array — an empty template.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profile": { "type": "string", "enum": ["full", "light", "off"], "description": "full — Full, light — Light, off — Day off" },
                    "blocks": { "type": "array", "maxItems": 16, "items": plan_item_schema() }
                },
                "required": ["profile", "blocks"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_week_schedule",
            "title": "Weekday profiles",
            "description": "Which profile each weekday has (full — Full, light — Light, off — Day off). Only the listed days change. The current schedule — get_plan → week. If today's weekday changes and the day hasn't started, today switches to the new profile and its template too; a started day doesn't change. The reply has week — the whole schedule.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {
                        "type": "object",
                        "description": "E.g. {\"mon\": \"full\", \"sat\": \"light\"}",
                        "properties": {
                            "mon": profile_schema(), "tue": profile_schema(), "wed": profile_schema(), "thu": profile_schema(),
                            "fri": profile_schema(), "sat": profile_schema(), "sun": profile_schema()
                        },
                        "additionalProperties": false,
                        "minProperties": 1
                    }
                },
                "required": ["days"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_night_start",
            "title": "Night start guard",
            "description": "Settings of the night start guard. At night \"Start day\" works only in the app window and only after holding the button (the tray, the mini timer and the phone don't start a day then), so a stray click doesn't start a new day. Night = from midnight until `until`, plus after today's day end when `after_day_end` is on. Only the given fields change; with no arguments it just returns the current settings. The reply has night_start (the settings) and night_now (whether a start right now counts as a night one). Allowed at any time, also during the lock.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean", "description": "false = one click starts the day at any time" },
                    "until": { "type": "string", "pattern": "^\\d{1,2}:\\d{2}$", "description": "HH:MM, 00:00–12:00, in the app's time zone: the night lasts from midnight until then (00:00 = only after the day end)" },
                    "after_day_end": { "type": "boolean", "description": "It is night after today's day end too" },
                    "hold_sec": { "type": "integer", "minimum": 1, "maximum": 10, "description": "How many seconds to hold the button" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_autolock",
            "title": "Autolock before the day starts",
            "description": "The autolock of a study profile (full — Full, light — Light; a day off never has one): if the study day hasn't started by `time`, the whole block list goes on (sites, apps, a paired phone) as during a study block, and holds until \"Start day\" — from then on the day's own lock and rules apply — or until the day end. Five minutes before, the user gets a warning. A day started before `time` is not touched. Neither the agent nor the app can lift it early — only the user's own emergency access (a long phrase typed by hand, logged). Once it went on today it holds even if the setting changes: a change shapes the days ahead (and today, while today's has not gone on yet). Its time going on and the start delay (from it going on to \"Start day\") go to the day log and to get_today_stats → autolock; get_plan → autolock and templates[].autolock show the settings. The default is 13:00 for both profiles. Allowed at any time. The reply has autolock (the time, or null — off) and today (today's autolock: time, on, at, start_delay_min).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profile": { "type": "string", "enum": ["full", "light"], "description": "full — Full, light — Light" },
                    "time": { "type": "string", "pattern": "^(\\d{1,2}:\\d{2}|off)$", "description": "HH:MM in the app's time zone, e.g. 13:00; \"off\" turns the autolock off for this profile" }
                },
                "required": ["profile", "time"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_day_end",
            "title": "Move the day end",
            "description": "Changes the end of the study day for today only (the setting stays untouched). The lock, the timers and the \"day is over\" notice follow the new time at once. Time is in the app's time zone, later than now and no later than 02:00 the next night (00:00–02:00 = the night after today). The reply has the old and the new time, how much of the plan is left and whether it fits before the new end. Every change goes to the day log.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "time": { "type": "string", "pattern": "^\\d{1,2}:\\d{2}$", "description": "HH:MM in the app's time zone, e.g. 23:00" },
                    "reason": { "type": "string", "description": "Why — goes to the log and the notification" }
                },
                "required": ["time"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_block_note",
            "title": "End-of-block line",
            "description": "Saves the user's line about a block: what was boring, what distracted them (up to 300 characters). Goes to the log and to get_today_stats as blocks[].note. Without name — the latest closed block. Calling again replaces the line.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "The block; the latest closed one by default" },
                    "note": { "type": "string", "description": "The user's words, verbatim if possible" }
                },
                "required": ["note"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "finish_block",
            "title": "Close a block now",
            "description": "Closes a started block right now at the minutes actually worked: the block's planned_min becomes the time worked in whole minutes rounded down (like done_min); at least 1 minute is needed — a block with less is simply removed with set_plan. The day goes on as after a normal end of a block (the break between blocks or the end of the day), everything goes to the log. Avoids the race of \"cut it with set_plan while the timer runs\". TWO STEPS. The first call (without confirm_token) changes nothing: it returns a preview (time worked, how much leaves the plan) and a one-time confirm_token valid for 2 minutes. Before the second call you MUST ask the user the direct question from the ask_user field and wait for an explicit \"yes\". Only then call finish_block with confirm_token. Don't confirm for the user, don't make the second call on your own and don't take general phrases (\"let's go on\", \"ok\") as consent. If the user asks to reshape the day and close the block in one step — set_plan with close: true.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Which block to close; the current one by default" },
                    "confirm_token": { "type": "string", "description": "The token from the first call — only after the user's explicit \"yes\"" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false }
        },
        {
            "name": "get_blocklist",
            "title": "Block list",
            "description": "What is blocked and whether the lock is on now. sites — domains (x.com, with its subdomains) and paths (youtube.com/shorts — browser policies only, the rest of the site stays open); apps — Windows apps by exe name (Telegram.exe): while the lock is on, running ones are closed and new ones can't start; phone_apps — phone apps (read only, picked on the phone). blocking: active — sites and apps are closed right now; day_lock — a lock is in force (a study day, a single timer with the lock, the autolock before the day starts, or a focus lock), maybe opened for a while by an access; reason — study | single | autolock | focus | emergency | break_access | lunch_at_pc | segment_access | not_started | not_study_day | completed | day_end; until — the end of an open access; focus_until — the end of a focus lock (start_focus_lock). can_remove — whether entries can be removed now (outside the lock only). admin — whether the app runs as administrator: without it sites are not blocked.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "add_to_blocklist",
            "title": "Add to the block list",
            "description": "Adds sites and/or apps to the block list. Always allowed, including during the lock: then the new entries close at once (sites — via hosts and browser policies, an app — closed if running and kept from starting). A site: a domain (reddit.com — with subdomains) or a domain with a path (youtube.com/shorts — without a path the whole site is closed); https://, www. and a trailing / are dropped. An app: an exe name (Steam or Steam.exe; a path is dropped); system processes (explorer.exe etc.) and ClockManage itself are never blocked. The list applies while a study day with the lock, a single timer with the lock, the autolock (set_autolock) or a focus lock (start_focus_lock) runs — adding outside of that closes nothing by itself. The reply has added (what was added, as stored), unchanged (already there), invalid (not a site / a system app), the whole blocklist and blocking.",
            "inputSchema": blocklist_schema(),
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "remove_from_blocklist",
            "title": "Remove from the block list",
            "description": "Removes sites and/or apps from the block list. Outside the lock only — before \"Start day\" (and before the autolock goes on), after the day end or when all blocks are done, with no single timer with the lock and no focus lock (get_blocklist → can_remove). During the lock — refused: the list can only grow. Remove only when the user explicitly asks. Entries are compared case-insensitively after the same cleanup as when adding; youtube.com doesn't remove youtube.com/shorts — they are different entries. The reply has removed, unchanged (wasn't in the list), invalid, the whole blocklist.",
            "inputSchema": blocklist_schema(),
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true }
        },
        {
            "name": "start_focus_lock",
            "title": "Focus lock",
            "description": "Blocks the whole block list (sites and apps; on a paired phone too) right now for minutes minutes — on any day: whether the study day hasn't started, is over or it's a day off doesn't matter. Neither the agent nor the app can lift it early — only the user's own emergency access (a long phrase typed by hand, logged); access in the break between blocks, lunch at the PC and segment access don't open it. While it runs, the block list can only grow and the app can't be quit. Calling again only extends it: minutes count from now, and the new end must be later than the current one, else it's refused. A lock running past midnight carries over to the next day. Turn it on only when the user explicitly asks, and for the time they named (up to 720 minutes at once); to block something not in the list, add_to_blocklist first. It doesn't touch the study day, the plan or the timer. The reply has until (HH:MM in the app's time), until_ms, minutes_left, extended and from (the old end when extended), blocklist and blocking. The start and every extension go to the day log and to get_today_stats → focus_locks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "minutes": { "type": "integer", "minimum": 1, "maximum": MAX_FOCUS_MIN, "description": "How many minutes to block, starting now" },
                    "hours": { "type": "number", "exclusiveMinimum": 0, "description": "Instead of minutes: 1.5 = 90 min" },
                    "reason": { "type": "string", "description": "Why, in the user's words — goes to the log and the notification" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false }
        }
    ])
}

fn ok(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn err(id: &Value, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

fn tool_result(r: Result<Value, String>) -> Value {
    match r {
        Ok(v) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
            "structuredContent": v,
            "isError": false
        }),
        Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
    }
}

fn parse_blocks(args: &Value) -> Result<Vec<PlanBlock>, String> {
    let (plan, close) = parse_items(args)?;
    if close.iter().any(|c| *c) {
        return Err(tr!("mcp_close_works_only_in_set_plan"));
    }
    Ok(plan)
}

/// Plan items and their "close as is" flags.
fn parse_items(args: &Value) -> Result<(Vec<PlanBlock>, Vec<bool>), String> {
    let blocks = args.get("blocks").and_then(Value::as_array).ok_or(tr("mcp_blocks_must_be_an_array"))?;
    if blocks.len() > 16 {
        return Err(tr!("mcp_no_more_than_16_plan_items"));
    }
    let mut plan = vec![];
    let mut close = vec![];
    for b in blocks {
        let name = b.get("name").and_then(Value::as_str).ok_or(tr("mcp_a_block_has_no_name"))?;
        let closing = b.get("close").and_then(Value::as_bool).unwrap_or(false);
        let is_break = match b.get("type").and_then(Value::as_str).unwrap_or("study") {
            "break" => true,
            "study" => false,
            other => return Err(tr!("mcp_type_is_study_or_break_not", name = name, other = other)),
        };
        if closing && is_break {
            return Err(tr!("mcp_close_closes_a_study_block_and", name = name));
        }
        let minutes = if let Some(m) = b.get("minutes").and_then(Value::as_f64) {
            m
        } else if let Some(h) = b.get("hours").and_then(Value::as_f64) {
            h * 60.0
        } else if closing {
            // Closing sets the length to the minutes worked.
            1.0
        } else {
            return Err(tr!("mcp_block_has_no_minutes_or_hours", name = name));
        };
        let max = if is_break { 240.0 } else { 480.0 };
        if !(1.0..=max).contains(&minutes) {
            return Err(tr!("mcp_the_duration_must_be_1_to", name = name, max = max));
        }
        let m = minutes.round() as u32;
        plan.push(if is_break { PlanBlock::brk(name, m) } else { PlanBlock::new(name, m) });
        close.push(closing);
    }
    Ok((plan, close))
}

fn parse_plan(args: &Value) -> Result<PlanRequest, String> {
    let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
    let date = args.get("date").and_then(Value::as_str).map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    let use_template = flag("use_template");
    if use_template && date.is_none() {
        return Err(tr!("mcp_use_template_works_only_with_date"));
    }
    let (plan, close) = if use_template { (vec![], vec![]) } else { parse_items(args)? };
    if plan.is_empty() && !use_template {
        return Err(tr!("mcp_the_plan_needs_at_least_one"));
    }
    let save_as_template = flag("save_as_template") && !use_template;
    if save_as_template && close.iter().any(|c| *c) {
        return Err(tr!("mcp_close_and_save_as_template_cant"));
    }
    Ok(PlanRequest { plan, save_as_template, date, use_template, close })
}

fn parse_template(args: &Value) -> Result<(DayKind, Vec<PlanBlock>), String> {
    let p = args.get("profile").and_then(Value::as_str).ok_or(tr("mcp_profile_is_required_full_light_or"))?;
    let kind = DayKind::from_key(p).ok_or(tr!("mcp_profile_is_full_light_or_off", p = p))?;
    Ok((kind, parse_blocks(args)?))
}

fn parse_week(args: &Value) -> Result<Vec<(usize, DayKind)>, String> {
    let days = args.get("days").and_then(Value::as_object).ok_or(tr("mcp_days_must_be_an_object_e"))?;
    let mut out = vec![];
    for (k, v) in days {
        let i = WEEKDAYS.iter().position(|d| d == k).ok_or(tr!("mcp_a_weekday_is_mon_sun_not", k = k))?;
        let p = v.as_str().unwrap_or("");
        let kind = DayKind::from_key(p).ok_or(tr!("mcp_a_profile_is_full_light_or", k = k, p = p))?;
        out.push((i, kind));
    }
    if out.is_empty() {
        return Err(tr!("mcp_days_has_no_days"));
    }
    Ok(out)
}

/// `sites` and `apps` of the block-list tools; at least one entry.
fn parse_lists(args: &Value) -> Result<(Vec<String>, Vec<String>), String> {
    let list = |k: &str| -> Result<Vec<String>, String> {
        let bad = || tr!("mcp_must_be_an_array_of_strings", k = k);
        match args.get(k) {
            None | Some(Value::Null) => Ok(vec![]),
            Some(Value::Array(a)) => a.iter().map(|v| v.as_str().map(str::to_string).ok_or_else(bad)).collect(),
            Some(_) => Err(bad()),
        }
    };
    let (sites, apps) = (list("sites")?, list("apps")?);
    if sites.is_empty() && apps.is_empty() {
        return Err(tr!("mcp_sites_or_apps_is_required_at"));
    }
    if sites.len() > 50 || apps.len() > 50 {
        return Err(tr!("mcp_no_more_than_50_sites_and"));
    }
    Ok((sites, apps))
}

fn parse_focus(args: &Value) -> Result<u32, String> {
    let minutes = if let Some(m) = args.get("minutes").and_then(Value::as_f64) {
        m
    } else if let Some(h) = args.get("hours").and_then(Value::as_f64) {
        h * 60.0
    } else {
        return Err(tr!("mcp_minutes_or_hours_is_required_how"));
    };
    if !(1.0..=MAX_FOCUS_MIN as f64).contains(&minutes) {
        return Err(tr!("mcp_a_focus_lock_is_1_to", max_focus_min = MAX_FOCUS_MIN / 60, MAX_FOCUS_MIN = MAX_FOCUS_MIN));
    }
    Ok(minutes.round() as u32)
}

fn parse_night(args: &Value) -> Result<NightStartRequest, String> {
    let flag = |k: &str| match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| tr!("mcp_must_be_true_or_false", k = k)),
    };
    let until_min = match args.get("until") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_str().and_then(clock::parse_hm) {
            Some(m) if m <= MAX_NIGHT_UNTIL_MIN => Some(m),
            _ => return Err(tr!("mcp_until_is_hh_mm_from_00")),
        },
    };
    let hold_sec = match args.get("hold_sec") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_f64() {
            Some(s) if s.fract() == 0.0 && (1.0..=MAX_NIGHT_HOLD_SEC as f64).contains(&s) => Some(s as u32),
            _ => return Err(tr!("mcp_hold_sec_is_a_whole_number", max = MAX_NIGHT_HOLD_SEC)),
        },
    };
    Ok(NightStartRequest { enabled: flag("enabled")?, until_min, after_day_end: flag("after_day_end")?, hold_sec })
}

fn parse_autolock(args: &Value) -> Result<(DayKind, Option<u32>), String> {
    let p = args.get("profile").and_then(Value::as_str).unwrap_or("");
    let kind = match DayKind::from_key(p) {
        Some(DayKind::Off) => return Err(tr!("mcp_a_day_off_has_no_autolock")),
        Some(k) => k,
        None => return Err(tr!("mcp_profile_is_full_or_light", p = p)),
    };
    let min = match args.get("time") {
        Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("off") => None,
        Some(Value::Bool(false)) | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(clock::parse_hm(s).ok_or(tr("mcp_time_is_hh_mm_or_off"))?),
        _ => return Err(tr!("mcp_time_is_hh_mm_or_off")),
    };
    Ok((kind, min))
}

fn handle_one(msg: &Value, host: &dyn McpHost) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str);
    let id = msg.get("id").cloned();
    let Some(id) = id else {
        // Notification (initialized, cancelled, ...) or a response — nothing to answer.
        return None;
    };
    let Some(method) = method else {
        return Some(err(&id, -32600, "Invalid Request"));
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOLS[0]);
            let version = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[0] };
            ok(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": SERVER_NAME, "title": "ClockManage — study timer", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": "A study timer that blocks distractions. Texts in the results (titles, log lines, errors) are in the app's language — answer the user in theirs. get_session_state — what is going on now (block, break, a segment like lunch or a nap). get_week_stats — journal hours for a week by subject and day (with a ready tsv). get_today_stats — the actual hours of a day: blocks[].journal_hours (rounded down to 0.25 h) and journal_total for the journal, blocks[].note — the user's \"what was boring\" lines, breaks[] — lunch, naps, walks. get_plan — today's plan, the templates of all profiles, the week schedule and the plans of the next 7 days. set_plan — the day plan (study blocks and type: \"break\" segments in their places; everything can be reordered, a block with an accidental start under a minute can be removed, a started one closed as is with close: true when the user asks); with date — a plan for a future day, today stays untouched. Worked time is always whole minutes rounded down: done_min = the cutting limit. set_template — the template of any profile (full/light/off). set_week_schedule — which profile each weekday has. set_day_end — move today's day end once. set_block_note — save the user's line about a block. finish_block — close a started block at the time worked: two steps, the second only after the user's explicit \"yes\". get_blocklist — what is in the block list (sites, .exe apps) and whether the lock is on. add_to_blocklist — add sites and apps (always allowed, during the lock they close at once); remove_from_blocklist — remove (outside the lock only and when the user asks). start_focus_lock — block the whole list right now for N minutes on any day; it can't be lifted early (only by the user's emergency access), only extended — turn it on only when the user explicitly asks. set_night_start — the night start guard: at night \"Start day\" takes a hold of the button in the app window (until a time, after the day end, how many seconds); without arguments it shows the current settings. set_autolock — the autolock of the Full or Light profile (13:00 by default, or off): if the day hasn't started by then, the block list goes on until \"Start day\"; only the user's emergency access opens it early."
                }),
            )
        }
        "ping" => ok(&id, json!({})),
        "tools/list" => ok(&id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let res = match name {
                "get_session_state" => Ok(host.session_state()),
                "get_today_stats" => host.day_stats(args.get("date").and_then(Value::as_str)),
                "get_plan" => host.get_plan(args.get("date").and_then(Value::as_str)),
                "get_week_stats" => host.week_stats(args.get("date").and_then(Value::as_str)),
                "set_plan" => parse_plan(&args).and_then(|r| host.set_plan(r)),
                "set_template" => parse_template(&args).and_then(|(k, p)| host.set_template(k, p)),
                "set_week_schedule" => parse_week(&args).and_then(|d| host.set_week_schedule(d)),
                "set_block_note" => match args.get("note").and_then(Value::as_str) {
                    Some(n) => host.set_block_note(args.get("name").and_then(Value::as_str), n),
                    None => Err(tr!("mcp_note_is_required")),
                },
                "finish_block" => host.finish_block(args.get("name").and_then(Value::as_str), args.get("confirm_token").and_then(Value::as_str)),
                "set_day_end" => match args.get("time").and_then(Value::as_str) {
                    Some(t) => host.set_day_end(t, args.get("reason").and_then(Value::as_str)),
                    None => Err(tr!("mcp_time_is_required_hh_mm")),
                },
                "get_blocklist" => Ok(host.get_blocklist()),
                "add_to_blocklist" => parse_lists(&args).and_then(|(s, a)| host.edit_blocklist(false, s, a)),
                "remove_from_blocklist" => parse_lists(&args).and_then(|(s, a)| host.edit_blocklist(true, s, a)),
                "set_night_start" => parse_night(&args).and_then(|r| host.set_night_start(r)),
                "set_autolock" => parse_autolock(&args).and_then(|(k, m)| host.set_autolock(k, m)),
                "start_focus_lock" => parse_focus(&args).and_then(|m| host.start_focus_lock(m, args.get("reason").and_then(Value::as_str))),
                _ => return Some(err(&id, -32602, &format!("Unknown tool: {name}"))),
            };
            ok(&id, tool_result(res))
        }
        "resources/list" => ok(&id, json!({ "resources": [] })),
        "prompts/list" => ok(&id, json!({ "prompts": [] })),
        _ => err(&id, -32601, "Method not found"),
    })
}

/// Handle a POST body. `None` means "202 Accepted, no body".
pub fn handle(body: &str, host: &dyn McpHost) -> Option<String> {
    let parsed: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Some(err(&Value::Null, -32700, "Parse error").to_string()),
    };
    match parsed {
        Value::Array(items) => {
            let out: Vec<Value> = items.iter().filter_map(|m| handle_one(m, host)).collect();
            (!out.is_empty()).then(|| Value::Array(out).to_string())
        }
        v => handle_one(&v, host).map(|r| r.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake(RefCell<Vec<PlanBlock>>, RefCell<Vec<Value>>);
    impl McpHost for Fake {
        fn session_state(&self) -> Value {
            json!({"phase": "work"})
        }
        fn day_stats(&self, _: Option<&str>) -> Result<Value, String> {
            Ok(json!({}))
        }
        fn week_stats(&self, d: Option<&str>) -> Result<Value, String> {
            Ok(json!({ "week_of": d }))
        }
        fn get_plan(&self, d: Option<&str>) -> Result<Value, String> {
            Ok(json!({ "today": *self.0.borrow(), "date": d }))
        }
        fn set_plan(&self, r: PlanRequest) -> Result<Value, String> {
            self.1.borrow_mut().push(json!({ "date": r.date, "use_template": r.use_template, "save": r.save_as_template, "close": r.close }));
            *self.0.borrow_mut() = r.plan;
            Ok(json!({"ok": true}))
        }
        fn set_template(&self, k: DayKind, p: Vec<PlanBlock>) -> Result<Value, String> {
            self.1.borrow_mut().push(json!({ "template": k, "plan": p }));
            Ok(json!({"ok": true}))
        }
        fn set_week_schedule(&self, d: Vec<(usize, DayKind)>) -> Result<Value, String> {
            self.1.borrow_mut().push(json!({ "week": d }));
            Ok(json!({"ok": true}))
        }
        fn set_day_end(&self, t: &str, r: Option<&str>) -> Result<Value, String> {
            Ok(json!({"new": t, "reason": r}))
        }
        fn set_block_note(&self, n: Option<&str>, note: &str) -> Result<Value, String> {
            Ok(json!({"block": n, "note": note}))
        }
        fn finish_block(&self, n: Option<&str>, t: Option<&str>) -> Result<Value, String> {
            Ok(match t {
                None => json!({"needs_confirmation": true, "block": n, "confirm_token": "abc"}),
                Some(t) => json!({"ok": true, "token": t}),
            })
        }
        fn get_blocklist(&self) -> Value {
            json!({ "sites": ["x.com"], "apps": ["Telegram.exe"] })
        }
        fn edit_blocklist(&self, remove: bool, sites: Vec<String>, apps: Vec<String>) -> Result<Value, String> {
            self.1.borrow_mut().push(json!({ "remove": remove, "sites": sites, "apps": apps }));
            Ok(json!({"ok": true}))
        }
        fn start_focus_lock(&self, minutes: u32, reason: Option<&str>) -> Result<Value, String> {
            self.1.borrow_mut().push(json!({ "focus": minutes, "reason": reason }));
            Ok(json!({"ok": true}))
        }
        fn set_night_start(&self, r: NightStartRequest) -> Result<Value, String> {
            self.1.borrow_mut().push(json!({ "enabled": r.enabled, "until_min": r.until_min, "after_day_end": r.after_day_end, "hold_sec": r.hold_sec }));
            Ok(json!({"ok": true}))
        }
        fn set_autolock(&self, k: DayKind, min: Option<u32>) -> Result<Value, String> {
            self.1.borrow_mut().push(json!({ "autolock": k, "min": min }));
            Ok(json!({"ok": true}))
        }
    }

    #[test]
    fn autolock_args() {
        let h = Fake(RefCell::new(vec![]), RefCell::new(vec![]));
        let r = handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, &h).unwrap();
        assert!(r.contains("set_autolock"));
        assert_eq!(call(&h, "set_autolock", json!({ "profile": "light", "time": "12:30" }))["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap(), &json!({ "autolock": "light", "min": 750 }));
        assert_eq!(call(&h, "set_autolock", json!({ "profile": "full", "time": "off" }))["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap(), &json!({ "autolock": "full", "min": null }));
        // A day off has none; the time is HH:MM or "off".
        assert_eq!(call(&h, "set_autolock", json!({ "profile": "off", "time": "13:00" }))["isError"], true);
        assert_eq!(call(&h, "set_autolock", json!({ "profile": "weekend", "time": "13:00" }))["isError"], true);
        assert_eq!(call(&h, "set_autolock", json!({ "profile": "full", "time": "25:00" }))["isError"], true);
        assert_eq!(call(&h, "set_autolock", json!({ "profile": "full" }))["isError"], true);
        assert_eq!(h.1.borrow().len(), 2);
    }

    #[test]
    fn initialize_and_call() {
        let h = Fake(RefCell::new(vec![]), RefCell::new(vec![]));
        let r = handle(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, &h).unwrap();
        assert!(r.contains("2025-03-26"));
        assert!(handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &h).is_none());
        let r = handle(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &h).unwrap();
        assert!(r.contains("get_today_stats") && r.contains("get_week_stats"));
        let r = handle(r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"get_week_stats","arguments":{"date":"2026-09-30"}}}"#, &h).unwrap();
        assert!(r.contains("2026-09-30"));
        let r = handle(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_plan","arguments":{"blocks":[{"name":"Математика","hours":1.5},{"name":"Экстернат","minutes":150}]}}}"#,
            &h,
        )
        .unwrap();
        assert!(r.contains("\"isError\":false"));
        assert_eq!(h.0.borrow()[0].minutes, 90);
        let r = handle(
            r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"set_plan","arguments":{"blocks":[{"name":"Математика","minutes":90},{"name":"Обед","minutes":45,"type":"break"}]}}}"#,
            &h,
        )
        .unwrap();
        assert!(r.contains("\"isError\":false"));
        assert!(h.0.borrow()[1].is_break());
        let r = handle(r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"set_plan","arguments":{"blocks":[{"name":"X","minutes":5,"type":"nap"}]}}}"#, &h).unwrap();
        assert!(r.contains("\"isError\":true"));
        let r = handle(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_plan","arguments":{"blocks":[{"name":"X"}]}}}"#, &h).unwrap();
        assert!(r.contains("\"isError\":true"));
        let r = handle(r#"{"jsonrpc":"2.0","id":5,"method":"tools/list"}"#, &h).unwrap();
        assert!(r.contains("set_day_end"));
        let r = handle(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"set_day_end","arguments":{"time":"23:00","reason":"разовый сдвиг"}}}"#, &h).unwrap();
        assert!(r.contains("\"isError\":false") && r.contains("разовый сдвиг"));
        let r = handle(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"set_day_end","arguments":{}}}"#, &h).unwrap();
        assert!(r.contains("\"isError\":true"));
        let r = handle(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"finish_block","arguments":{"name":"Математика"}}}"#, &h).unwrap();
        assert!(r.contains("needs_confirmation") && r.contains("Математика"));
        let r = handle(r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"finish_block","arguments":{"confirm_token":"abc"}}}"#, &h).unwrap();
        assert!(r.contains("\"isError\":false") && r.contains("abc"));
    }

    #[test]
    fn night_start_args() {
        let h = Fake(RefCell::new(vec![]), RefCell::new(vec![]));
        let r = call(&h, "set_night_start", json!({ "until": "6:30", "hold_sec": 3, "after_day_end": false }));
        assert_eq!(r["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap(), &json!({ "enabled": null, "until_min": 390, "after_day_end": false, "hold_sec": 3 }));
        assert_eq!(call(&h, "set_night_start", json!({}))["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap()["until_min"], Value::Null);
        assert_eq!(call(&h, "set_night_start", json!({ "until": "13:00" }))["isError"], true);
        assert_eq!(call(&h, "set_night_start", json!({ "hold_sec": 0 }))["isError"], true);
        assert_eq!(call(&h, "set_night_start", json!({ "hold_sec": 2.5 }))["isError"], true);
        assert_eq!(call(&h, "set_night_start", json!({ "enabled": "yes" }))["isError"], true);
    }

    fn call(h: &Fake, name: &str, args: Value) -> Value {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let r: Value = serde_json::from_str(&handle(&body.to_string(), h).unwrap()).unwrap();
        r["result"].clone()
    }

    #[test]
    fn set_plan_closes_a_block_as_is() {
        let h = Fake(RefCell::new(vec![]), RefCell::new(vec![]));
        let r = handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, &h).unwrap();
        assert!(r.contains("\"close\""));
        // close needs no minutes
        let r = call(&h, "set_plan", json!({ "blocks": [{ "name": "Математика", "close": true }, { "name": "Экстернат", "minutes": 150 }] }));
        assert_eq!(r["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap()["close"], json!([true, false]));
        // not for a segment, not into a template
        assert_eq!(call(&h, "set_plan", json!({ "blocks": [{ "name": "Обед", "type": "break", "close": true }] }))["isError"], true);
        assert_eq!(call(&h, "set_plan", json!({ "blocks": [{ "name": "A", "close": true }], "save_as_template": true }))["isError"], true);
        assert_eq!(call(&h, "set_template", json!({ "profile": "full", "blocks": [{ "name": "A", "minutes": 5, "close": true }] }))["isError"], true);
    }

    #[test]
    fn plans_for_other_days() {
        let h = Fake(RefCell::new(vec![]), RefCell::new(vec![]));
        let r = handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, &h).unwrap();
        assert!(r.contains("set_template") && r.contains("set_week_schedule"));

        // A plan for Monday, from Sunday.
        let r = call(&h, "set_plan", json!({ "date": "2026-10-05", "blocks": [{ "name": "Экстернат", "minutes": 180 }] }));
        assert_eq!(r["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap()["date"], "2026-10-05");
        // use_template needs a date; with it blocks are not needed.
        assert_eq!(call(&h, "set_plan", json!({ "use_template": true }))["isError"], true);
        assert_eq!(call(&h, "set_plan", json!({ "date": "2026-10-05", "use_template": true }))["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap()["use_template"], true);
        assert_eq!(call(&h, "set_plan", json!({ "blocks": [] }))["isError"], true);

        // The template of any profile, breaks included.
        let blocks = json!([
            { "name": "Экстернат", "minutes": 180 },
            { "name": "Обед", "minutes": 45, "type": "break" },
            { "name": "Сон", "minutes": 20, "type": "break" },
            { "name": "Словацкий", "minutes": 90 },
            { "name": "Математика", "minutes": 60 }
        ]);
        let r = call(&h, "set_template", json!({ "profile": "full", "blocks": blocks }));
        assert_eq!(r["isError"], false);
        let last = h.1.borrow().last().unwrap().clone();
        assert_eq!(last["template"], "full");
        assert_eq!(last["plan"][1]["type"], "break");
        assert_eq!(call(&h, "set_template", json!({ "profile": "off", "blocks": [] }))["isError"], false);
        assert_eq!(call(&h, "set_template", json!({ "profile": "weekend", "blocks": [] }))["isError"], true);

        // The week schedule: only the listed days.
        let r = call(&h, "set_week_schedule", json!({ "days": { "mon": "full", "sun": "light" } }));
        assert_eq!(r["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap()["week"], json!([[0, "full"], [6, "light"]]));
        assert_eq!(call(&h, "set_week_schedule", json!({ "days": { "monday": "full" } }))["isError"], true);
        assert_eq!(call(&h, "set_week_schedule", json!({ "days": {} }))["isError"], true);

        let r = call(&h, "get_plan", json!({ "date": "2026-10-05" }));
        assert_eq!(r["structuredContent"]["date"], "2026-10-05");
    }

    #[test]
    fn blocklist_and_focus_lock() {
        let h = Fake(RefCell::new(vec![]), RefCell::new(vec![]));
        let r = handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, &h).unwrap();
        for t in ["get_blocklist", "add_to_blocklist", "remove_from_blocklist", "start_focus_lock"] {
            assert!(r.contains(t), "{t}");
        }
        assert_eq!(call(&h, "get_blocklist", json!({}))["structuredContent"]["apps"][0], "Telegram.exe");

        assert_eq!(call(&h, "add_to_blocklist", json!({ "sites": ["reddit.com"] }))["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap().clone(), json!({ "remove": false, "sites": ["reddit.com"], "apps": [] }));
        assert_eq!(call(&h, "remove_from_blocklist", json!({ "apps": ["Steam"] }))["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap().clone(), json!({ "remove": true, "sites": [], "apps": ["Steam"] }));
        // Nothing to do, or not a list of strings.
        assert_eq!(call(&h, "add_to_blocklist", json!({}))["isError"], true);
        assert_eq!(call(&h, "add_to_blocklist", json!({ "sites": [], "apps": [] }))["isError"], true);
        assert_eq!(call(&h, "add_to_blocklist", json!({ "sites": "reddit.com" }))["isError"], true);
        assert_eq!(call(&h, "add_to_blocklist", json!({ "sites": [1] }))["isError"], true);
        let many: Vec<String> = (0..51).map(|i| format!("s{i}.com")).collect();
        assert_eq!(call(&h, "add_to_blocklist", json!({ "sites": many }))["isError"], true);

        assert_eq!(call(&h, "start_focus_lock", json!({ "minutes": 90, "reason": "ютуб" }))["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap().clone(), json!({ "focus": 90, "reason": "ютуб" }));
        assert_eq!(call(&h, "start_focus_lock", json!({ "hours": 1.5 }))["isError"], false);
        assert_eq!(h.1.borrow().last().unwrap()["focus"], 90);
        assert_eq!(call(&h, "start_focus_lock", json!({}))["isError"], true);
        assert_eq!(call(&h, "start_focus_lock", json!({ "minutes": 0 }))["isError"], true);
        assert_eq!(call(&h, "start_focus_lock", json!({ "minutes": MAX_FOCUS_MIN + 1 }))["isError"], true);
    }
}
