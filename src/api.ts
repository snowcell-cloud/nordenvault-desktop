import { invoke } from "@tauri-apps/api/core";
import type {
  AgentConfig,
  AgentStatus,
  AuthStateDto,
  DesktopStatusResponse,
} from "./types";

export const getAuthState = () => invoke<AuthStateDto>("get_auth_state");
export const startLogin = () => invoke<string>("start_login");
export const handleAuthCallback = (code: string, stateParam: string) =>
  invoke<AuthStateDto>("handle_auth_callback", { code, stateParam });
export const logout = () => invoke<void>("logout");
export const resetDevice = () => invoke<void>("reset_device");

export const checkDesktopStatus = () =>
  invoke<DesktopStatusResponse>("check_desktop_status");
export const provisionMachine = () =>
  invoke<AgentConfig>("provision_machine");

export const getConfig = () => invoke<AgentConfig>("get_config");
export const addFolder = (path: string) =>
  invoke<AgentConfig>("add_folder", { path });
export const removeFolder = (folderId: string) =>
  invoke<AgentConfig>("remove_folder", { folderId });
export const toggleFolder = (folderId: string, enabled: boolean) =>
  invoke<AgentConfig>("toggle_folder", { folderId, enabled });

export const getStatus = () => invoke<AgentStatus>("get_status");
export const pauseSync = () => invoke<void>("pause_sync");
export const resumeSync = () => invoke<void>("resume_sync");
