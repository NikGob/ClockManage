//! Minimal MCP server (JSON-RPC 2.0 over Streamable HTTP, JSON responses only).
//!
//! The transport lives in the app; this module only maps requests to [`McpHost`] calls.

use serde_json::{json, Value};

use crate::config::{DayKind, WEEKDAYS};
use crate::day::{PlanBlock, MAX_FOCUS_MIN};

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
            "name": { "type": "string", "description": "Предмет, например «Математика», или отрезок («Обед», «Сон»)" },
            "minutes": { "type": "integer", "minimum": 1 },
            "hours": { "type": "number", "exclusiveMinimum": 0, "description": "Альтернатива minutes: 1.5 = 90 мин" },
            "type": { "type": "string", "enum": ["study", "break"], "description": "break — неучебный отрезок (обед, сон, прогулка); по умолчанию study" }
        },
        "required": ["name"]
    })
}

/// A plan item of today's `set_plan`: also "close as is".
fn today_item_schema() -> Value {
    let mut s = plan_item_schema();
    s["properties"]["close"] = json!({
        "type": "boolean",
        "description": "Только сегодня и только для начатого и ещё не закрытого учебного блока: закрыть его как есть — план блока станет фактически отработанным (целые минуты, вниз), остаток снимается из плана, день идёт дальше как после конца блока. minutes тогда не нужны. Если отработано меньше минуты — блок просто удаляется."
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
            "sites": { "type": "array", "maxItems": 50, "items": { "type": "string" }, "description": "Домены (reddit.com — вместе с поддоменами) или домен с путём (youtube.com/shorts)" },
            "apps": { "type": "array", "maxItems": 50, "items": { "type": "string" }, "description": "Имена exe: Telegram.exe, Steam (.exe допишется, путь отбрасывается)" }
        },
        "additionalProperties": false
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "get_session_state",
            "title": "Текущее состояние таймера",
            "description": "Что сейчас идёт: какой блок и часть, работа/перерыв/отрезок/пауза/ожидание (phase: work | break | segment | await | idle | done), сколько осталось, действует ли блокировка (blocking: active, day_lock, reason — в том числе focus, focus_until — конец фокус-блокировки). blocks[].done_min — отработано целыми минутами вниз: это же число — нижний предел, до которого set_plan может урезать начатый блок. Во время отрезка — segment {type, planned_min, elapsed_min, overrun_min, alarm, prep, prep_min, stopwatch, queue}: prep — идёт подготовка (у сна: кофе, дойти до кровати), отсчёта нет, ждём «Лёг» — тогда elapsed_min/overrun_min про подготовку; stopwatch — отрезок «без времени», секундомер вверх, overrun_min считается от planned_min. В blocks отрезки плана помечены type: \"break\"; taken — уже взят (идёт, прошёл или ждёт в очереди; в том числе взят вручную раньше своего места — тогда сам он второй раз не запустится). plan_forecast считает и отрезки (с подготовкой): идущий, очередь и запланированные. Время — миллисекунды и готовые строки.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_today_stats",
            "title": "Статистика дня",
            "description": "Статистика дня. blocks[] — только учебные блоки: planned_min, actual_min, journal_hours (actual_min вниз до 0,25 ч), note (строка «что было скучно / куда отвлекался»); journal_total — сумма journal_hours. breaks[] — неучебные отрезки (обед, сон, прогулка, свои): {type, planned_min, actual_min, overrun_min, start, end, prep_min, prep_planned_min, lay_at, stopwatch}; в учебные часы и journal_hours не входят. У отрезка с подготовкой (сон) actual_min — только сам сон от «Лёг» (lay_at), prep_min — подготовка отдельно (null — подготовки не было); закончен до «Лёг» — actual_min 0. stopwatch: true — шёл «без времени» (секундомер), overrun_min всё равно от planned_min. Ещё: паузы (короче 10 с не пишутся), доступ на паузе, аварийные доступы, сдвиги конца дня (day_end_changes), фокус-блокировки (focus_locks: at, until, minutes, reason, by). Без аргумента — сегодня.",
            "inputSchema": {
                "type": "object",
                "properties": { "date": { "type": "string", "description": "YYYY-MM-DD, по умолчанию сегодня (МСК)" } },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_week_stats",
            "title": "Неделя для журнала",
            "description": "Часы для журнала за неделю (пн–вс), в которую попадает date: subjects[] — предмет и journal_hours по дням (блоки с одинаковым именем складываются; каждый блок округлён вниз до 0,25 ч), day_totals, total, actual_min (неокруглённый факт). tsv — та же таблица текстом с табуляцией для вставки в таблицу. Отрезки (обед, сон) не входят.",
            "inputSchema": {
                "type": "object",
                "properties": { "date": { "type": "string", "description": "Любой день недели, YYYY-MM-DD; по умолчанию текущая неделя" } },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_plan",
            "title": "План дня",
            "description": "План сегодняшнего дня по порядку (today: учебные блоки и неучебные отрезки с type: \"break\"), профиль сегодняшнего дня (profile) и его шаблон (template), конец дня на сегодня (day_end) и из настроек (day_end_default), типы отрезков (segment_types: name, minutes, alarm). Ещё: templates — шаблоны всех профилей (full/light/off: plan и blocking — включается ли блокировка); week — профиль каждого дня недели (mon…sun); upcoming — следующие 7 дней: date, weekday, profile, plan и source (\"date\" — план задан на эту дату через set_plan с date, \"template\" — шаблон профиля). С date — ещё day: план и профиль этого дня.",
            "inputSchema": {
                "type": "object",
                "properties": { "date": { "type": "string", "description": "YYYY-MM-DD — какой день показать в day (сегодня или позже)" } },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "set_plan",
            "title": "Задать план дня",
            "description": "Без date — заменяет план сегодняшнего дня списком пунктов по порядку дня. Пункты сопоставляются со старыми по имени (регистр не важен) и типу в любом порядке: переставлять можно всё — неначатые и начатые блоки, отрезки; отработанное едет вместе с блоком. Учебный блок: {name, minutes|hours}. Неучебный отрезок (обед, сон, прогулка…): {name, minutes, type: \"break\"} — стоит на своём месте: когда закрывается блок перед ним, вместо перерыва между блоками запускается этот отрезок (у сна сначала подготовка без отсчёта до кнопки «Лёг», будильник — от «Лёг»). Отрезок, взятый вручную раньше своего места, гасит запланированный того же типа. Отрезки не входят в учебные часы и journal_hours, но учитываются в plan_forecast (с подготовкой). Правила. Неначатое — добавлять, удлинять, урезать, удалять. Отрезки, даже прошедшие, можно удалять — их запись в breaks[] остаётся. Начатый блок нельзя удалить или переименовать — кроме блока, где отработано меньше 1 минуты (случайный старт): он удаляется. Пока действует блокировка, начатый блок нельзя урезать ниже отработанного; предел — целые минуты вниз, ровно done_min из get_session_state (10 мин 40 с → можно 10). Закрыть начатый блок как есть — пункт {name, close: true}: план блока = отработанное, остаток снимается, день идёт дальше; так закрывай только по просьбе пользователя (текущий блок без явной просьбы — через finish_block, он спросит). Всё или ничего: при ошибке план не меняется. Отработанное время не стирается. В ответе changes (added | removed | shortened | lengthened | moved | closed) и plan_forecast. С date позже сегодняшнего — план только на тот день: он начнётся с этого плана вместо шаблона своего профиля, сегодняшний план и шаблоны не меняются (до 60 дней вперёд; профиль дня — по расписанию недели, см. get_plan → week). use_template: true с date — убрать план, заданный на эту дату (день начнётся с шаблона).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "blocks": { "type": "array", "minItems": 1, "maxItems": 16, "items": today_item_schema() },
                    "date": { "type": "string", "description": "YYYY-MM-DD; по умолчанию сегодня. Позже сегодняшнего — план на тот день" },
                    "save_as_template": { "type": "boolean", "description": "Также сделать этот план шаблоном профиля этого дня (сегодняшнего или дня из date): Полный/Лёгкий/Выходной" },
                    "use_template": { "type": "boolean", "description": "Только с date: убрать план, заданный на эту дату; blocks тогда не нужен" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_template",
            "title": "Задать шаблон профиля",
            "description": "Заменяет шаблон профиля дня — план, с которого начинается каждый день этого профиля. В любой день и даже во время учёбы: начатый сегодняшний день не меняется. Если сегодня этот же профиль, день ещё не начат и его план совпадал со старым шаблоном, сегодняшний план тоже станет новым (как на экране «План»; в ответе today_plan_updated). Даты, для которых план задан отдельно (set_plan с date), начнутся со своего плана — они в ответе в overridden_dates. Формат blocks тот же, что в set_plan, включая отрезки type: \"break\"; пустой массив — пустой шаблон.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profile": { "type": "string", "enum": ["full", "light", "off"], "description": "full — Полный, light — Лёгкий, off — Выходной" },
                    "blocks": { "type": "array", "maxItems": 16, "items": plan_item_schema() }
                },
                "required": ["profile", "blocks"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_week_schedule",
            "title": "Профили дней недели",
            "description": "Какой профиль у дней недели (full — Полный, light — Лёгкий, off — Выходной). Меняются только перечисленные дни. Текущее расписание — get_plan → week. Если меняется сегодняшний день недели, а день ещё не начат, сегодня тоже переключается на новый профиль и его шаблон; начатый день не меняется. В ответе week — расписание целиком.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {
                        "type": "object",
                        "description": "Например {\"mon\": \"full\", \"sat\": \"light\"}",
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
        },
        {
            "name": "set_block_note",
            "title": "Строка в конце блока",
            "description": "Сохраняет строку пользователя о блоке: что было скучно, куда отвлекался (до 300 символов). Попадает в лог и в get_today_stats как blocks[].note. Без name — последний закрытый блок. Повторный вызов заменяет строку.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Блок; по умолчанию последний закрытый" },
                    "note": { "type": "string", "description": "Слова пользователя, по возможности дословно" }
                },
                "required": ["note"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "finish_block",
            "title": "Закрыть блок сейчас",
            "description": "Закрывает начатый блок прямо сейчас на фактически отработанных минутах: planned_min блока становится равным отработанному в целых минутах вниз (как done_min), нужна хотя бы 1 минута — блок с меньшим просто удаляется через set_plan. День идёт дальше как после обычного конца блока (перерыв между блоками или конец дня), всё пишется в лог. Убирает гонку «урезать set_plan, пока таймер идёт». ДВА ШАГА. Первый вызов (без confirm_token) ничего не меняет: возвращает предпросмотр (сколько отработано, сколько уйдёт из плана) и одноразовый confirm_token на 2 минуты. Перед вторым вызовом ОБЯЗАТЕЛЬНО задай пользователю прямой вопрос из поля ask_user и дождись явного «да». Только потом вызови finish_block с confirm_token. Не подтверждай за пользователя, не делай второй вызов по своей инициативе и не трактуй общие фразы («давай дальше», «ок») как согласие. Если пользователь сам просит перекроить день и закрыть блок в том же шаге — set_plan с close: true.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Какой блок закрыть; по умолчанию текущий" },
                    "confirm_token": { "type": "string", "description": "Токен из первого вызова — только после явного «да» пользователя" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false }
        },
        {
            "name": "get_blocklist",
            "title": "Блок-лист",
            "description": "Что блокируется и действует ли блокировка сейчас. sites — домены (x.com, вместе с поддоменами) и пути (youtube.com/shorts — только политики браузеров, остальной сайт открыт); apps — приложения Windows по имени exe (Telegram.exe): пока идёт блокировка, запущенные закрываются, новые не дают запуститься; phone_apps — приложения телефона (только чтение, выбираются на телефоне). blocking: active — сайты и приложения закрыты прямо сейчас; day_lock — действует блокировка (учебный день, одиночный таймер с блокировкой или фокус-блокировка), возможно временно открытая доступом; reason — study | single | focus | emergency | pause_access | lunch_at_pc | segment_access | not_started | not_study_day | completed | day_end; until — конец открытого доступа; focus_until — конец фокус-блокировки (start_focus_lock). can_remove — можно ли сейчас убирать из списка (только вне блокировки). admin — запущена ли программа с правами администратора: без них сайты не блокируются.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "add_to_blocklist",
            "title": "Добавить в блок-лист",
            "description": "Добавляет сайты и/или приложения в блок-лист. Можно всегда, в том числе во время блокировки: тогда новое закрывается сразу (сайты — через hosts и политики браузеров, приложение — закрывается, если запущено, и не даёт себя запустить). Сайт: домен (reddit.com — с поддоменами) или домен с путём (youtube.com/shorts — без пути закрывается весь сайт); https://, www. и / в конце убираются. Приложение: имя exe (Steam или Steam.exe; путь отбрасывается); системные процессы (explorer.exe и т. п.) и сам ClockManage не блокируются. Список действует, пока идёт учебный день с блокировкой, одиночный таймер с блокировкой или фокус-блокировка (start_focus_lock) — само добавление вне этого времени ничего не закрывает. В ответе: added (что добавилось, в сохранённом виде), unchanged (уже было), invalid (не сайт / системное приложение), blocklist целиком и blocking.",
            "inputSchema": blocklist_schema(),
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "remove_from_blocklist",
            "title": "Убрать из блок-листа",
            "description": "Убирает сайты и/или приложения из блок-листа. Только вне блокировки — до «Начать день», после конца дня или когда все блоки отсижены, без одиночного таймера с блокировкой и без фокус-блокировки (get_blocklist → can_remove). Во время блокировки — отказ: список можно только расширять. Убирай только по явной просьбе пользователя. Записи сравниваются без учёта регистра после той же очистки, что при добавлении; youtube.com не убирает youtube.com/shorts — это разные записи. В ответе: removed, unchanged (такого в списке не было), invalid, blocklist целиком.",
            "inputSchema": blocklist_schema(),
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true }
        },
        {
            "name": "start_focus_lock",
            "title": "Фокус-блокировка",
            "description": "Включает блокировку всего блок-листа (сайты и приложения; на подключённом телефоне — тоже) прямо сейчас на minutes минут — в любой день: учебный день не начат, уже закончен, выходной — неважно. Снять её раньше нельзя ни агенту, ни в приложении — только аварийным доступом самого пользователя (длинная фраза руками, пишется в лог); доступ на паузе, обед за ПК и «Доступ» у отрезков её не открывают. Пока она идёт, блок-лист можно только расширять и выйти из программы нельзя. Повторный вызов только продлевает: minutes считаются от текущего момента, и новый конец должен быть позже текущего, иначе отказ. Если блокировка идёт за полночь, она переходит на следующий день. Включай только по явной просьбе пользователя и на тот срок, который он назвал (до 720 минут за раз); чтобы закрыть то, чего нет в списке, сначала add_to_blocklist. Учебный день, план и таймер она не трогает. В ответе: until (ЧЧ:ММ по времени программы), until_ms, minutes_left, extended и from (старый конец при продлении), blocklist и blocking. Запуск и каждое продление пишутся в лог дня и в get_today_stats → focus_locks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "minutes": { "type": "integer", "minimum": 1, "maximum": MAX_FOCUS_MIN, "description": "Сколько минут блокировать, начиная с текущего момента" },
                    "hours": { "type": "number", "exclusiveMinimum": 0, "description": "Альтернатива minutes: 1.5 = 90 мин" },
                    "reason": { "type": "string", "description": "Зачем, словами пользователя — попадёт в лог и в уведомление" }
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
        return Err("close работает только в set_plan на сегодня: закрыть можно начатый блок сегодняшнего дня.".into());
    }
    Ok(plan)
}

/// Plan items and their "close as is" flags.
fn parse_items(args: &Value) -> Result<(Vec<PlanBlock>, Vec<bool>), String> {
    let blocks = args.get("blocks").and_then(Value::as_array).ok_or("Нужен массив blocks.")?;
    if blocks.len() > 16 {
        return Err("Не больше 16 пунктов плана в день.".into());
    }
    let mut plan = vec![];
    let mut close = vec![];
    for b in blocks {
        let name = b.get("name").and_then(Value::as_str).ok_or("У блока нет name.")?;
        let closing = b.get("close").and_then(Value::as_bool).unwrap_or(false);
        let is_break = match b.get("type").and_then(Value::as_str).unwrap_or("study") {
            "break" => true,
            "study" => false,
            other => return Err(format!("«{name}»: type бывает \"study\" или \"break\", а не «{other}».")),
        };
        if closing && is_break {
            return Err(format!("«{name}»: close закрывает учебный блок, а это отрезок (type: \"break\")."));
        }
        let minutes = if let Some(m) = b.get("minutes").and_then(Value::as_f64) {
            m
        } else if let Some(h) = b.get("hours").and_then(Value::as_f64) {
            h * 60.0
        } else if closing {
            // Closing sets the length to the minutes worked.
            1.0
        } else {
            return Err(format!("У блока «{name}» нет minutes или hours."));
        };
        let max = if is_break { 240.0 } else { 480.0 };
        if !(1.0..=max).contains(&minutes) {
            return Err(format!("«{name}»: длительность должна быть от 1 до {max} минут."));
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
        return Err("use_template работает только с date: какой день вернуть к шаблону.".into());
    }
    let (plan, close) = if use_template { (vec![], vec![]) } else { parse_items(args)? };
    if plan.is_empty() && !use_template {
        return Err("В плане нужен хотя бы один пункт.".into());
    }
    let save_as_template = flag("save_as_template") && !use_template;
    if save_as_template && close.iter().any(|c| *c) {
        return Err("close и save_as_template вместе нельзя: шаблон получил бы урезанный блок. Сначала закрой блок, шаблон задай отдельно (set_template).".into());
    }
    Ok(PlanRequest { plan, save_as_template, date, use_template, close })
}

fn parse_template(args: &Value) -> Result<(DayKind, Vec<PlanBlock>), String> {
    let p = args.get("profile").and_then(Value::as_str).ok_or("Нужен profile: full, light или off.")?;
    let kind = DayKind::from_key(p).ok_or(format!("profile бывает full, light или off, а не «{p}»."))?;
    Ok((kind, parse_blocks(args)?))
}

fn parse_week(args: &Value) -> Result<Vec<(usize, DayKind)>, String> {
    let days = args.get("days").and_then(Value::as_object).ok_or("Нужен объект days, например {\"mon\": \"full\"}.")?;
    let mut out = vec![];
    for (k, v) in days {
        let i = WEEKDAYS.iter().position(|d| d == k).ok_or(format!("День недели — mon…sun, а не «{k}»."))?;
        let p = v.as_str().unwrap_or("");
        let kind = DayKind::from_key(p).ok_or(format!("{k}: профиль бывает full, light или off, а не «{p}»."))?;
        out.push((i, kind));
    }
    if out.is_empty() {
        return Err("В days нет ни одного дня.".into());
    }
    Ok(out)
}

/// `sites` and `apps` of the block-list tools; at least one entry.
fn parse_lists(args: &Value) -> Result<(Vec<String>, Vec<String>), String> {
    let list = |k: &str| -> Result<Vec<String>, String> {
        let bad = || format!("{k}: нужен массив строк, например [\"reddit.com\"].");
        match args.get(k) {
            None | Some(Value::Null) => Ok(vec![]),
            Some(Value::Array(a)) => a.iter().map(|v| v.as_str().map(str::to_string).ok_or_else(bad)).collect(),
            Some(_) => Err(bad()),
        }
    };
    let (sites, apps) = (list("sites")?, list("apps")?);
    if sites.is_empty() && apps.is_empty() {
        return Err("Нужен sites или apps — хотя бы одна запись.".into());
    }
    if sites.len() > 50 || apps.len() > 50 {
        return Err("Не больше 50 сайтов и 50 приложений за раз.".into());
    }
    Ok((sites, apps))
}

fn parse_focus(args: &Value) -> Result<u32, String> {
    let minutes = if let Some(m) = args.get("minutes").and_then(Value::as_f64) {
        m
    } else if let Some(h) = args.get("hours").and_then(Value::as_f64) {
        h * 60.0
    } else {
        return Err("Нужен minutes (или hours): на сколько включить блокировку.".into());
    };
    if !(1.0..=MAX_FOCUS_MIN as f64).contains(&minutes) {
        return Err(format!("Фокус-блокировка — от 1 до {MAX_FOCUS_MIN} минут ({} ч) за раз.", MAX_FOCUS_MIN / 60));
    }
    Ok(minutes.round() as u32)
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
                    "instructions": "Учебный таймер с блокировкой отвлекалок. get_session_state — что идёт сейчас (блок, перерыв, отрезок вроде обеда или сна). get_week_stats — часы для журнала за неделю по предметам и дням (с готовым tsv). get_today_stats — фактические часы за день: blocks[].journal_hours (вниз до 0,25 ч) и journal_total для журнала, blocks[].note — строки «что было скучно», breaks[] — обед, сон, прогулки. get_plan — план на сегодня, шаблоны всех профилей, расписание недели и планы следующих 7 дней. set_plan — план дня (учебные блоки и отрезки type: \"break\" на своих местах; переставлять можно всё, блок со случайным стартом меньше минуты — удалять, начатый — закрыть как есть через close: true по просьбе пользователя); с date — план на будущий день, сегодняшний не трогается. Отработанное везде в целых минутах вниз: done_min = предел урезания. set_template — шаблон любого профиля (full/light/off). set_week_schedule — какой профиль у дней недели. set_day_end — разово сдвинуть конец сегодняшнего дня. set_block_note — записать строку пользователя о блоке. finish_block — закрыть начатый блок на отработанном: два шага, второй только после явного «да» пользователя. get_blocklist — что в блок-листе (сайты, приложения .exe) и действует ли блокировка. add_to_blocklist — добавить сайты и приложения (можно всегда, во время блокировки закрываются сразу); remove_from_blocklist — убрать (только вне блокировки и по просьбе пользователя). start_focus_lock — заблокировать весь блок-лист прямо сейчас на N минут в любой день; снять раньше нельзя (только аварийный доступ пользователя), можно только продлить — включай только по явной просьбе пользователя."
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
                    None => Err("Нужен note.".into()),
                },
                "finish_block" => host.finish_block(args.get("name").and_then(Value::as_str), args.get("confirm_token").and_then(Value::as_str)),
                "set_day_end" => match args.get("time").and_then(Value::as_str) {
                    Some(t) => host.set_day_end(t, args.get("reason").and_then(Value::as_str)),
                    None => Err("Нужен time в формате ЧЧ:ММ.".into()),
                },
                "get_blocklist" => Ok(host.get_blocklist()),
                "add_to_blocklist" => parse_lists(&args).and_then(|(s, a)| host.edit_blocklist(false, s, a)),
                "remove_from_blocklist" => parse_lists(&args).and_then(|(s, a)| host.edit_blocklist(true, s, a)),
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
