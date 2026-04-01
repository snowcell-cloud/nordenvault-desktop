import { useState } from "react";
import * as api from "../api";
import type { AgentConfig } from "../types";
import { CloudIcon } from "./Icons";

interface Props {
  onProvisioned: (config: AgentConfig) => void;
}

export default function SetupPage({ onProvisioned }: Props) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSetup() {
    setLoading(true);
    setError(null);
    try {
      const config = await api.provisionMachine();
      onProvisioned(config);
    } catch (e) {
      setError(String(e));
      setLoading(false);
    }
  }

  return (
    <div className="setup-page">
      <CloudIcon size={32} style={{ color: "var(--accent)", flexShrink: 0 }} />
      <h2 className="setup-title">Set up this device</h2>
      <p className="setup-desc">
        Register this device with your NordenVault account to start backing up your files.
      </p>

      {error && <div className="error-msg">{error}</div>}

      <button
        className="btn btn-primary btn-full"
        onClick={handleSetup}
        disabled={loading}
      >
        {loading ? (
          <>
            <span className="spinner" style={{ width: 14, height: 14 }} />
            Setting up...
          </>
        ) : (
          "Register this device"
        )}
      </button>
    </div>
  );
}
