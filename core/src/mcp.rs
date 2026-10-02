//! Minimal MCP server (JSON-RPC 2.0 over Streamable HTTP, JSON responses only).
//!
//! The transport lives in the app; this module only maps requests to [`McpHost`] calls.

use serde_json::{json, Value};

use crate::day::PlanBlock;

pub const SERVER_NAME: &str = "clockmanage";
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

pub trait McpHost {
    fn session_state(&self) -> Value;
    fn day_stats(&self, date: Option<&str>) -> Result<Value, String>;
    fn get_plan(&self) -> Value;
    fn set_plan(&self, plan: Vec<PlanBlock>, save_as_template: bool) -> Result<Value, String>;
    fn set_day_end(&self, time: &str, reason: Option<&str>) -> Result<Value, String>;
}

fn tools() -> Value {
    json!([
        {
            "name": "get_session_state",
            "title": "Текущее состояние таймера",
            "description": "Что сейчас идёт: какой блок и часть, работа/перерыв/пауза/ожидание, сколько осталось, действует ли блокировка. Время — миллисекунды и готовые строки.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_today_stats",
            "title": "Статистика дня",
            "description": "Фактическое время по каждому блоку, паузы (сколько и сколько длились), доступ к заблокированному на паузе, аварийные доступы, обед, разовые сдвиги конца дня (day_end_changes). Без аргумента — сегодня.",
            "inputSchema": {
                "type": "object",
                "properties": { "date": { "type": "string", "description": "YYYY-MM-DD, по умолчанию сегодня (МСК)" } },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_plan",
            "title": "План дня",
            "description": "План сегодняшнего дня и шаблон по умолчанию, конец дня на сегодня (day_end) и из шаблона (day_end_default).",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "set_plan",
            "title": "Задать план дня",
            "description": "Заменяет план сегодняшнего дня списком блоков (блоки сопоставляются со старыми по имени). Можно добавлять, удлинять, урезать и удалять ещё не начатые блоки. Начатый блок нельзя удалить или переименовать, а пока действует блокировка — урезать меньше уже отработанного. Отработанное время не стирается. В ответе changes — что изменилось.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "blocks": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string", "description": "Предмет, например «Математика»" },
                                "minutes": { "type": "integer", "minimum": 1 },
                                "hours": { "type": "number", "exclusiveMinimum": 0, "description": "Альтернатива minutes: 1.5 = 90 мин" }
                            },
                            "required": ["name"]
                        }
                    },
                    "save_as_template": { "type": "boolean", "description": "Также сделать этот план шаблоном профиля сегодняшнего дня (Полный/Лёгкий/Выходной)" }
                },
                "required": ["blocks"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_day_end",
            "title": "Сдвинуть конец дня",
            "description": "Разово меняет время конца учебного дня только на сегодня (шаблон в настройках не трогается). Блокировка, таймеры и уведомление «день закончен» сразу считаются от нового времени. Время — МСК, позже текущего момента и не позже 02:00 следующих суток (00:00–02:00 = ночь после сегодняшнего дня). В ответе: старое и новое время, сколько плана осталось и влезает ли он до нового конца. Каждое изменение пишется в лог дня.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "time": { "type": "string", "pattern": "^\\d{1,2}:\\d{2}$", "description": "ЧЧ:ММ по Москве, например 23:00" },
                    "reason": { "type": "string", "description": "Почему сдвигаем — попадёт в лог и в уведомление" }
                },
                "required": ["time"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
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

fn parse_plan(args: &Value) -> Result<(Vec<PlanBlock>, bool), String> {
    let blocks = args.get("blocks").and_then(Value::as_array).ok_or("Нужен массив blocks.")?;
    let mut plan = vec![];
    for b in blocks {
        let name = b.get("name").and_then(Value::as_str).ok_or("У блока нет name.")?;
        let minutes = if let Some(m) = b.get("minutes").and_then(Value::as_f64) {
            m
        } else if let Some(h) = b.get("hours").and_then(Value::as_f64) {
            h * 60.0
        } else {
            return Err(format!("У блока «{name}» нет minutes или hours."));
        };
        if !(1.0..=480.0).contains(&minutes) {
            return Err(format!("«{name}»: длительность должна быть от 1 до 480 минут."));
        }
        plan.push(PlanBlock::new(name, minutes.round() as u32));
    }
    let save = args.get("save_as_template").and_then(Value::as_bool).unwrap_or(false);
    Ok((plan, save))
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
                    "serverInfo": { "name": SERVER_NAME, "title": "ClockManage — учебный таймер", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": "Учебный таймер с блокировкой отвлекалок. Используй get_session_state, чтобы узнать, что идёт сейчас, get_today_stats — фактические часы за день, set_plan — задать план дня (часы по предметам), set_day_end — разово сдвинуть конец сегодняшнего дня."
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
                "get_plan" => Ok(host.get_plan()),
                "set_plan" => parse_plan(&args).and_then(|(p, s)| host.set_plan(p, s)),
                "set_day_end" => match args.get("time").and_then(Value::as_str) {
                    Some(t) => host.set_day_end(t, args.get("reason").and_then(Value::as_str)),
                    None => Err("Нужен time в формате ЧЧ:ММ.".into()),
                },
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

    struct Fake(RefCell<Vec<PlanBlock>>);
    impl McpHost for Fake {
        fn session_state(&self) -> Value {
            json!({"phase": "work"})
        }
        fn day_stats(&self, _: Option<&str>) -> Result<Value, String> {
            Ok(json!({}))
        }
        fn get_plan(&self) -> Value {
            json!(*self.0.borrow())
        }
        fn set_plan(&self, p: Vec<PlanBlock>, _: bool) -> Result<Value, String> {
            *self.0.borrow_mut() = p;
            Ok(json!({"ok": true}))
        }
        fn set_day_end(&self, t: &str, r: Option<&str>) -> Result<Value, String> {
            Ok(json!({"new": t, "reason": r}))
        }
    }

    #[test]
    fn initialize_and_call() {
        let h = Fake(RefCell::new(vec![]));
        let r = handle(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, &h).unwrap();
        assert!(r.contains("2025-03-26"));
        assert!(handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &h).is_none());
        let r = handle(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &h).unwrap();
        assert!(r.contains("get_today_stats"));
        let r = handle(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_plan","arguments":{"blocks":[{"name":"Математика","hours":1.5},{"name":"Экстернат","minutes":150}]}}}"#,
            &h,
        )
        .unwrap();
        assert!(r.contains("\"isError\":false"));
        assert_eq!(h.0.borrow()[0].minutes, 90);
        let r = handle(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_plan","arguments":{"blocks":[{"name":"X"}]}}}"#, &h).unwrap();
        assert!(r.contains("\"isError\":true"));
        let r = handle(r#"{"jsonrpc":"2.0","id":5,"method":"tools/list"}"#, &h).unwrap();
        assert!(r.contains("set_day_end"));
        let r = handle(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"set_day_end","arguments":{"time":"23:00","reason":"разовый сдвиг"}}}"#, &h).unwrap();
        assert!(r.contains("\"isError\":false") && r.contains("разовый сдвиг"));
        let r = handle(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"set_day_end","arguments":{}}}"#, &h).unwrap();
        assert!(r.contains("\"isError\":true"));
    }
}
