import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import type { wellness } from "../../lib/api";

type Unit = "sec" | "min" | "hour";

const CATEGORIES: { key: string; label: string; emoji: string; defaultSeconds: number }[] = [
  { key: "blink",   label: "Blink eyes",   emoji: "👀", defaultSeconds: 10 * 60 },
  { key: "water",   label: "Drink water",  emoji: "💧", defaultSeconds: 45 * 60 },
  { key: "stretch", label: "Stretch",      emoji: "🧍", defaultSeconds: 30 * 60 },
  { key: "posture", label: "Posture",      emoji: "🪴", defaultSeconds: 60 * 60 },
  { key: "eyes",    label: "Eye break",    emoji: "👁️", defaultSeconds: 20 * 60 },
  { key: "meal",    label: "Meal / snack", emoji: "🍴", defaultSeconds: 180 * 60 },
];

const UNITS: { key: Unit; label: string; factor: number }[] = [
  { key: "sec",  label: "sec",  factor: 1 },
  { key: "min",  label: "min",  factor: 60 },
  { key: "hour", label: "hour", factor: 3600 },
];

function bestUnit(totalSeconds: number): { value: number; unit: Unit } {
  if (totalSeconds <= 0) return { value: 1, unit: "sec" };
  if (totalSeconds >= 3600 && totalSeconds % 3600 === 0) return { value: totalSeconds / 3600, unit: "hour" };
  if (totalSeconds >= 60 && totalSeconds % 60 === 0)     return { value: totalSeconds / 60,   unit: "min" };
  return { value: Math.max(1, totalSeconds), unit: "sec" };
}

function toSeconds(value: number, unit: Unit): number {
  const v = Math.max(1, Math.floor(value || 1));
  return v * UNITS.find((u) => u.key === unit)!.factor;
}

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
      const secs =
        configs.find((c) => c.category === category)?.intervalSeconds ??
        CATEGORIES.find((c) => c.key === category)?.defaultSeconds ??
        600;
      await api.wellnessSetCategory(category, !current, secs);
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const setInterval = async (category: string, secs: number) => {
    setBusy(true);
    try {
      const current = configs.find((c) => c.category === category)?.enabled ?? true;
      await api.wellnessSetCategory(category, current, secs);
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
        Mid-top popups remind you to blink, hydrate, stretch, and eat. Works even
        when Strawberry is minimized. Each reminder repeats on a fully
        customizable interval (seconds / minutes / hours).
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

        <button
          className="btn"
          disabled={busy}
          title="Fire a test reminder popup right now"
          onClick={() => void api.wellnessTestPopup("blink")}
        >
          🧪 Test popup
        </button>

        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input
            className="quick-input"
            style={{ width: 110 }}
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
          const totalSecs = cfg?.intervalSeconds ?? cat.defaultSeconds;
          const { value: shownValue, unit: shownUnit } = bestUnit(totalSecs);
          return (
            <IntervalRow
              key={cat.key}
              emoji={cat.emoji}
              label={cat.label}
              isEnabled={isEnabled}
              value={shownValue}
              unit={shownUnit}
              onToggle={() => void toggle(cat.key, isEnabled)}
              onChange={(v, u) => void setInterval(cat.key, toSeconds(v, u))}
            />
          );
        })}
      </div>
    </section>
  );
}

interface IntervalRowProps {
  emoji: string;
  label: string;
  isEnabled: boolean;
  value: number;
  unit: Unit;
  onToggle: () => void;
  onChange: (value: number, unit: Unit) => void;
}

function IntervalRow({ emoji, label, isEnabled, value, unit, onToggle, onChange }: IntervalRowProps) {
  // local state so typing in the input is responsive; we commit on blur/Enter.
  const [draft, setDraft] = useState<string>(String(value));
  const [draftUnit, setDraftUnit] = useState<Unit>(unit);
  useEffect(() => { setDraft(String(value)); }, [value]);
  useEffect(() => { setDraftUnit(unit); }, [unit]);

  const commit = () => {
    const n = Math.max(1, parseInt(draft, 10) || 1);
    setDraft(String(n));
    onChange(n, draftUnit);
  };

  return (
    <div
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
      <span style={{ fontSize: 18 }} title={label}>{emoji}</span>
      <label className="todo-row" style={{ flex: 1, cursor: "pointer" }}>
        <input type="checkbox" checked={isEnabled} onChange={onToggle} />
        <span className="todo-title">{label}</span>
      </label>
      <input
        className="quick-input"
        style={{ width: 64, textAlign: "center" }}
        type="number"
        min={1}
        value={draft}
        disabled={!isEnabled}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
      <select
        className="quick-select"
        style={{ width: 70 }}
        value={draftUnit}
        disabled={!isEnabled}
        onChange={(e) => {
          const u = e.target.value as Unit;
          setDraftUnit(u);
          const n = Math.max(1, parseInt(draft, 10) || 1);
          onChange(n, u);
        }}
        aria-label="Unit"
      >
        {UNITS.map((u) => (
          <option key={u.key} value={u.key}>{u.label}</option>
        ))}
      </select>
    </div>
  );
}
