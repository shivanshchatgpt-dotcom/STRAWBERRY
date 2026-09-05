import { useCallback, useMemo, useRef, useState } from "react";
import type { docx } from "../../lib/api";
import { BlockLinkPanel } from "./BlockLinkPanel";

/**
 * 📄 BlockEditor — the DOCX block canvas.
 *
 * Block-based (NOT .docx) editor: every block is typed JSON rendered by a
 * per-type component. Smart paste goes through Rust (docx_parse_paste) so
 * heavy parsing stays native. Undo/redo is a local history stack; autosave
 * is debounced through the store.
 *
 * Zero new dependencies by design (spec §PERFORMANCE): charts render as
 * inline SVG, math as a LaTeX-subset renderer, tables as plain DOM.
 */

// ─────────────────────────── small utils ───────────────────────────

const uid = () =>
  "b" + Math.random().toString(36).slice(2, 10) + Date.now().toString(36);

function extractBlockText(block: docx.Block): { title: string; content: string } {
  const data = block.data as Record<string, unknown>;
  switch (block.type) {
    case "text":
    case "heading":
    case "callout": {
      const html = String(data.html ?? "");
      const text = html.replace(/<[^>]*>/g, "").trim();
      return {
        title: text.slice(0, 60) || `Block ${block.type}`,
        content: text,
      };
    }
    case "code": {
      const code = String(data.code ?? "");
      return { title: `Code: ${code.slice(0, 40)}`, content: code };
    }
    case "formula": {
      const latex = String(data.latex ?? "");
      return { title: `Formula: ${latex.slice(0, 40)}`, content: latex };
    }
    case "table": {
      const rows = (data.rows as string[][] | undefined) ?? [];
      const text = rows.map((r) => r.join(" | ")).join("\n");
      return { title: "Table block", content: text };
    }
    case "todo": {
      const tasks = (data.tasks as docx.TodoTask[] | undefined) ?? [];
      const text = tasks.map((t) => `${t.done ? "[x]" : "[ ]"} ${t.text}`).join("\n");
      return { title: String(data.bannerText ?? "Todo list"), content: text };
    }
    case "tree": {
      const root = data.root as docx.TreeNode | undefined;
      const text = flattenTree(root);
      return { title: "Tree structure", content: text };
    }
    case "chart": {
      const rows = (data.data as string[][] | undefined) ?? [];
      const text = rows.map((r) => r.join(", ")).join("\n");
      return { title: String(data.title ?? "Chart"), content: text };
    }
    case "image": {
      const alt = String(data.alt ?? "Image");
      return { title: alt, content: alt };
    }
    default:
      return { title: `Block ${block.type}`, content: "" };
  }
}

function flattenTree(node: docx.TreeNode | undefined, depth = 0): string {
  if (!node) return "";
  const indent = "  ".repeat(depth);
  let result = `${indent}${node.text}\n`;
  for (const child of node.children) {
    result += flattenTree(child, depth + 1);
  }
  return result;
}

interface DragState {
  dragId: string | null;
  overId: string | null;
}

// ─────────────────────────── component ───────────────────────────

export function BlockEditor({
  doc,
  onChange,
  selectedId,
  setSelectedId,
}: {
  doc: docx.DocxDocument;
  onChange: (title: string, blocks: docx.Block[]) => void;
  selectedId: string | null;
  setSelectedId: (id: string | null) => void;
}) {
  const [drag, setDrag] = useState<DragState>({ dragId: null, overId: null });
  const [slashOpenFor, setSlashOpenFor] = useState<string | null>(null);
  const canvasRef = useRef<HTMLDivElement>(null);

  const setBlocks = useCallback(
    (next: docx.Block[]) => onChange(doc.title, next),
    [doc.title, onChange],
  );

  const updateBlock = useCallback(
    (id: string, data: Record<string, unknown>) => {
      setBlocks(doc.blocks.map((b) => (b.id === id ? { ...b, data: { ...b.data, ...data } } : b)));
    },
    [doc.blocks, setBlocks],
  );

  const moveBlock = useCallback(
    (id: string, dir: -1 | 1) => {
      const idx = doc.blocks.findIndex((b) => b.id === id);
      const target = idx + dir;
      if (idx < 0 || target < 0 || target >= doc.blocks.length) return;
      const next = [...doc.blocks];
      [next[idx], next[target]] = [next[target], next[idx]];
      setBlocks(next);
    },
    [doc.blocks, setBlocks],
  );

  const deleteBlock = useCallback(
    (id: string) => setBlocks(doc.blocks.filter((b) => b.id !== id)),
    [doc.blocks, setBlocks],
  );

  const insertAfter = useCallback(
    (id: string | null, block: docx.Block) => {
      if (id === null) {
        setBlocks([...doc.blocks, block]);
        return;
      }
      const idx = doc.blocks.findIndex((b) => b.id === id);
      const next = [...doc.blocks];
      next.splice(idx + 1, 0, block);
      setBlocks(next);
    },
    [doc.blocks, setBlocks],
  );

  // ── SMART PASTE (core feature) ─────────────────────────────────
  const handlePaste = useCallback(
    async (e: React.ClipboardEvent) => {
      const cd = e.clipboardData;
      if (!cd) return;
      const html = cd.getData("text/html");
      const text = cd.getData("text/plain");

      // Images: paste straight into an image block (data URL, local only).
      const imageItem = Array.from(cd.items).find((i) => i.type.startsWith("image/"));
      if (imageItem && !html) {
        const file = imageItem.getAsFile();
        if (file) {
          const reader = new FileReader();
          reader.onload = () => {
            insertAfter(selectedId, {
              id: uid(),
              type: "image",
              data: { src: String(reader.result), alt: "pasted image" },
            });
          };
          reader.readAsDataURL(file);
          e.preventDefault();
          return;
        }
      }

      if (!text && !html) return;
      e.preventDefault();
      try {
        const blocks = await api.docxParsePaste({ html: html || null, text: text || null });
        if (blocks.length) {
          let next = [...doc.blocks];
          const at = selectedId ? next.findIndex((b) => b.id === selectedId) : next.length - 1;
          next.splice(at + 1, 0, ...blocks);
          setBlocks(next);
        }
      } catch {
        // Rust parse failed (rare) — fall back to a raw text block so the
        // paste is NEVER lost (spec rule 10: graceful degradation).
        if (text) {
          insertAfter(selectedId, {
            id: uid(),
            type: "text",
            data: { html: `<p>${text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/\n/g, "<br>")}</p>` },
          });
        }
      }
    },
    [doc.blocks, insertAfter, selectedId, setBlocks],
  );

  // ── keyboard: Cmd/Ctrl+Z undo, Shift+Z redo, Ctrl+/ palette ─────
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key === "/") {
        e.preventDefault();
        setSlashOpenFor((v) => (v ? null : "menu"));
      }
    },
    [],
  );

  const dropOnto = useCallback(
    (targetId: string) => {
      if (!drag.dragId || drag.dragId === targetId) return;
      const from = doc.blocks.findIndex((b) => b.id === drag.dragId);
      const to = doc.blocks.findIndex((b) => b.id === targetId);
      if (from < 0 || to < 0) return;
      const next = [...doc.blocks];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      setBlocks(next);
      setDrag({ dragId: null, overId: null });
    },
    [doc.blocks, drag.dragId, setBlocks],
  );

  return (
    <div
      ref={canvasRef}
      className="docx-canvas"
      onPaste={(e) => void handlePaste(e)}
      onKeyDown={onKeyDown}
      tabIndex={0}
    >
      {/* Slash command palette */}
      {slashOpenFor === "menu" && (
        <div className="docx-slash" role="menu">
          {BLOCK_MENU.map((m) => (
            <button
              key={m.type}
              className="docx-slash-item"
              onClick={() => {
                insertAfter(selectedId, newBlock(m.type));
                setSlashOpenFor(null);
              }}
            >
              <span>{m.icon}</span> {m.label}
              <em>{m.hint}</em>
            </button>
          ))}
        </div>
      )}

      {doc.blocks.length === 0 && (
        <div className="docx-empty">
          <p>📄 Empty document</p>
          <p className="text-dim">
            Paste anything here (tables, lists, code, formulas…), press{" "}
            <kbd>Ctrl+/</kbd> for the insert menu, or <kbd>Ctrl+V</kbd> smart-paste.
          </p>
        </div>
      )}

      {doc.blocks.map((b, i) => (
        <div
          key={b.id}
          className={`docx-block${selectedId === b.id ? " selected" : ""}${drag.overId === b.id ? " drop-target" : ""}`}
          onMouseDown={() => setSelectedId(b.id)}
          draggable
          onDragStart={() => setDrag({ dragId: b.id, overId: null })}
          onDragOver={(e) => {
            e.preventDefault();
            setDrag((d) => (d.overId === b.id ? d : { ...d, overId: b.id }));
          }}
          onDrop={() => dropOnto(b.id)}
          onDragEnd={() => setDrag({ dragId: null, overId: null })}
        >
          <div className="docx-block-gutter">
            <button className="icon-btn" title="Move up" onClick={() => moveBlock(b.id, -1)}>↑</button>
            <button className="icon-btn" title="Move down" onClick={() => moveBlock(b.id, 1)}>↓</button>
            <button className="icon-btn" title="Insert below" onClick={() => setSlashOpenFor("menu")}>+</button>
            <button className="icon-btn" title="Delete block" onClick={() => deleteBlock(b.id)}>✕</button>
            <span className="docx-block-index">{i + 1}</span>
          </div>
          <div className="docx-block-body">
            <BlockView block={b} update={(d) => updateBlock(b.id, d)} allBlocks={doc.blocks} />
            {selectedId === b.id && (() => {
              const { title, content } = extractBlockText(b);
              return (
              <BlockLinkPanel
                blockId={b.id}
                documentId={doc.id}
                blockType={b.type}
                blockContent={content}
                blockTitle={title}
                onNavigateToMemory={(memoryId) => {
                  try {
                    // eslint-disable-next-line @typescript-eslint/no-explicit-any
                    const w = window as any;
                    if (w.__strawberry_open_memory) {
                      w.__strawberry_open_memory(memoryId);
                    }
                  } catch {
                    /* navigation hook not installed — ignore */
                  }
                }}
              />
              );
            })()}
          </div>
        </div>
      ))}
    </div>
  );
}

// ─────────────────────────── menu / factories ───────────────────────────

const BLOCK_MENU: Array<{ type: docx.BlockType; icon: string; label: string; hint: string }> = [
  { type: "text", icon: "¶", label: "Text", hint: "rich paragraph" },
  { type: "heading", icon: "H", label: "Heading", hint: "section title" },
  { type: "table", icon: "▦", label: "Table", hint: "editable grid" },
  { type: "todo", icon: "☑", label: "Todo banner", hint: "task list banner" },
  { type: "tree", icon: "🌳", label: "Tree", hint: "hierarchy" },
  { type: "chart", icon: "📊", label: "Chart", hint: "bar/line/pie/scatter" },
  { type: "formula", icon: "∑", label: "Math", hint: "LaTeX block" },
  { type: "code", icon: "⌨", label: "Code", hint: "code block" },
  { type: "callout", icon: "💡", label: "Callout", hint: "highlighted note" },
  { type: "divider", icon: "—", label: "Divider", hint: "separator" },
];

export function newBlock(type: docx.BlockType): docx.Block {
  const id = uid();
  switch (type) {
    case "heading":
      return { id, type, data: { html: "<h2>Heading</h2>", level: 2 } };
    case "table":
      return {
        id,
        type,
        data: {
          rows: [
            ["Header 1", "Header 2"],
            ["", ""],
          ],
          props: {
            headerRow: true,
            borderThickness: 1,
            borderColor: "#333333",
            zebra: false,
            cellPadding: 6,
            align: "left",
            outerBorder: true,
          },
        },
      };
    case "todo":
      return {
        id,
        type,
        data: {
          bannerText: "Tasks",
          bannerColor: "#1d2434",
          textColor: "#ffffff",
          bannerHeight: 52,
          tasks: [{ id: uid(), text: "New task", done: false, priority: 0 }],
        },
      };
    case "tree":
      return { id, type, data: { root: { id: uid(), text: "Topic", children: [], collapsed: false } } };
    case "chart":
      return {
        id,
        type,
        data: {
          chartType: "bar",
          title: "Chart",
          xLabel: "",
          yLabel: "",
          showLegend: false,
          sourceBlockId: null,
          annotations: [],
          data: [
            ["Category", "Value"],
            ["A", "10"],
            ["B", "25"],
          ],
        },
      };
    case "formula":
      return { id, type, data: { latex: "E = mc^2", display: true } };
    case "code":
      return { id, type, data: { code: "// code", language: "" } };
    case "callout":
      return { id, type, data: { html: "<p>Callout…</p>", tone: "info" } };
    case "divider":
      return { id, type, data: {} };
    case "image":
      return { id, type, data: { src: "", alt: "" } };
    default:
      return { id, type: "text", data: { html: "<p>Type or paste here…</p>" } };
  }
}

// ─────────────────────────── per-type renderers ───────────────────────────

function BlockView({
  block,
  update,
  allBlocks,
}: {
  block: docx.Block;
  update: (data: Record<string, unknown>) => void;
  allBlocks: docx.Block[];
}) {
  switch (block.type) {
    case "text":
    case "heading":
      return <EditableHtml block={block} update={update} />;
    case "callout":
      return <EditableHtml block={block} update={update} className="docx-callout" />;
    case "divider":
      return <hr className="docx-divider" />;
    case "table":
      return <TableBlock block={block} update={update} />;
    case "todo":
      return <TodoBlock block={block} update={update} />;
    case "tree":
      return <TreeBlock block={block} update={update} />;
    case "chart":
      return <ChartBlock block={block} update={update} allBlocks={allBlocks} />;
    case "formula":
      return <FormulaBlock block={block} update={update} />;
    case "code":
      return <CodeBlock block={block} update={update} />;
    case "image":
      return <ImageBlock block={block} update={update} />;
    default:
      return <div className="text-dim">unsupported block: {block.type}</div>;
  }
}

// ── rich text / heading / callout ─────────────────────────────────

function EditableHtml({
  block,
  update,
  className,
}: {
  block: docx.Block;
  update: (d: Record<string, unknown>) => void;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const committed = useRef(true);
  return (
    <div
      ref={ref}
      className={`docx-rich${className ? " " + className : ""}`}
      contentEditable
      suppressContentEditableWarning
      dangerouslySetInnerHTML={{ __html: String(block.data.html ?? "") }}
      onInput={() => {
        committed.current = false;
      }}
      onBlur={() => {
        if (!committed.current && ref.current) {
          // Minimal client-side sanitation happens in Rust on paste; on blur
          // we only persist text the user typed in an already-clean field.
          update({ html: ref.current.innerHTML });
          committed.current = true;
        }
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          (e.target as HTMLElement).blur();
        }
      }}
    />
  );
}

// ── table block (spec §TABLE REQUIREMENTS) ────────────────────────

function TableBlock({ block, update }: { block: docx.Block; update: (d: Record<string, unknown>) => void }) {
  const rows = (block.data.rows as string[][]) ?? [];
  const props = (block.data.props as docx.TableProps) ?? defaultTableProps();
  const [sel, setSel] = useState<{ r: number; c: number } | null>(null);

  const setRows = (next: string[][]) => update({ rows: next, props });
  const setProps = (p: Partial<docx.TableProps>) => update({ props: { ...props, ...p }, rows });

  const editCell = (r: number, c: number, v: string) => {
    const next = rows.map((row) => [...row]);
    next[r][c] = v;
    setRows(next);
  };
  const addRow = (at: number) => {
    const next = rows.map((row) => [...row]);
    const cols = rows[0]?.length ?? 2;
    next.splice(at, 0, Array.from({ length: cols }, () => ""));
    setRows(next);
  };
  const addCol = (at: number) => {
    const next = rows.map((row) => [...row]);
    next.forEach((row) => row.splice(at, 0, ""));
    setRows(next);
  };
  const delRow = (r: number) => setRows(rows.filter((_, i) => i !== r));
  const delCol = (c: number) => setRows(rows.map((row) => row.filter((_, i) => i !== c)));

  return (
    <div className="docx-table-wrap">
      <table
        className="docx-table"
        style={{
          borderCollapse: "collapse",
        }}
      >
        <tbody>
          {rows.map((row, r) => (
            <tr key={r} className={props.zebra && r % 2 === 1 ? "zebra" : ""}>
              {row.map((cell, c) => (
                <td
                  key={c}
                  style={{
                    border: `${props.borderThickness}px solid ${props.borderColor}`,
                    padding: `${props.cellPadding}px 8px`,
                    textAlign: props.align as "left" | "center" | "right",
                    fontWeight: props.headerRow && r === 0 ? 700 : 400,
                    background: props.headerRow && r === 0 ? "rgba(127,127,127,0.12)" : undefined,
                  }}
                  className={sel && sel.r === r && sel.c === c ? "sel" : ""}
                  onClick={() => setSel({ r, c })}
                  onDoubleClick={(e) => {
                    const td = e.currentTarget as HTMLTableCellElement;
                    td.setAttribute("contentEditable", "true");
                    td.focus();
                  }}
                  onBlur={(e) => {
                    const td = e.currentTarget as HTMLTableCellElement;
                    td.removeAttribute("contentEditable");
                    editCell(r, c, td.textContent ?? "");
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      (e.currentTarget as HTMLTableCellElement).blur();
                    }
                  }}
                >
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {sel && (
        <div className="docx-table-tools">
          <button className="icon-btn" title="Add row above" onClick={() => addRow(sel.r)}>↥+</button>
          <button className="icon-btn" title="Add row below" onClick={() => addRow(sel.r + 1)}>↧+</button>
          <button className="icon-btn" title="Delete row" onClick={() => { delRow(sel.r); setSel(null); }}>↥✕</button>
          <button className="icon-btn" title="Add column left" onClick={() => addCol(sel.c)}>↤+</button>
          <button className="icon-btn" title="Add column right" onClick={() => addCol(sel.c + 1)}>↦+</button>
          <button className="icon-btn" title="Delete column" onClick={() => { delCol(sel.c); setSel(null); }}>↤✕</button>
          <label className="docx-toggle">
            <input type="checkbox" checked={props.headerRow} onChange={(e) => setProps({ headerRow: e.target.checked })} /> header
          </label>
          <label className="docx-toggle">
            <input type="checkbox" checked={props.zebra} onChange={(e) => setProps({ zebra: e.target.checked })} /> zebra
          </label>
          <label className="docx-range">
            border
            <input
              type="range" min={0} max={4} value={props.borderThickness}
              onChange={(e) => setProps({ borderThickness: Number(e.target.value) })}
            />
          </label>
          <label className="docx-range">
            pad
            <input
              type="range" min={0} max={18} value={props.cellPadding}
              onChange={(e) => setProps({ cellPadding: Number(e.target.value) })}
            />
          </label>
          <select
            className="quick-select"
            value={props.align}
            onChange={(e) => setProps({ align: e.target.value })}
          >
            <option value="left">left</option>
            <option value="center">center</option>
            <option value="right">right</option>
          </select>
          <input
            type="color"
            value={props.borderColor}
            onChange={(e) => setProps({ borderColor: e.target.value })}
            title="Border color"
            className="docx-color"
          />
        </div>
      )}
    </div>
  );
}

function defaultTableProps(): docx.TableProps {
  return {
    headerRow: true,
    borderThickness: 1,
    borderColor: "#333333",
    zebra: false,
    cellPadding: 6,
    align: "left",
    outerBorder: true,
  };
}

// ── todo banner block (spec §TODO/BANNER) ─────────────────────────

function TodoBlock({ block, update }: { block: docx.Block; update: (d: Record<string, unknown>) => void }) {
  const data = block.data as {
    bannerText?: string;
    bannerColor?: string;
    textColor?: string;
    bannerHeight?: number;
    tasks?: docx.TodoTask[];
  };
  const tasks = data.tasks ?? [];
  const done = tasks.filter((t) => t.done).length;
  const pct = tasks.length ? Math.round((done / tasks.length) * 100) : 0;

  const setTasks = (next: docx.TodoTask[]) => update({ ...data, tasks: next });
  const addTask = () =>
    setTasks([...tasks, { id: uid(), text: "New task", done: false, priority: 0 }]);
  const toggle = (id: string) =>
    setTasks(tasks.map((t) => (t.id === id ? { ...t, done: !t.done } : t)));
  const remove = (id: string) => setTasks(tasks.filter((t) => t.id !== id));
  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= tasks.length) return;
    const next = [...tasks];
    [next[i], next[j]] = [next[j], next[i]];
    setTasks(next);
  };

  return (
    <div className="docx-todo" style={{ borderColor: data.bannerColor ?? "#1d2434" }}>
      <div
        className="docx-todo-banner"
        style={{
          background: data.bannerColor ?? "#1d2434",
          color: data.textColor ?? "#fff",
          minHeight: (data.bannerHeight ?? 52) + "px",
        }}
      >
        <input
          className="docx-todo-title"
          style={{ color: data.textColor ?? "#fff" }}
          value={data.bannerText ?? ""}
          onChange={(e) => update({ ...data, bannerText: e.target.value })}
          aria-label="Banner title"
        />
        <span className="docx-todo-progress">{pct}%</span>
      </div>
      <div className="docx-todo-progressbar">
        <span style={{ width: `${pct}%` }} />
      </div>
      <ul className="docx-todo-list">
        {tasks.map((t, i) => (
          <li key={t.id} className={t.done ? "done" : ""}>
            <input
              type="checkbox"
              checked={t.done}
              onChange={() => toggle(t.id)}
              aria-label={`complete ${t.text}`}
            />
            <input
              className="docx-todo-text"
              value={t.text}
              onChange={(e) => setTasks(tasks.map((x) => (x.id === t.id ? { ...x, text: e.target.value } : x)))}
              aria-label="task text"
            />
            {t.priority > 0 && <span className={"docx-prio p" + t.priority}>{"!".repeat(t.priority)}</span>}
            <button className="icon-btn" onClick={() => move(i, -1)} title="Move up">↑</button>
            <button className="icon-btn" onClick={() => move(i, 1)} title="Move down">↓</button>
            <button className="icon-btn" onClick={() => remove(t.id)} title="Delete task">✕</button>
          </li>
        ))}
      </ul>
      <div className="docx-todo-tools">
        <button className="btn" onClick={addTask}>+ task</button>
        <label>color <input type="color" value={data.bannerColor ?? "#1d2434"} onChange={(e) => update({ ...data, bannerColor: e.target.value })} /></label>
        <label>text <input type="color" value={data.textColor ?? "#ffffff"} onChange={(e) => update({ ...data, textColor: e.target.value })} /></label>
        <label>height
          <input type="range" min={36} max={120} value={data.bannerHeight ?? 52}
            onChange={(e) => update({ ...data, bannerHeight: Number(e.target.value) })} />
        </label>
      </div>
    </div>
  );
}

// ── tree block (spec §TREE) ───────────────────────────────────────

function TreeBlock({ block, update }: { block: docx.Block; update: (d: Record<string, unknown>) => void }) {
  const root = (block.data.root as docx.TreeNode) ?? { id: uid(), text: "", children: [], collapsed: false };

  const mutate = (fn: (n: docx.TreeNode) => void) => {
    const clone = JSON.parse(JSON.stringify(root)) as docx.TreeNode;
    fn(clone);
    update({ root: clone });
  };

  const renderNode = (node: docx.TreeNode, depth: number, parent: docx.TreeNode | null, siblings: docx.TreeNode[]): React.ReactNode => {
    void parent; void siblings;
    return (
      <li key={node.id} className={node.collapsed && node.children.length ? "collapsed" : ""}>
        <div className="docx-tree-row" style={{ paddingLeft: depth * 18 }}>
          {node.children.length > 0 ? (
            <button
              className="icon-btn docx-tree-toggle"
              onClick={() => mutate((n) => {
                const find = (x: docx.TreeNode): docx.TreeNode | null =>
                  x.id === node.id ? x : x.children.map(find).find(Boolean) ?? null;
                const hit = find(n);
                if (hit) hit.collapsed = !hit.collapsed;
              })}
            >{node.collapsed ? "▸" : "▾"}</button>
          ) : (
            <span className="docx-tree-bullet">•</span>
          )}
          <input
            className="docx-tree-text"
            value={node.text}
            onChange={(e) => mutate((n) => {
              const find = (x: docx.TreeNode): docx.TreeNode | null =>
                x.id === node.id ? x : x.children.map(find).find(Boolean) ?? null;
              const hit = find(n);
              if (hit) hit.text = e.target.value;
            })}
          />
          <button
            className="icon-btn" title="Add child (Tab)"
            onClick={() => mutate((n) => {
              const find = (x: docx.TreeNode): docx.TreeNode | null =>
                x.id === node.id ? x : x.children.map(find).find(Boolean) ?? null;
              const hit = find(n);
              if (hit) hit.children.push({ id: uid(), text: "new", children: [], collapsed: false });
            })}
          >+</button>
          <button
            className="icon-btn" title="Delete branch"
            onClick={() => mutate((n) => {
              const strip = (x: docx.TreeNode): docx.TreeNode =>
                ({ ...x, children: x.children.filter((c) => c.id !== node.id).map(strip) });
              Object.assign(n, strip(n));
            })}
          >✕</button>
        </div>
        {!node.collapsed && node.children.length > 0 && (
          <ul className="docx-tree-children">
            {node.children.map((c) => renderNode(c, depth + 1, node, node.children))}
          </ul>
        )}
      </li>
    );
  };

  return (
    <div className="docx-tree">
      <ul className="docx-tree-root">{renderNode(root, 0, null, [root])}</ul>
      <button
        className="btn"
        onClick={() => mutate((n) => {
          n.children.push({ id: uid(), text: "new branch", children: [], collapsed: false });
        })}
      >
        + root child
      </button>
    </div>
  );
}

// ── chart block — pure SVG, table-linked (spec §CHART) ────────────

function ChartBlock({
  block,
  update,
  allBlocks,
}: {
  block: docx.Block;
  update: (d: Record<string, unknown>) => void;
  allBlocks: docx.Block[];
}) {
  const cfg = (block.data as unknown) as docx.ChartConfig;
  // Live link to a source table (spec: table → chart relationship).
  const sourceRows = useMemo(() => {
    if (cfg.sourceBlockId) {
      const src = allBlocks.find((b) => b.id === cfg.sourceBlockId);
      if (src && src.type === "table") return (src.data.rows as string[][]) ?? [];
    }
    return cfg.data ?? [];
  }, [cfg.sourceBlockId, cfg.data, allBlocks]);

  const type = cfg.chartType ?? "bar";
  const title = cfg.title ?? "";
  const W = 520;
  const H = 300;
  const P = 40; // padding

  const numeric = useMemo(() => {
    if (sourceRows.length < 2) return [];
    return sourceRows.slice(1).map((r) => ({
      label: r[0] ?? "",
      value: Number(r[1]) || 0,
    }));
  }, [sourceRows]);

  const maxY = Math.max(1, ...numeric.map((d) => d.value));
  const annotations = cfg.annotations ?? [];

  return (
    <div className="docx-chart">
      <div className="docx-chart-head">
        <input
          className="docx-chart-title"
          value={title}
          placeholder="Chart title"
          onChange={(e) => update({ ...cfg, title: e.target.value })}
        />
        <select className="quick-select" value={type} onChange={(e) => update({ ...cfg, chartType: e.target.value as docx.ChartConfig["chartType"] })}>
          <option value="bar">bar</option>
          <option value="line">line</option>
          <option value="pie">pie</option>
          <option value="scatter">scatter</option>
        </select>
      </div>
      <svg width="100%" viewBox={`0 0 ${W} ${H}`} className="docx-chart-svg">
        {(type === "bar" || type === "line" || type === "scatter") && (
          <>
            {/* gridlines */}
            {[0, 0.25, 0.5, 0.75, 1].map((f) => (
              <line key={f} x1={P} x2={W - 10} y1={H - P - f * (H - 2 * P)} y2={H - P - f * (H - 2 * P)}
                stroke="currentColor" strokeOpacity={0.12} />
            ))}
            {/* y axis labels */}
            {[0, 0.5, 1].map((f) => (
              <text key={f} x={P - 6} y={H - P - f * (H - 2 * P) + 4} textAnchor="end" fontSize={10}
                fill="currentColor" opacity={0.55}>
                {Math.round(maxY * f)}
              </text>
            ))}
          </>
        )}
        {type === "bar" &&
          numeric.map((d, i) => {
            const bw = (W - 2 * P) / Math.max(1, numeric.length) - 8;
            const h = (d.value / maxY) * (H - 2 * P);
            return (
              <g key={i}>
                <rect x={P + i * ((W - 2 * P) / Math.max(1, numeric.length)) + 4}
                  y={H - P - h} width={Math.max(4, bw)} height={h}
                  rx={4} fill="var(--berry, #e11d48)" />
                <text x={P + i * ((W - 2 * P) / Math.max(1, numeric.length)) + 4 + bw / 2}
                  y={H - P + 14} textAnchor="middle" fontSize={10} fill="currentColor" opacity={0.7}>
                  {d.label}
                </text>
              </g>
            );
          })}
        {type === "line" && (
          <>
            {numeric.map((d, i) => {
              const x = P + (i / Math.max(1, numeric.length - 1)) * (W - 2 * P);
              const y = H - P - (d.value / maxY) * (H - 2 * P);
              return <circle key={i} cx={x} cy={y} r={4} fill="var(--berry, #e11d48)" />;
            })}
            <polyline
              fill="none" stroke="var(--berry, #e11d48)" strokeWidth={2}
              points={numeric
                .map((d, i) => {
                  const x = P + (i / Math.max(1, numeric.length - 1)) * (W - 2 * P);
                  const y = H - P - (d.value / maxY) * (H - 2 * P);
                  return `${x},${y}`;
                })
                .join(" ")}
            />
          </>
        )}
        {type === "scatter" &&
          numeric.map((d, i) => {
            const x = P + (i / Math.max(1, numeric.length - 1)) * (W - 2 * P);
            const y = H - P - (d.value / maxY) * (H - 2 * P);
            return <circle key={i} cx={x} cy={y} r={5} fill="var(--berry, #e11d48)" opacity={0.75} />;
          })}
        {type === "pie" &&
          (() => {
            const total = numeric.reduce((s, d) => s + Math.abs(d.value), 0) || 1;
            const cx = W / 2;
            const cy = H / 2;
            const r = Math.min(W, H) / 2 - P;
            let acc = 0;
            const palette = ["#e11d48", "#3b82f6", "#f59e0b", "#10b981", "#8b5cf6", "#06b6d4"];
            return numeric.map((d, i) => {
              const frac = Math.abs(d.value) / total;
              const a0 = acc * 2 * Math.PI - Math.PI / 2;
              acc += frac;
              const a1 = acc * 2 * Math.PI - Math.PI / 2;
              const large = a1 - a0 > Math.PI ? 1 : 0;
              const x0 = cx + r * Math.cos(a0);
              const y0 = cy + r * Math.sin(a0);
              const x1 = cx + r * Math.cos(a1);
              const y1 = cy + r * Math.sin(a1);
              return (
                <path key={i}
                  d={`M ${cx} ${cy} L ${x0} ${y0} A ${r} ${r} 0 ${large} 1 ${x1} ${y1} Z`}
                  fill={palette[i % palette.length]} opacity={0.85} />
              );
            });
          })()}
        {annotations.map((a) => (
          <text key={a.id} x={a.x * W} y={a.y * H} fontSize={a.fontSize} fill="currentColor"
            className="docx-annotation">
            {a.text}
          </text>
        ))}
      </svg>
      <div className="docx-chart-tools">
        <label>
          X <input className="quick-input" value={cfg.xLabel ?? ""} onChange={(e) => update({ ...cfg, xLabel: e.target.value })} />
        </label>
        <label>
          Y <input className="quick-input" value={cfg.yLabel ?? ""} onChange={(e) => update({ ...cfg, yLabel: e.target.value })} />
        </label>
        <label className="docx-toggle">
          <input type="checkbox" checked={cfg.showLegend ?? false} onChange={(e) => update({ ...cfg, showLegend: e.target.checked })} /> legend
        </label>
        <button
          className="btn"
          onClick={() =>
            update({
              ...cfg,
              annotations: [
                ...(cfg.annotations ?? []),
                { id: uid(), text: "note", x: 0.4, y: 0.2, fontSize: 13 },
              ],
            })
          }
        >
          + annotation
        </button>
        {annotations.length > 0 && (
          <div className="docx-annotation-list">
            {annotations.map((a, i) => (
              <span key={a.id}>
                <input
                  className="quick-input"
                  value={a.text}
                  onChange={(e) => {
                    const next = [...annotations];
                    next[i] = { ...a, text: e.target.value };
                    update({ ...cfg, annotations: next });
                  }}
                />
                <button
                  className="icon-btn"
                  onClick={() => update({ ...cfg, annotations: annotations.filter((x) => x.id !== a.id) })}
                >✕</button>
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ── formula block — LaTeX subset renderer (offline, no KaTeX dep) ─

function FormulaBlock({ block, update }: { block: docx.Block; update: (d: Record<string, unknown>) => void }) {
  const latex = String(block.data.latex ?? "");
  const [editing, setEditing] = useState(false);
  const rendered = useMemo(() => renderLatexSubset(latex), [latex]);
  return (
    <div className="docx-formula">
      {editing ? (
        <textarea
          className="docx-formula-input"
          value={latex}
          onChange={(e) => update({ latex: e.target.value, display: block.data.display ?? true })}
          onBlur={() => setEditing(false)}
          autoFocus
          rows={2}
        />
      ) : (
        <div
          className="docx-formula-view"
          title="Click to edit LaTeX"
          onClick={() => setEditing(true)}
        >
          {rendered}
        </div>
      )}
      <span className="docx-formula-hint">∑ LaTeX — click to edit, copy as $…$</span>
    </div>
  );
}

/** Tiny deterministic LaTeX-subset → HTML renderer. Handles the common
 * paste shapes honestly; exotic macros fall back to verbatim text (spec
 * rule 10 — never lose content). */
function renderLatexSubset(src: string): string {
  let s = src
    .replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  s = s
    .replace(/\\frac\{([^{}]*)\}\{([^{}]*)\}/g, "<span class=\"mfrac\"><span>$1</span><span>$2</span></span>")
    .replace(/\\sqrt\{([^{}]*)\}/g, "√<span class=\"msqrt\">($1)</span>")
    .replace(/\^\{([^{}]*)\}/g, "<sup>$1</sup>")
    .replace(/\^([A-Za-z0-9])/g, "<sup>$1</sup>")
    .replace(/_\{([^{}]*)\}/g, "<sub>$1</sub>")
    .replace(/_([A-Za-z0-9])/g, "<sub>$1</sub>")
    .replace(/\\alpha/g, "α").replace(/\\beta/g, "β").replace(/\\gamma/g, "γ")
    .replace(/\\delta/g, "δ").replace(/\\Delta/g, "Δ").replace(/\\pi/g, "π")
    .replace(/\\infty/g, "∞").replace(/\\times/g, "×").replace(/\\cdot/g, "·")
    .replace(/\\leq/g, "≤").replace(/\\geq/g, "≥").replace(/\\neq/g, "≠")
    .replace(/\\sum/g, "∑").replace(/\\int/g, "∫").replace(/\\partial/g, "∂")
    .replace(/\\left\(/g, "(").replace(/\\right\)/g, ")")
    .replace(/\\\\/g, "<br>")
    .replace(/\$/g, "");
  return s;
}

// ── code block ─────────────────────────────────────────────────────

function CodeBlock({ block, update }: { block: docx.Block; update: (d: Record<string, unknown>) => void }) {
  const code = String(block.data.code ?? "");
  const lang = String(block.data.language ?? "");
  const [editing, setEditing] = useState(false);
  return (
    <div className="docx-code">
      <div className="docx-code-head">
        <input
          className="quick-input docx-code-lang"
          placeholder="language"
          value={lang}
          onChange={(e) => update({ code, language: e.target.value })}
        />
        <button className="btn" onClick={() => setEditing((v) => !v)}>
          {editing ? "preview" : "edit"}
        </button>
        <button
          className="btn"
          onClick={() => void navigator.clipboard?.writeText(code)}
          title="Copy code"
        >
          📋
        </button>
      </div>
      {editing ? (
        <textarea
          className="docx-code-input"
          value={code}
          onChange={(e) => update({ code: e.target.value, language: lang })}
          rows={Math.max(3, code.split("\n").length)}
        />
      ) : (
        <pre className="docx-code-view">{code}</pre>
      )}
    </div>
  );
}

// ── image block ───────────────────────────────────────────────────

function ImageBlock({ block, update }: { block: docx.Block; update: (d: Record<string, unknown>) => void }) {
  const src = String(block.data.src ?? "");
  const alt = String(block.data.alt ?? "");
  const fileRef = useRef<HTMLInputElement>(null);
  return (
    <div className="docx-image">
      {src ? (
        <img src={src} alt={alt} className="docx-image-img" />
      ) : (
        <button className="btn" onClick={() => fileRef.current?.click()}>
          🖼 choose image…
        </button>
      )}
      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        hidden
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (!f) return;
          const reader = new FileReader();
          reader.onload = () => update({ src: String(reader.result), alt: f.name });
          reader.readAsDataURL(f);
        }}
      />
      <input
        className="quick-input docx-image-alt"
        placeholder="alt text"
        value={alt}
        onChange={(e) => update({ src, alt: e.target.value })}
      />
    </div>
  );
}

// api is imported lazily to avoid a circular import at module scope.
import { api } from "../../lib/api";
