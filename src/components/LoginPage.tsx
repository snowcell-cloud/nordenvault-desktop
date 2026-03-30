import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import * as api from "../api";
import type { AuthStateDto } from "../types";
import { CloudIcon } from "./Icons";

interface Props {
  onLogin: (auth: AuthStateDto) => void;
}

export default function LoginPage({ onLogin }: Props) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleLogin() {
    setLoading(true);
    setError(null);

    try {
      const authUrl = await api.startLogin();

      // Listen for the deep-link callback event emitted from Rust
      const unlisten = await listen<void>("auth:complete", async () => {
        unlisten();
        try {
          const auth = await api.getAuthState();
          if (auth.isLoggedIn) {
            onLogin(auth);
          } else {
            setError("Login failed. Please try again.");
          }
        } catch (e) {
          setError(String(e));
        } finally {
          setLoading(false);
        }
      });

      // Open the auth URL in the system browser
      await openUrl(authUrl);
    } catch (e) {
      setError(String(e));
      setLoading(false);
    }
  }

  return (
    <div className="login-page">
      <CloudIcon className="login-logo" />
      <h1 className="login-title">NordenVault</h1>
      <p className="login-subtitle">
        Secure backups, stored in Europe.
        <br />
        Sign in to start syncing your files.
      </p>

      {error && <div className="error-msg">{error}</div>}

      <button
        className="btn btn-primary btn-full"
        onClick={handleLogin}
        disabled={loading}
        style={{ marginTop: 8 }}
      >
        {loading ? (
          <>
            <span className="spinner" style={{ width: 14, height: 14 }} />
            Opening browser...
          </>
        ) : (
          "Sign in with NordenVault"
        )}
      </button>
    </div>
  );
}
