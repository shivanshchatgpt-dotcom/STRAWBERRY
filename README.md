# 🍓 Strawberry

> **Your laptop forgets everything. Strawberry remembers — 100% locally.**
> Band karo, kal wapas aao, poochho *"main kya kar raha tha?"* — Strawberry batayega.

A local-first **desktop memory + planner** for students and developers who juggle
50 tabs, 6 apps and 4 projects at once. Every chat, screenshot, clipboard snippet,
window position and focus session becomes searchable, resumable memory — with
**zero cloud, zero API keys, zero telemetry**.

**Platform:** Linux · Windows · macOS (Tauri v2)
**Stack:** Rust + React + TypeScript + SQLite (FTS5) — no LLM required for core features

---

## The pain it kills

Every time you sit down to work you burn 10–15 minutes re-assembling context:
which repo, which tabs, which terminal folder, what was I even doing?
And every evening you close the lid on all of it.

Strawberry answers the three questions nobody else does:
**kaha tha (where) · kya kar raha tha (what) · kyu (why)** — then puts it ALL back.

## ✨ Features

| | |
|---|---|
| 🧊 **Freeze & Resume** | One click freezes the whole workspace — windows *with geometry*, browser tabs, terminal directories, running dev servers. One click tomorrow restores everything. Auto-named sessions (*"🧊 ComfyUI · 16:53"*). |
| 🧠 **Context Recall** | One-click workspace snapshot + a Hinglish story of where/what/why, linked to your own past notes. *"13:31 baje tum VS Code me the — kul 11 apps khule the…"* |
| 📺 **Screen Memory** | Periodic screen frames indexed by perceptual hash. "That UI I saw last Tuesday" → found in seconds. Blocklist-aware, auto-delete ready. |
| 🌳 **Knowledge Tree** | Saved AI chats / notes / imports become a tree; every item gets a deterministic Rust-generated brief — code blocks, commands, errors, URLs, decisions extracted without any LLM. FTS5 search everywhere. |
| 📥 **Universal Inbox** | A background daemon captures clipboard notes, code snippets, URLs and errors as first-class items. Nothing lost to Ctrl+C chaos. |
| 🗓️ **Planner** | Habits (streaks, consistency rings, day-backfill, month heatmaps), Focus timer + stopwatch with session stats, Schedule with Day / **Next 48 Hours** / Week / Calendar views. |
| 🎨 **Liquid Glass UI** | Frosted-glass everything over an ambient strawberry field. Dark & light themes, keyboard-first (Ctrl+1..5). |

## 🎬 60-second demo flow

1. Import a few AI chats → tree + briefs appear instantly
2. Work normally → hit **Snapshot Lo** (Context Recall) or **🧊 Freeze Now**
3. Close everything. Even kill it.
4. Come back tomorrow → **Load Previous Work** / **Resume**
   → *"16:51 baje tum chat-memory-tree me the — kul 11 apps open the…"*
   → windows, tabs and terminals walk back into place
5. "Wo purple button wala screen dhoondo last week ka" → Screen Memory finds it

## 🔒 Privacy model

| Cloud service | Data sent | |
|---|---|---|
| Microsoft Recall / Rewind.ai | Screenshots + content to their cloud index | ❌ |
| Notion AI / Mem | Your notes to LLM providers | ❌ |
| **Strawberry** | **Nothing. Ever. No network calls in core features.** | ✅ |

One SQLite database + plain files under your OS app-data dir. Delete the folder,
it's gone — no account, no sync, no strings.

## 🛠️ Build

```bash
npm install
npm run tauri dev      # dev
npm run tauri build    # AppImage / deb / msi / dmg
```

Requirements: Node 18+, Rust stable, Linux (KDE/Wayland extras: `qdbus6`,
`rsvg-convert` for icon regeneration), WebKitGTK dev libs.

```
src-tauri/src/
├── brief/        # deterministic chat briefing engine (Rust)
├── snapshot.rs   # 🧠 Context Recall collectors (windows/tabs/clipboard/story)
├── workspace/    # 🧊 Freeze & Resume engine
├── screen/       # 📺 pHash screen memory + capture pipeline
├── resume/       # ⏯️ resume points + day summaries
├── planner       # habits · focus · schedule (SQLite)
└── commands/     # Tauri IPC surface (~45 commands)

capture-daemon/   # standalone clipboard daemon writing straight into app.db
strawberry-core/  # shared compression crate
```

## Status

Built for the Razorpay buildathon. v0.9 — release builds, demo video and
optional local-LLM ("ask your memory" via Ollama) on the roadmap.

---

*Made with 🍓 by [@shivanshchatgpt-dotcom](https://github.com/shivanshchatgpt-dotcom) — fully offline, fully yours.*
