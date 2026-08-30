import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import type { wellness } from "../../lib/api";

const CATEGORIES: { key: string; label: string; emoji: string; defaultInterval: number }[] = [
  { key: "blink", label: "Blink eyes", emoji: "👀", defaultInterval: 10 },
  { key: "water", label: "Drink water", emoji: "💧", defaultInterval: 45 },
  { key: "stretch", label: "Stretch", emoji: "🧍", defaultInterval: 30 },
  { key: "posture", label: "Posture", emoji: "🪴", defaultInterval: 60 },
  { key: "eyes", label: "Eye break", emoji: "👁️", defaultInterval: 20 },
  { key: "meal", label: "Meal / snack", emoji: "🍴", defaultInterval: 180 },
];

export function WellnessCard() {
  const [enabled, setEnabled] = useState(true);
  const [configs, setConfigs] = useState<wellness.WellnessConfig[]>([]);
  const [busy, setBusy] = useState(false);
  const [snooze, setSnooze] = useState("");

  const refresh = async () => {
    setBusy(true);
    try {
      const [state, cfg] = await Promise.all([
        api.wellnessGetState(),
        api.wellnessGetConfig(),
      ]);
      setEnabled(state.enabled);
      setConfigs(cfg);
    } catch {
      // ignore
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const toggle = async (category: string, current: boolean) => {
    setBusy(true);
    try {
      const interval =
        configs.find((c) => c.category === category)?.intervalMinutes ??
        CATEGORIES.find((c) => c.key === category)?.defaultInterval ??
        10;
      await api.wellnessSetCategory(category, !current, interval);
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const setInterval = async (category: string, mins: number) => {
    setBusy(true);
    try {
      const current = configs.find((c) => c.category === category)?.enabled ?? true;
      await api.wellnessSetCategory(category, current, mins);
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const snoozeMins = async () => {
    const m = parseInt(snooze, 10);
    if (!m || m <= 0) return;
    setBusy(true);
    try {
      await api.wellnessSnooze(m);
      setSnooze("");
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const map = useMemo(() => {
    const m = new Map<string, wellness.WellnessConfig>();
    for (const c of configs) m.set(c.category, c);
    return m;
  }, [configs]);

  return (
    <section className="panel" aria-label="Wellness Agent">
      <h3 className="panel-title">🧠 Wellness Agent</h3>
      <p className="text-dim" style={{ fontSize: 12.5, margin: "4px 0 10px" }}>
        Mid-top popups remind you to blink, hydrate, stretch, and eat. Works even when Strawberry is minimized.
      </p>

      <div className="quick-row" style={{ marginBottom: 10 }}>
        <button
          className={"btn primary" + (enabled ? " active" : "")}
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              await api.wellnessSetEnabled(!enabled);
              await refresh();
            } finally {
              setBusy(false);
            }
          }}
        >
          {enabled ? "✅ Agent ON" : "💤 Agent OFF"}
        </button>

        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input
            className="quick-input"
            style={{ width: 90 }}
            placeholder="Snooze (min)"
            value={snooze}
            onChange={(e) => setSnooze(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void snoozeMins()}
          />
          <button className="btn" disabled={busy || !snooze} onClick={() => void snoozeMins()}>
            Snooze
          </button>
        </div>
      </div>

      <div style={{ display: "grid", gap: 8 }}>
        {CATEGORIES.map((cat) => {
          const cfg = map.get(cat.key);
          const isEnabled = cfg?.enabled ?? true;
          const interval = cfg?.intervalMinutes ?? cat.defaultInterval;
          return (
            <div
              key={cat.key}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "6px 8px",
                borderRadius: 10,
                background: "rgba(255,255,255,0.04)",
                border: "1px solid rgba(255,255,255,0.08)",
              }}
            >
              <span style={{ fontSize: 18 }} title={cat.label}>{cat.emoji}</span>
              <label className="todo-row" style={{ flex: 1, cursor: "pointer" }}>
                <input
                  type="checkbox"
                  checked={isEnabled}
                  onChange={() => void toggle(cat.key, isEnabled)}
                />
                <span className="todo-title">{cat.label}</span>
              </label>
              <input
                className="quick-input"
                style={{ width: 70, textAlign: "center" }}
                type="number"
                min={1}
                max={240}
                value={interval}
                disabled={!isEnabled}
                onChange={(e) =>
                  void setInterval(cat.key, Math.max(1, parseInt(e.target.value || "10", 10)))
                }
              />
              <span className="text-dim" style={{ fontSize: 11 }}>min</span>
            </div>
          );
        })}
      </div>
    </section>
  );
}
