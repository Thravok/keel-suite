import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { core, event } from "@tauri-apps/api";
import "./App.css";
import {
  type ActionSpec,
  type AppStateSnapshot,
  type KeyButton,
  type LayoutKind,
  type MediaCommand,
  type Profile,
  emptyAction,
  layoutDims,
} from "./types";

const { invoke } = core;
const { listen } = event;

function layoutLabel(layout: LayoutKind): string {
  switch (layout) {
    case "mini":
      return "Mini (3×2)";
    case "xl":
      return "XL (8×4)";
    default:
      return "Original / Mk2 (5×3)";
  }
}

type ActionKindOption = ActionSpec["type"];

const ACTION_KINDS: ActionKindOption[] = [
  "empty",
  "hotkey",
  "open_url",
  "open_app",
  "run_command",
  "media",
  "text",
  "brightness",
  "go_page",
  "go_back",
];

const MEDIA_COMMANDS: MediaCommand[] = [
  "play_pause",
  "next",
  "previous",
  "volume_up",
  "volume_down",
  "mute",
];

function actionKindLabel(kind: ActionKindOption): string {
  switch (kind) {
    case "empty":
      return "Empty";
    case "hotkey":
      return "Hotkey";
    case "open_url":
      return "Open URL";
    case "open_app":
      return "Open app";
    case "run_command":
      return "Run command";
    case "media":
      return "Media";
    case "text":
      return "Type text";
    case "brightness":
      return "Brightness";
    case "go_page":
      return "Go to page";
    case "go_back":
      return "Go back";
  }
}

function defaultAction(kind: ActionKindOption): ActionSpec {
  switch (kind) {
    case "empty":
      return emptyAction();
    case "hotkey":
      return { type: "hotkey", cmd: true, ctrl: false, alt: false, shift: false, key: "c" };
    case "open_url":
      return { type: "open_url", url: "https://example.local" };
    case "open_app":
      return { type: "open_app", path: "" };
    case "run_command":
      return { type: "run_command", program: "", args: [] };
    case "media":
      return { type: "media", command: "play_pause" };
    case "text":
      return { type: "text", text: "" };
    case "brightness":
      return { type: "brightness", percent: 70 };
    case "go_page":
      return { type: "go_page", page_id: "" };
    case "go_back":
      return { type: "go_back" };
  }
}

function mediaLabel(command: MediaCommand): string {
  switch (command) {
    case "play_pause":
      return "Play / pause";
    case "next":
      return "Next track";
    case "previous":
      return "Previous track";
    case "volume_up":
      return "Volume up";
    case "volume_down":
      return "Volume down";
    case "mute":
      return "Mute";
  }
}

function activeProfile(state: AppStateSnapshot | null): Profile | null {
  if (!state?.settings.active_profile_id) return null;
  return state.profiles.find((p) => p.id === state.settings.active_profile_id) ?? null;
}

function currentPage(profile: Profile) {
  return profile.pages.find((p) => p.id === profile.current_page_id) ?? null;
}

interface KeyEditorProps {
  index: number;
  draft: KeyButton;
  pages: Profile["pages"];
  onChange: (next: KeyButton) => void;
  onSave: () => void;
  busy: boolean;
}

function KeyEditor({ index, draft, pages, onChange, onSave, busy }: KeyEditorProps) {
  const kind = draft.action.type as ActionKindOption;

  const setKind = (next: ActionKindOption) => {
    if (next === kind) return;
    onChange({ ...draft, action: defaultAction(next) });
  };

  const setAction = (action: ActionSpec) => {
    onChange({ ...draft, action });
  };

  return (
    <div className="panel__body">
      <p className="panel__section-title">Key #{index}</p>

      <div className="field">
        <span className="field__label">Title</span>
        <input
          className="input"
          value={draft.title}
          onChange={(e) => onChange({ ...draft, title: e.target.value })}
        />
      </div>

      <div className="field">
        <span className="field__label">Color</span>
        <input
          className="input"
          value={draft.color}
          onChange={(e) => onChange({ ...draft, color: e.target.value })}
          placeholder="#2a2e35"
        />
      </div>

      <div className="field">
        <span className="field__label">Action</span>
        <select className="select" value={kind} onChange={(e) => setKind(e.target.value as ActionKindOption)}>
          {ACTION_KINDS.map((k) => (
            <option key={k} value={k}>
              {actionKindLabel(k)}
            </option>
          ))}
        </select>
      </div>

      {draft.action.type === "hotkey" && (
        <>
          <div className="checkbox-row">
            <label>
              <input
                type="checkbox"
                checked={draft.action.cmd}
                onChange={(e) => {
                  const action = draft.action;
                  if (action.type !== "hotkey") return;
                  setAction({ ...action, cmd: e.target.checked });
                }}
              />
              Cmd/Meta
            </label>
            <label>
              <input
                type="checkbox"
                checked={draft.action.ctrl}
                onChange={(e) => {
                  const action = draft.action;
                  if (action.type !== "hotkey") return;
                  setAction({ ...action, ctrl: e.target.checked });
                }}
              />
              Ctrl
            </label>
            <label>
              <input
                type="checkbox"
                checked={draft.action.alt}
                onChange={(e) => {
                  const action = draft.action;
                  if (action.type !== "hotkey") return;
                  setAction({ ...action, alt: e.target.checked });
                }}
              />
              Alt
            </label>
            <label>
              <input
                type="checkbox"
                checked={draft.action.shift}
                onChange={(e) => {
                  const action = draft.action;
                  if (action.type !== "hotkey") return;
                  setAction({ ...action, shift: e.target.checked });
                }}
              />
              Shift
            </label>
          </div>
          <div className="field">
            <span className="field__label">Key</span>
            <input
              className="input"
              value={draft.action.key}
              onChange={(e) => {
                const action = draft.action;
                if (action.type !== "hotkey") return;
                setAction({ ...action, key: e.target.value });
              }}
            />
          </div>
        </>
      )}

      {draft.action.type === "open_url" && (
        <div className="field">
          <span className="field__label">URL</span>
          <input
            className="input"
            value={draft.action.url}
            onChange={(e) => {
              const action = draft.action;
              if (action.type !== "open_url") return;
              setAction({ ...action, url: e.target.value });
            }}
          />
        </div>
      )}

      {draft.action.type === "open_app" && (
        <div className="field">
          <span className="field__label">Path</span>
          <input
            className="input"
            value={draft.action.path}
            onChange={(e) => {
              const action = draft.action;
              if (action.type !== "open_app") return;
              setAction({ ...action, path: e.target.value });
            }}
          />
        </div>
      )}

      {draft.action.type === "run_command" && (
        <>
          <div className="field">
            <span className="field__label">Program</span>
            <input
              className="input"
              value={draft.action.program}
              onChange={(e) => {
                const action = draft.action;
                if (action.type !== "run_command") return;
                setAction({ ...action, program: e.target.value });
              }}
            />
          </div>
          <div className="field">
            <span className="field__label">Args</span>
            <input
              className="input"
              value={draft.action.args.join(" ")}
              onChange={(e) => {
                const action = draft.action;
                if (action.type !== "run_command") return;
                setAction({
                  ...action,
                  args: e.target.value.split(/\s+/).filter(Boolean),
                });
              }}
            />
            <span className="field__hint">Space-separated; no shell</span>
          </div>
        </>
      )}

      {draft.action.type === "media" && (
        <div className="field">
          <span className="field__label">Media</span>
          <select
            className="select"
            value={draft.action.command}
            onChange={(e) => {
              const action = draft.action;
              if (action.type !== "media") return;
              setAction({ ...action, command: e.target.value as MediaCommand });
            }}
          >
            {MEDIA_COMMANDS.map((c) => (
              <option key={c} value={c}>
                {mediaLabel(c)}
              </option>
            ))}
          </select>
        </div>
      )}

      {draft.action.type === "text" && (
        <div className="field">
          <span className="field__label">Text</span>
          <textarea
            className="textarea"
            value={draft.action.text}
            onChange={(e) => {
              const action = draft.action;
              if (action.type !== "text") return;
              setAction({ ...action, text: e.target.value });
            }}
          />
        </div>
      )}

      {draft.action.type === "brightness" && (
        <div className="field">
          <span className="field__label">Percent</span>
          <input
            className="input"
            type="number"
            min={0}
            max={100}
            value={draft.action.percent}
            onChange={(e) => {
              const action = draft.action;
              if (action.type !== "brightness") return;
              setAction({
                ...action,
                percent: Math.min(100, Math.max(0, Number(e.target.value))),
              });
            }}
          />
        </div>
      )}

      {draft.action.type === "go_page" && (
        <div className="field">
          <span className="field__label">Page</span>
          <select
            className="select"
            value={draft.action.page_id}
            onChange={(e) => {
              const action = draft.action;
              if (action.type !== "go_page") return;
              setAction({ ...action, page_id: e.target.value });
            }}
          >
            <option value="">Select page…</option>
            {pages.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </div>
      )}

      <button type="button" className="btn btn--primary" disabled={busy} onClick={onSave}>
        Save key
      </button>
    </div>
  );
}

export default function App() {
  const [state, setState] = useState<AppStateSnapshot | null>(null);
  const [selectedKey, setSelectedKey] = useState<number | null>(null);
  const [keyDraft, setKeyDraft] = useState<KeyButton | null>(null);
  const [status, setStatus] = useState("Ready — offline");
  const [busy, setBusy] = useState(false);
  const [newProfileName, setNewProfileName] = useState("New profile");
  const [newLayout, setNewLayout] = useState<LayoutKind>("original");
  const [newPageName, setNewPageName] = useState("New page");
  const lastClickRef = useRef<{ index: number; at: number } | null>(null);

  const profile = useMemo(() => activeProfile(state), [state]);
  const page = useMemo(() => (profile ? currentPage(profile) : null), [profile]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    (async () => {
      try {
        const snap = await invoke<AppStateSnapshot>("get_state");
        if (!cancelled) setState(snap);
      } catch (e) {
        if (!cancelled) setStatus(String(e));
      }

      unlisten = await listen<AppStateSnapshot>("keel://state", (event) => {
        setState(event.payload);
      });
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (selectedKey === null || !page) {
      setKeyDraft(null);
      return;
    }
    const key = page.keys[selectedKey];
    if (!key) {
      setSelectedKey(null);
      setKeyDraft(null);
      return;
    }
    setKeyDraft(structuredClone(key));
  }, [selectedKey, page]);

  const run = useCallback(async (label: string, fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
      setStatus(label);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const onSelectProfile = (id: string) => {
    void run(`Active profile`, async () => {
      await invoke("set_active_profile", { id });
      setSelectedKey(null);
    });
  };

  const onCreateProfile = () => {
    void run("Profile created", async () => {
      const created = await invoke<Profile>("create_profile", {
        name: newProfileName.trim() || "New profile",
        layout: newLayout,
      });
      await invoke("set_active_profile", { id: created.id });
      setSelectedKey(null);
    });
  };

  const onAddPage = () => {
    void run("Page added", async () => {
      await invoke("add_page", { name: newPageName.trim() || "New page" });
    });
  };

  const onGoPage = (pageId: string) => {
    void run("Switched page", async () => {
      await invoke("go_page", { page_id: pageId, push_stack: false });
    });
  };

  const onNavigateBack = () => {
    void run("Back", async () => {
      await invoke("navigate_back");
      setSelectedKey(null);
    });
  };

  const onBrightness = (brightness: number) => {
    if (!state) return;
    const settings = { ...state.settings, brightness };
    setState({ ...state, settings });
    void invoke("set_settings", { settings }).catch((e) => setStatus(String(e)));
  };

  const onKeyClick = (index: number) => {
    const now = Date.now();
    const last = lastClickRef.current;
    if (last && last.index === index && now - last.at < 350) {
      lastClickRef.current = null;
      void run(`Fired key ${index}`, async () => {
        await invoke("press_key", { index });
      });
      return;
    }
    lastClickRef.current = { index, at: now };
    setSelectedKey(index);
  };

  const onSaveKey = () => {
    if (selectedKey === null || !keyDraft) return;
    void run("Key saved", async () => {
      await invoke("update_key", { index: selectedKey, key: keyDraft });
    });
  };

  const onRefreshHid = () => {
    void run("HID refreshed", async () => {
      await invoke("refresh_hid");
    });
  };

  const dims = profile ? layoutDims(profile.layout) : { rows: 3, cols: 5 };
  const deviceLabel =
    state && state.devices.length > 0
      ? `${state.devices[0].kind} · ${state.devices[0].serial}`
      : "No HID device";

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <span className="brand__title">KEEL</span>
          <span className="brand__tag">offline command deck</span>
        </div>
        <div className="topbar__meta">
          <span>{deviceLabel}</span>
        </div>
      </header>

      <div className="shell">
        <aside className="panel">
          <div className="panel__head">Library</div>
          <div className="panel__body">
            <div className="panel__section">
              <span className="panel__section-title">Profiles</span>
              <ul className="list">
                {(state?.profiles ?? []).map((p) => (
                  <li key={p.id} className="list__item">
                    <button
                      type="button"
                      className={`list__btn ${state?.settings.active_profile_id === p.id ? "is-active" : ""}`}
                      onClick={() => onSelectProfile(p.id)}
                    >
                      {p.name}
                    </button>
                  </li>
                ))}
              </ul>
            </div>

            <div className="panel__section">
              <span className="panel__section-title">New profile</span>
              <input
                className="input"
                value={newProfileName}
                onChange={(e) => setNewProfileName(e.target.value)}
              />
              <select className="select" value={newLayout} onChange={(e) => setNewLayout(e.target.value as LayoutKind)}>
                <option value="mini">{layoutLabel("mini")}</option>
                <option value="original">{layoutLabel("original")}</option>
                <option value="xl">{layoutLabel("xl")}</option>
              </select>
              <button type="button" className="btn btn--primary" disabled={busy} onClick={onCreateProfile}>
                Create profile
              </button>
            </div>

            {profile && (
              <div className="panel__section">
                <span className="panel__section-title">Pages</span>
                <ul className="list">
                  {profile.pages.map((p) => (
                    <li key={p.id} className="list__item">
                      <button
                        type="button"
                        className={`list__btn ${profile.current_page_id === p.id ? "is-active" : ""}`}
                        onClick={() => onGoPage(p.id)}
                      >
                        {p.name}
                      </button>
                    </li>
                  ))}
                </ul>
                <input className="input" value={newPageName} onChange={(e) => setNewPageName(e.target.value)} />
                <button type="button" className="btn" disabled={busy || !profile} onClick={onAddPage}>
                  Add page
                </button>
                {profile.page_stack.length > 0 && (
                  <button type="button" className="btn btn--ghost" disabled={busy} onClick={onNavigateBack}>
                    Navigate back
                  </button>
                )}
              </div>
            )}

            <div className="panel__section">
              <span className="panel__section-title">Brightness</span>
              <div className="slider-row">
                <input
                  type="range"
                  min={0}
                  max={100}
                  value={state?.settings.brightness ?? 70}
                  onChange={(e) => onBrightness(Number(e.target.value))}
                />
                <span>{state?.settings.brightness ?? 70}%</span>
              </div>
            </div>
          </div>
        </aside>

        <main className="deck-main">
          {!profile || !page ? (
            <div className="empty-state">Create a profile to begin.</div>
          ) : (
            <>
              <div className="deck-header">
                <span className="deck-header__title">{profile.name}</span>
                <span className="deck-header__sub">· {page.name}</span>
                <span className="deck-header__layout">{layoutLabel(profile.layout)}</span>
                {profile.page_stack.length > 0 && (
                  <button type="button" className="btn btn--ghost" disabled={busy} onClick={onNavigateBack}>
                    Back
                  </button>
                )}
              </div>

              <div className="deck-frame">
                <div
                  className="deck-grid"
                  style={{ gridTemplateColumns: `repeat(${dims.cols}, 72px)` }}
                >
                  {page.keys.map((key, index) => (
                    <button
                      key={key.id}
                      type="button"
                      className={`key ${selectedKey === index ? "is-selected" : ""}`}
                      style={{ backgroundColor: key.color || "var(--key-face)" }}
                      onClick={() => onKeyClick(index)}
                      title={key.title || `Key ${index}`}
                    >
                      {key.title ? key.title : <span className="key__placeholder">·</span>}
                    </button>
                  ))}
                </div>
                <p className="deck-hint">Click: select · Double-click: run action</p>
              </div>

              <div className="hid-bar">
                <span className={`hid-bar__status ${state?.devices.length ? "" : "is-offline"}`}>
                  {deviceLabel}
                </span>
                <button type="button" className="btn" disabled={busy} onClick={onRefreshHid}>
                  Refresh HID
                </button>
              </div>
            </>
          )}
        </main>

        <aside className="panel panel--editor">
          <div className="panel__head">Key editor</div>
          {selectedKey !== null && keyDraft && profile ? (
            <KeyEditor
              index={selectedKey}
              draft={keyDraft}
              pages={profile.pages}
              onChange={setKeyDraft}
              onSave={onSaveKey}
              busy={busy}
            />
          ) : (
            <div className="panel__body">
              <p className="empty-state">Select a key on the deck.</p>
            </div>
          )}
        </aside>
      </div>

      <footer className="statusbar">{status}</footer>
    </div>
  );
}
