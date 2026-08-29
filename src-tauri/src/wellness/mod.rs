use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

pub type Cmd<T> = Result<T, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WellnessCategory {
    Blink,
    Water,
    Stretch,
    Posture,
    Eyes,
    Meal,
}

impl std::fmt::Display for WellnessCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blink => write!(f, "blink"),
            Self::Water => write!(f, "water"),
            Self::Stretch => write!(f, "stretch"),
            Self::Posture => write!(f, "posture"),
            Self::Eyes => write!(f, "eyes"),
            Self::Meal => write!(f, "meal"),
        }
    }
}

impl std::str::FromStr for WellnessCategory {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "blink" => Ok(Self::Blink),
            "water" => Ok(Self::Water),
            "stretch" => Ok(Self::Stretch),
            "posture" => Ok(Self::Posture),
            "eyes" => Ok(Self::Eyes),
            "meal" => Ok(Self::Meal),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WellnessConfig {
    pub category: String,
    pub enabled: bool,
    pub interval_minutes: i64,
    pub last_reminded_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WellnessReminder {
    pub category: String,
    pub title: String,
    pub message: String,
    pub emoji: String,
    pub duration_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WellnessState {
    pub enabled: bool,
    pub next_reminder_in_secs: i64,
    pub last_category: Option<String>,
    pub snoozed_until: Option<String>,
}

impl Default for WellnessState {
    fn default() -> Self {
        Self {
            enabled: true,
            next_reminder_in_secs: 600,
            last_category: None,
            snoozed_until: None,
        }
    }
}

pub struct WellnessAgent {
    app: AppHandle,
    state: Arc<Mutex<WellnessState>>,
}

impl WellnessAgent {
    pub fn new(app: AppHandle) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            app,
            state: Arc::new(Mutex::new(WellnessState::default())),
        }))
    }

    pub fn start(agent: Arc<Mutex<Self>>) {
        std::thread::spawn(move || {
            loop {
                Self::tick(&agent);
                std::thread::sleep(Duration::from_secs(60));
            }
        });
    }

    fn tick(agent: &Arc<Mutex<Self>>) {
        let state = {
            let s = agent.lock().unwrap();
            let x = s.state.lock().unwrap().clone(); 
            x
        };
        if !state.enabled {
            return;
        }

        if let Some(snoozed) = &state.snoozed_until {
            if Utc::now().to_rfc3339() < *snoozed {
                return;
            }
        }

        let app = {
            let s = agent.lock().unwrap();
            s.app.clone()
        };
        let agent = agent.clone();

        tauri::async_runtime::spawn_blocking(move || {
            let guard = agent.lock().unwrap();
            if let Ok(conn) = guard.db_conn(&app) {
                if let Some(reminder) = guard.next_due(&conn) {
                    let _ = app.emit("wellness:notify", reminder.clone());
                    let _ = app.emit("wellness:popup", reminder);
                }
            }
        });
    }

    fn db_conn(&self, app: &AppHandle) -> Result<rusqlite::Connection, String> {
        let state = app.state::<Arc<crate::state::AppState>>();
        let st = state.inner();
        let path = st.db_path();
        rusqlite::Connection::open(&path).map_err(|e| e.to_string())
    }

    fn next_due(&self, conn: &rusqlite::Connection) -> Option<WellnessReminder> {
        let categories: Vec<WellnessCategory> = vec![
            WellnessCategory::Blink,
            WellnessCategory::Water,
            WellnessCategory::Stretch,
            WellnessCategory::Posture,
            WellnessCategory::Eyes,
            WellnessCategory::Meal,
        ];

        let mut eligible: Vec<(WellnessCategory, i64, Option<String>)> = Vec::new();

        for cat in &categories {
            let (enabled, interval, last) = self.load_category(conn, cat);
            if !enabled {
                continue;
            }

            let due = if let Some(last_str) = &last {
                if let Ok(last_dt) = DateTime::parse_from_rfc3339(last_str) {
                    let mins = Utc::now().signed_duration_since(last_dt.with_timezone(&Utc)).num_minutes();
                    mins >= interval
                } else {
                    true
                }
            } else {
                true
            };

            if due {
                eligible.push((cat.clone(), interval, last));
            }
        }

        if eligible.is_empty() {
            return None;
        }

        let (cat, _, _) = &eligible[0];
        self.mark_reminded(conn, cat);

        Some(match cat {
            WellnessCategory::Blink => WellnessReminder {
                category: cat.to_string(),
                title: "👀 Blink your eyes".to_string(),
                message: "It's been 10 minutes — blink deliberately for a few seconds.".to_string(),
                emoji: "👀".to_string(),
                duration_secs: 5,
            },
            WellnessCategory::Water => WellnessReminder {
                category: cat.to_string(),
                title: "💧 Drink some water".to_string(),
                message: "45 minutes of focus — thoda paani pe le boss.".to_string(),
                emoji: "💧".to_string(),
                duration_secs: 6,
            },
            WellnessCategory::Stretch => WellnessReminder {
                category: cat.to_string(),
                title: "🧍 Stand & stretch".to_string(),
                message: "30 minutes up — utho, stretch karo, back le lo.".to_string(),
                emoji: "🧍".to_string(),
                duration_secs: 6,
            },
            WellnessCategory::Posture => WellnessReminder {
                category: cat.to_string(),
                title: "🪴 Posture check".to_string(),
                message: "60 minutes — seedha baith, shoulders down, neck relaxed.".to_string(),
                emoji: "🪴".to_string(),
                duration_secs: 5,
            },
            WellnessCategory::Eyes => WellnessReminder {
                category: cat.to_string(),
                title: "👁️ Eye break".to_string(),
                message: "20-20-20 rule: 20 feet ki jagah 20 seconds tak dekho.".to_string(),
                emoji: "👁️".to_string(),
                duration_secs: 5,
            },
            WellnessCategory::Meal => {
                let now = Utc::now();
                let hour = now.hour();
                let (msg, title) = if hour >= 7 && hour < 10 {
                    ("Breakfast ka time ho gaya — khaye kuch fresh.".to_string(), "🍳 Breakfast time".to_string())
                } else if hour >= 13 && hour < 15 {
                    ("Lunch ka time — dhyan se khaye, light rakhe.".to_string(), "🍱 Lunch time".to_string())
                } else if hour >= 19 && hour < 22 {
                    ("Dinner ka time — thoda jaldi khao, light rakhe.".to_string(), "🍽️ Dinner time".to_string())
                } else {
                    ("Snack le lo ya paani pe lo — energy sustain kar.".to_string(), "🥪 Snack / hydrate".to_string())
                };
                WellnessReminder {
                    category: cat.to_string(),
                    title,
                    message: msg,
                    emoji: "🍴".to_string(),
                    duration_secs: 6,
                }
            }
        })
    }

    fn load_category(&self, conn: &rusqlite::Connection, cat: &WellnessCategory) -> (bool, i64, Option<String>) {
        let cat_str = cat.to_string();
        let mut stmt = conn.prepare_cached(
            "SELECT enabled, interval_minutes, last_reminded_at FROM wellness_config WHERE category = ?1"
        ).ok();

        if let Some(s) = &mut stmt {
            if let Ok(row) = s.query_row([&cat_str], |r| {
                Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?))
            }) {
                return row;
            }
        }

        let interval = match cat {
            WellnessCategory::Blink => 10,
            WellnessCategory::Water => 45,
            WellnessCategory::Stretch => 30,
            WellnessCategory::Posture => 60,
            WellnessCategory::Eyes => 20,
            WellnessCategory::Meal => 180,
        };

        (true, interval, None)
    }

    fn mark_reminded(&self, conn: &rusqlite::Connection, cat: &WellnessCategory) {
        let now = Utc::now().to_rfc3339();
        let cat_str = cat.to_string();
        let _ = conn.execute(
            "INSERT INTO wellness_config(category, enabled, interval_minutes, last_reminded_at) VALUES(?1, 1, (SELECT interval_minutes FROM wellness_config WHERE category = ?1), ?2)
             ON CONFLICT(category) DO UPDATE SET last_reminded_at = excluded.last_reminded_at",
            [&cat_str, &now],
        );
    }

    pub fn get_state(&self, _app: &AppHandle) -> Cmd<WellnessState> {
        Ok(self.state.lock().unwrap().clone())
    }

    pub fn set_enabled(&self, _app: &AppHandle, enabled: bool) -> Cmd<()> {
        self.state.lock().unwrap().enabled = enabled;
        Ok(())
    }

    pub fn snooze(&self, _app: &AppHandle, minutes: i64) -> Cmd<()> {
        let until = Utc::now() + chrono::Duration::minutes(minutes);
        self.state.lock().unwrap().snoozed_until = Some(until.to_rfc3339());
        Ok(())
    }

    pub fn get_config(&self, app: &AppHandle) -> Cmd<Vec<WellnessConfig>> {
        let conn = self.db_conn(app)?;
        let mut stmt = conn.prepare("SELECT category, enabled, interval_minutes, last_reminded_at FROM wellness_config").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| {
            Ok(WellnessConfig {
                category: r.get(0)?,
                enabled: r.get::<_, i64>(1)? != 0,
                interval_minutes: r.get(2)?,
                last_reminded_at: r.get(3)?,
            })
        }).map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn set_category(&self, app: &AppHandle, category: String, enabled: bool, interval_minutes: i64) -> Cmd<()> {
        let conn = self.db_conn(app)?;
        let enabled_val: String = if enabled { "1".into() } else { "0".into() };
        let interval_str: String = interval_minutes.to_string();
        conn.execute(
            "INSERT INTO wellness_config(category, enabled, interval_minutes, last_reminded_at) VALUES(?1, ?2, ?3, NULL)
             ON CONFLICT(category) DO UPDATE SET enabled = excluded.enabled, interval_minutes = excluded.interval_minutes",
            [&category, &enabled_val, &interval_str],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn record_activity(&self, app: &AppHandle, source: &str) -> Cmd<()> {
        let conn = self.db_conn(app)?;
        let _ = conn.execute(
            "INSERT INTO wellness_activity(ts, source) VALUES(?1, ?2)",
            [&Utc::now().to_rfc3339(), &source.to_string()],
        );
        Ok(())
    }

    pub fn dismiss(&self) {
        let mut state = self.state.lock().unwrap();
        state.last_category = None;
        state.snoozed_until = None;
    }
}
