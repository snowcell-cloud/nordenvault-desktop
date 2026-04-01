import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import type { AgentConfig, AgentStatus, AuthStateDto } from "../types";
import {
  FolderIcon,
  LogOutIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  TrashIcon,
} from "./Icons";

interface Props {
  auth: AuthStateDto;
  config: AgentConfig;
  status: AgentStatus;
  onConfigChange: (cfg: AgentConfig) => void;
  onStatusChange: (st: AgentStatus) => void;
  onLogout: () => void;
}

export default function Dashboard({
  auth,
  config,
  status,
  onConfigChange,
  onStatusChange,
  onLogout,
}: Props) {
  const [addingFolder, setAddingFolder] = useState(false);
  const [togglingPause, setTogglingPause] = useState(false);
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<string>("update:available", (e) => {
      setUpdateVersion(e.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  async function handleAddFolder() {
    setAddingFolder(true);
    try {
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === "string") {
        const newConfig = await api.addFolder(selected);
        onConfigChange(newConfig);
      }
    } catch {
      // user cancelled
    } finally {
      setAddingFolder(false);
    }
  }

  async function handleRemoveFolder(folderId: string) {
    const newConfig = await api.removeFolder(folderId).catch(() => config);
    onConfigChange(newConfig);
  }

  async function handleToggleFolder(folderId: string, enabled: boolean) {
    const newConfig = await api
      .toggleFolder(folderId, enabled)
      .catch(() => config);
    onConfigChange(newConfig);
  }

  async function handleTogglePause() {
    setTogglingPause(true);
    try {
      if (status.status === "paused") {
        await api.resumeSync();
        onStatusChange({ ...status, status: "idle" });
      } else {
        await api.pauseSync();
        onStatusChange({ ...status, status: "paused" });
      }
    } finally {
      setTogglingPause(false);
    }
  }

  const isPaused = status.status === "paused";
  const lastBackup = status.lastBackupAt
    ? new Date(status.lastBackupAt).toLocaleString()
    : "Never";

  return (
    <>
      {/* Update banner */}
      {updateVersion && (
        <div className="update-banner">
          Update available: v{updateVersion} —{" "}
          <a
            href="https://nordenvault.com/download"
            target="_blank"
            rel="noreferrer"
          >
            Download
          </a>
        </div>
      )}

      {/* Stats */}
      <div className="section">
        <div className="stats-row">
          <div className="stat-card">
            <div className="stat-value">{config.watchedFolders.length}</div>
            <div className="stat-label">Folders</div>
          </div>
          <div className="stat-card">
            <div className="stat-value">{status.queueDepth}</div>
            <div className="stat-label">Pending</div>
          </div>
        </div>
      </div>

      {/* Machine info */}
      {config.machineName && (
        <div className="section">
          <div className="section-label" style={{ marginBottom: 6 }}>
            This device
          </div>
          <div className="machine-info">
            <div className="machine-info-name">{config.machineName}</div>
            {config.machineId && (
              <div className="machine-info-id">
                {config.machineId.slice(0, 8)}...
              </div>
            )}
          </div>
        </div>
      )}

      {/* Error display */}
      {status.status === "error" && status.errorMessage && (
        <div className="section">
          <div className="error-msg">{status.errorMessage}</div>
          <button
            className="btn btn-ghost"
            style={{ marginTop: 6, fontSize: 11, color: "var(--text2)" }}
            onClick={async () => {
              await api.resetDevice().catch(() => {});
              onLogout();
            }}
          >
            Reset device and sign out
          </button>
        </div>
      )}

      {/* Folders */}
      <div className="section" style={{ flex: 1 }}>
        <div className="section-header">
          <span className="section-label">Watched folders</span>
          <button
            className="btn btn-ghost"
            onClick={handleAddFolder}
            disabled={addingFolder}
            title="Add folder"
          >
            <PlusIcon size={14} />
          </button>
        </div>

        {config.watchedFolders.length === 0 ? (
          <div className="empty-state">
            No folders yet.
            <br />
            Click + to add a folder to back up.
          </div>
        ) : (
          <div className="folder-list">
            {config.watchedFolders.map((folder) => {
              const parts = folder.path.split("/");
              const name = parts[parts.length - 1] || folder.path;
              const parent = parts.slice(0, -1).join("/");

              return (
                <div key={folder.id} className="folder-item">
                  <FolderIcon
                    size={14}
                    className={
                      folder.enabled
                        ? undefined
                        : "folder-icon-disabled"
                    }
                    style={{ color: folder.enabled ? "var(--accent)" : "var(--text2)", flexShrink: 0 }}
                  />
                  <div className="folder-item-path">
                    <div className="folder-item-name">{name}</div>
                    <div className="folder-item-full">{parent}</div>
                  </div>
                  <div className="folder-actions">
                    <label className="toggle">
                      <input
                        type="checkbox"
                        checked={folder.enabled}
                        onChange={(e) =>
                          handleToggleFolder(folder.id, e.target.checked)
                        }
                      />
                      <span className="toggle-track" />
                    </label>
                    <button
                      className="btn btn-danger-ghost"
                      onClick={() => handleRemoveFolder(folder.id)}
                      title="Remove folder"
                    >
                      <TrashIcon size={12} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="footer">
        <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
          <span className="footer-meta">Last backup: {lastBackup}</span>
          <span className="footer-meta" style={{ opacity: 0.6 }}>{auth.email}</span>
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          <button
            className="btn btn-ghost"
            onClick={handleTogglePause}
            disabled={togglingPause}
            title={isPaused ? "Resume sync" : "Pause sync"}
          >
            {isPaused ? <PlayIcon size={13} /> : <PauseIcon size={13} />}
          </button>
          <button
            className="btn btn-ghost"
            onClick={onLogout}
            title={`Sign out (${auth.email ?? ""})`}
          >
            <LogOutIcon size={13} />
          </button>
        </div>
      </div>
    </>
  );
}
