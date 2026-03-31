import { useEffect, useState } from "react";
import "./App.css";
import * as api from "./api";
import type { AgentConfig, AgentStatus, AuthStateDto } from "./types";
import { CloudIcon } from "./components/Icons";
import LoginPage from "./components/LoginPage";
import SetupPage from "./components/SetupPage";
import Dashboard from "./components/Dashboard";

type View = "loading" | "login" | "setup" | "dashboard";

function App() {
  const [view, setView] = useState<View>("loading");
  const [auth, setAuth] = useState<AuthStateDto | null>(null);
  const [config, setConfig] = useState<AgentConfig | null>(null);
  const [status, setStatus] = useState<AgentStatus>({
    status: "idle",
    lastBackupAt: null,
    queueDepth: 0,
    errorMessage: null,
  });

  useEffect(() => {
    initApp();
    const interval = setInterval(() => {
      if (auth?.isLoggedIn) {
        api.getStatus().then(setStatus).catch(() => {});
        // Re-fetch auth state so email/name appear once the background profile fetch completes
        if (!auth.email) {
          api.getAuthState().then(setAuth).catch(() => {});
        }
      }
    }, 3000);
    return () => clearInterval(interval);
  }, [auth?.isLoggedIn]);

  async function initApp() {
    try {
      const [authState, cfg] = await Promise.all([
        api.getAuthState(),
        api.getConfig(),
      ]);
      setAuth(authState);
      setConfig(cfg);

      if (!authState.isLoggedIn) {
        setView("login");
      } else {
        // Verify the local machine ID is still valid in the backend.
        // If the record was lost (e.g. provisioned before backend tracked machines),
        // clear it and show the setup page so the user re-provisions cleanly.
        let verifiedMachineId = cfg.machineId;
        if (cfg.machineId) {
          try {
            const desktopStatus = await api.checkDesktopStatus();
            if (!desktopStatus.machineId) {
              verifiedMachineId = null;
            }
          } catch {
            // Offline or auth error — trust the local config for now
          }
        }

        if (!verifiedMachineId) {
          setView("setup");
        } else {
          const st = await api.getStatus();
          setStatus(st);
          setView("dashboard");
        }
      }
    } catch {
      setView("login");
    }
  }

  function handleLoginSuccess(newAuth: AuthStateDto) {
    setAuth(newAuth);
    setView("setup");
  }

  async function handleProvisioned(newConfig: AgentConfig) {
    setConfig(newConfig);
    const st = await api.getStatus().catch(() => status);
    setStatus(st);
    setView("dashboard");
  }

  async function handleLogout() {
    await api.logout().catch(() => {});
    setAuth(null);
    setConfig(null);
    setView("login");
  }

  if (view === "loading") {
    return (
      <div className="app" style={{ alignItems: "center", justifyContent: "center" }}>
        <div className="spinner" />
      </div>
    );
  }

  return (
    <div className="app">
      <header className="header">
        <CloudIcon className="header-logo" />
        <span className="header-title">NordenVault</span>
        {view === "dashboard" && (
          <StatusBadge status={status.status} />
        )}
      </header>

      {view === "login" && <LoginPage onLogin={handleLoginSuccess} />}
      {view === "setup" && auth && (
        <SetupPage auth={auth} onProvisioned={handleProvisioned} />
      )}
      {view === "dashboard" && auth && config && (
        <Dashboard
          auth={auth}
          config={config}
          status={status}
          onConfigChange={setConfig}
          onStatusChange={setStatus}
          onLogout={handleLogout}
        />
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const labels: Record<string, string> = {
    idle: "Up to date",
    syncing: "Syncing...",
    error: "Error",
    paused: "Paused",
  };
  return (
    <span className={`status-badge status-${status}`}>
      <span className="dot" />
      {labels[status] ?? status}
    </span>
  );
}

export default App;
