import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import type { AgentConfig } from "../types";

export default function BackupPrivacy({ config, onChange }: { config: AgentConfig; onChange: (cfg: AgentConfig) => void }) {
  const [mode, setMode] = useState<"closed" | "standard" | "create" | "connect" | "restore">("closed");
  const [key, setKey] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [snapshots, setSnapshots] = useState<api.Snapshot[] | null>(null);
  const [selected, setSelected] = useState("");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  function close() {
    setMode("closed"); setKey(""); setConfirmation(""); setSnapshots(null); setSelected(""); setError("");
  }
  async function perform(action: () => Promise<void>) {
    setBusy(true); setError(""); setMessage("");
    try { await action(); } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  async function submit() {
    await perform(async () => {
      if (mode === "standard") {
        onChange(await api.enableStandardBackup());
        close(); setMessage("Standard backup enabled.");
      } else if (mode === "restore") {
        if (!snapshots) {
          const items = await api.backupSnapshots(key);
          setSnapshots(items); setSelected(items[0]?.id || "");
        } else {
          const dest = await open({ directory: true, multiple: false });
          if (typeof dest !== "string") return;
          const result = await api.restoreSnapshot(key, selected, dest);
          close(); setMessage("Verified restore saved in " + result);
        }
      } else {
        onChange(await api.enableEncryptedBackup(key, mode === "create"));
        close(); setMessage("Encrypted backups enabled.");
      }
    });
  }
  return <section className="section backup-privacy">
    <h2 className="section-label">Backup privacy</h2>
    {mode === "closed" && <>
      <p>{config.backupMode === "private" ? "Private backup is active. Files and names are encrypted before upload."
        : config.backupMode === "standard" ? "Standard backup is active. You can open these files in the web dashboard."
        : "Choose how this device backs up your folders. Older uploads keep their original protection."}</p>
      {config.backupMode !== "private" && <button className="btn btn-primary" disabled={busy} onClick={() => perform(async () => {
        if (config.encryptedRepository) { setMode("connect"); return; }
        setKey(await api.generateRecoveryKey()); setMode("create");
      })}>Private backup{config.backupMode === "unselected" ? " (recommended)" : ""}</button>}
      {config.backupMode !== "private" && <p>Only someone with the recovery key and storage access can restore file contents and names.</p>}
      {config.backupMode !== "standard" && <button className="btn" disabled={busy} onClick={() => {close(); setMode("standard");}}>Standard backup</button>}
      {config.backupMode !== "standard" && <p>Open files in the dashboard. Snowcell’s authorised storage administrators can also read them.</p>}
      <button className="btn" onClick={() => {close(); setMode("connect");}}>Use a saved key</button>
      <button className="btn" onClick={() => {close(); setMode("restore");}}>Restore a private backup</button>
    </>}
    {mode === "standard" && <>
      <p>Standard backups are readable in the dashboard and by authorised storage administrators. You won’t need a separate recovery key.</p>
      {config.backupMode === "private" && <p>Your existing private snapshots keep their encryption. Choose folders again after switching to decide which files to upload in Standard mode.</p>}
      <button className="btn btn-primary" disabled={busy} onClick={submit}>Confirm standard backup</button>
      <button className="btn" disabled={busy} onClick={close}>Cancel</button>
    </>}
    {mode !== "closed" && mode !== "standard" && <>
      {mode === "create" ? <>
        <p>Your computer remembers this key for daily backups. Save a spare copy in a password manager or on paper, outside your backup folders. On a replacement computer, sign in and use the key to restore. Snowcell cannot replace a lost key.</p>
        <label>Recovery key<textarea readOnly value={key} spellCheck={false} onFocus={e => e.target.select()} /></label>
        <label>Confirm the last 6 characters from your saved copy<input autoComplete="off" value={confirmation} onChange={e => setConfirmation(e.target.value)} /></label>
      </> : <>
        <label>Recovery key<input type="password" autoComplete="off" spellCheck={false} value={key} disabled={busy} onChange={e => {setKey(e.target.value); setSnapshots(null);}} /></label>
        <p>{mode === "restore" ? "Use a key from any device in this storage organisation. Restored files go into a new folder." : "Reconnect this device to the existing encrypted repository."}</p>
      </>}
      {snapshots && (snapshots.length ? <label>Backup to restore<select value={selected} onChange={e => setSelected(e.target.value)}>
        {snapshots.map(s => <option key={s.id} value={s.id}>{new Date(s.time).toLocaleString()} · {s.id.slice(0,8)}</option>)}
      </select></label> : <p>This repository has no snapshots yet.</p>)}
      <button className="btn btn-primary" disabled={busy || !key.trim() || (mode === "create" && confirmation !== key.slice(-6)) || (mode === "restore" && snapshots !== null && !selected)} onClick={submit}>
        {busy ? "Working…" : mode !== "restore" ? "Enable encrypted backups" : snapshots ? "Choose restore folder" : "Find backups"}
      </button>
      <button className="btn" disabled={busy} onClick={close}>Cancel</button>
    </>}
    {error && <p className="error-msg" role="alert">{error}</p>}
    {message && <p role="status">{message}</p>}
  </section>;
}
