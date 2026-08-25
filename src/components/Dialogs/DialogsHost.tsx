import { useMemo, useRef, useState } from "react";
import { useAppStore } from "../../store/appStore";
import { Dialog } from "./Dialog";
import { isValidName, isSupportedImport, readFileAsText, formatBytes, MAX_IMPORT_BYTES } from "../../lib/utils";

export function DialogsHost() {
  const dialog = useAppStore((s) => s.dialog);
  if (dialog.kind === "none") return null;
  switch (dialog.kind) {
    case "create-root":
      return <CreateRootDialog />;
    case "create-folder":
      return <CreateFolderDialog parentId={dialog.parentId} />;
    case "create-chat":
      return <CreateChatDialog parentId={dialog.parentId} />;
    case "import-chat":
      return <ImportChatDialog parentId={dialog.parentId} />;
    case "rename-root":
      return <RenameRootDialog rootId={dialog.rootId} initial={dialog.name} />;
    case "rename-folder":
      return <RenameFolderDialog nodeId={dialog.nodeId} initial={dialog.name} />;
    case "confirm-delete":
      return <ConfirmDeleteDialog target={dialog.target} />;
  }
}

// ---------------------------------------------------------------------------
// Create root
// ---------------------------------------------------------------------------

function CreateRootDialog() {
  const busy = useAppStore((s) => s.busy);
  const createRoot = useAppStore((s) => s.createRoot);
  const [name, setName] = useState("");
  const valid = isValidName(name);

  return (
    <Dialog
      title="New Index"
      hint="A top-level tree, e.g. School, Biology, Vibe Coding — anything you like."
      submitLabel="Create Index"
      busy={busy}
      canSubmit={valid}
      onSubmit={() => void createRoot(name)}
    >
      <div className="field">
        <label htmlFor="dlg-name">Index name</label>
        <input
          id="dlg-name"
          type="text"
          value={name}
          autoFocus
          placeholder="e.g. School"
          onChange={(e) => setName(e.target.value)}
        />
        {!valid && name.trim() !== "" && (
          <div className="field-error">Name must be 1–200 characters.</div>
        )}
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Rename root / folder (shared shape)
// ---------------------------------------------------------------------------

function RenameRootDialog({ rootId, initial }: { rootId: string; initial: string }) {
  const busy = useAppStore((s) => s.busy);
  const renameRoot = useAppStore((s) => s.renameRoot);
  const [name, setName] = useState(initial);
  const valid = isValidName(name) && name.trim() !== initial.trim();

  return (
    <Dialog
      title="Rename Index"
      submitLabel="Rename"
      busy={busy}
      canSubmit={valid}
      onSubmit={() => void renameRoot(rootId, name)}
    >
      <div className="field">
        <label>Name</label>
        <input
          type="text"
          value={name}
          autoFocus
          onChange={(e) => setName(e.target.value)}
        />
      </div>
    </Dialog>
  );
}

function RenameFolderDialog({ nodeId, initial }: { nodeId: string; initial: string }) {
  const busy = useAppStore((s) => s.busy);
  const renameFolder = useAppStore((s) => s.renameFolder);
  // The reader reuses this dialog to rename the open chat.
  const detail = useAppStore((s) => s.chatDetail);
  const renameChat = useAppStore((s) => s.renameChat);
  const [name, setName] = useState(initial);
  const valid = isValidName(name) && name.trim() !== initial.trim();
  const isChatRename =
    detail != null && detail.meta.nodeId === nodeId && detail.meta.title === initial;

  const submit = async () => {
    if (!valid) return;
    if (isChatRename && detail) await renameChat(detail.meta.chatId, name);
    else await renameFolder(nodeId, name);
  };

  return (
    <Dialog
      title={isChatRename ? "Rename Chat" : "Rename Folder"}
      submitLabel="Rename"
      busy={busy}
      canSubmit={valid}
      onSubmit={() => void submit()}
    >
      <div className="field">
        <label>Name</label>
        <input
          type="text"
          value={name}
          autoFocus
          onChange={(e) => setName(e.target.value)}
        />
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Create folder
// ---------------------------------------------------------------------------

function CreateFolderDialog({ parentId }: { parentId: string | null }) {
  const busy = useAppStore((s) => s.busy);
  const createFolder = useAppStore((s) => s.createFolder);
  const breadcrumb = useAppStore((s) => s.breadcrumb);
  const [name, setName] = useState("");
  const valid = isValidName(name);

  const parentLabel =
    parentId == null
      ? breadcrumb[breadcrumb.length - 1]?.label ?? "index root"
      : "current folder";

  return (
    <Dialog
      title="New Folder"
      hint={`Created inside: ${parentLabel}`}
      submitLabel="Create Folder"
      busy={busy}
      canSubmit={valid}
      onSubmit={() => void createFolder(parentId, name)}
    >
      <div className="field">
        <label>Folder name</label>
        <input
          type="text"
          value={name}
          autoFocus
          placeholder="e.g. Physics"
          onChange={(e) => setName(e.target.value)}
        />
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Create chat (paste text)
// ---------------------------------------------------------------------------

function CreateChatDialog({ parentId }: { parentId: string | null }) {
  const busy = useAppStore((s) => s.busy);
  const createChat = useAppStore((s) => s.createChat);
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  const [tags, setTags] = useState("");

  const valid = isValidName(title) && text.trim().length > 0;

  return (
    <Dialog
      title="New Chat"
      hint="Paste the conversation or notes. A brief is generated locally on save."
      submitLabel="Save Chat"
      busy={busy}
      canSubmit={valid}
      onSubmit={() =>
        void createChat(
          parentId,
          title,
          text,
          tags.trim() ? tags.trim() : null,
        )
      }
    >
      <div className="field">
        <label>Title</label>
        <input
          type="text"
          value={title}
          autoFocus
          placeholder="e.g. Gravity Chat"
          onChange={(e) => setTitle(e.target.value)}
        />
      </div>
      <div className="field">
        <label>Chat text</label>
        <textarea
          value={text}
          placeholder={"User:\n…\n\nAssistant:\n…"}
          onChange={(e) => setText(e.target.value)}
        />
        {text.trim() === "" && (
          <div className="field-error">Chat text cannot be empty.</div>
        )}
      </div>
      <div className="field">
        <label>Tags (optional, comma separated)</label>
        <input
          type="text"
          value={tags}
          placeholder="physics, homework"
          onChange={(e) => setTags(e.target.value)}
        />
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Import chat (.txt/.md/.json via browser file input)
// ---------------------------------------------------------------------------

function ImportChatDialog({ parentId }: { parentId: string | null }) {
  const busy = useAppStore((s) => s.busy);
  const importChat = useAppStore((s) => s.importChat);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [text, setText] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [tags, setTags] = useState("");
  const [error, setError] = useState<string | null>(null);

  const defaultTitle = useMemo(() => {
    if (!file) return "";
    const base = file.name.replace(/\.(txt|md|json)$/i, "");
    return base;
  }, [file]);

  const effectiveTitle = title.trim() || defaultTitle;
  const valid = text != null && effectiveTitle.length > 0 && !error;

  const pickFile = async (f: File | undefined | null) => {
    setError(null);
    setFile(null);
    setText(null);
    if (!f) return;
    if (!isSupportedImport(f.name)) {
      setError("Unsupported file type. Use .txt, .md or .json.");
      return;
    }
    if (f.size > MAX_IMPORT_BYTES) {
      setError(`File too large (${formatBytes(f.size)}). Limit is ${formatBytes(MAX_IMPORT_BYTES)}.`);
      return;
    }
    try {
      const content = await readFileAsText(f);
      if (content.trim() === "") {
        setError("The selected file is empty.");
        return;
      }
      setFile(f);
      setText(content);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  };

  return (
    <Dialog
      title="Import Chat"
      hint="Read locally in the app and saved into your tree. JSON message exports are converted automatically."
      submitLabel="Import"
      busy={busy}
      canSubmit={valid}
      onSubmit={() => {
        if (text == null || !file) return;
        void importChat(parentId, file.name, text, tags.trim() ? tags.trim() : null);
      }}
    >
      <div className="field">
        <label>File (.txt, .md, .json)</label>
        <div className="file-row">
          <button
            type="button"
            className="btn"
            onClick={() => fileInputRef.current?.click()}
            disabled={busy}
          >
            Choose file…
          </button>
          <span className="text-dim" style={{ fontSize: 12.5 }}>
            {file ? `${file.name} (${formatBytes(file.size)})` : "No file selected"}
          </span>
        </div>
        <input
          ref={fileInputRef}
          type="file"
          accept=".txt,.md,.json"
          style={{ display: "none" }}
          onChange={(e) => void pickFile(e.target.files?.[0])}
        />
        {error && <div className="field-error">{error}</div>}
      </div>

      {file && (
        <div className="field">
          <label>Title</label>
          <input
            type="text"
            value={title}
            autoFocus
            placeholder={defaultTitle}
            onChange={(e) => setTitle(e.target.value)}
          />
        </div>
      )}

      {text != null && (
        <div className="import-preview">
          Preview: {text.slice(0, 220).replace(/\n/g, " ⏎ ")}
          {text.length > 220 ? " …" : ""}
        </div>
      )}

      <div className="field">
        <label>Tags (optional, comma separated)</label>
        <input
          type="text"
          value={tags}
          placeholder="imported, reference"
          onChange={(e) => setTags(e.target.value)}
        />
      </div>

      {file === null && error == null && (
        <button
          type="button"
          className="btn ghost small"
          onClick={() => fileInputRef.current?.click()}
        >
          Select a file to enable import
        </button>
      )}
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Delete confirmation
// ---------------------------------------------------------------------------

function ConfirmDeleteDialog({
  target,
}: {
  target: import("../../store/appStore").DeleteTarget;
}) {
  const busy = useAppStore((s) => s.busy);
  const deleteRoot = useAppStore((s) => s.deleteRoot);
  const deleteFolder = useAppStore((s) => s.deleteFolder);
  const deleteChat = useAppStore((s) => s.deleteChat);

  const noun =
    target.type === "root"
      ? `index "${target.name}" and EVERYTHING inside it`
      : target.type === "folder"
        ? `folder "${target.name}", all subfolders and all chats inside it`
        : `chat "${target.name}"`;

  const confirm = () => {
    if (target.type === "root") void deleteRoot(target);
    else if (target.type === "folder") void deleteFolder(target);
    else void deleteChat(target);
  };

  return (
    <Dialog
      title="Confirm Deletion"
      hint={`This permanently deletes the ${noun}, including its files on disk. This cannot be undone.`}
      submitLabel="Delete"
      busy={busy}
      canSubmit
      onSubmit={confirm}
    >
    </Dialog>
  );
}
