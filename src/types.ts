export interface AuthStateDto {
  isLoggedIn: boolean;
  email: string | null;
  name: string | null;
  userId: string | null;
}

export interface WatchedFolder {
  id: string;
  path: string;
  enabled: boolean;
}

export interface AgentConfig {
  workosClientId: string | null;
  machineId: string | null;
  machineName: string | null;
  orgId: string | null;
  credentialId: string | null;
  bucketName: string | null;
  endpointUrl: string | null;
  region: string | null;
  accessKeyId: string | null;
  watchedFolders: WatchedFolder[];
}

export interface AgentStatus {
  status: "idle" | "syncing" | "error" | "paused";
  lastBackupAt: string | null;
  queueDepth: number;
  errorMessage: string | null;
}

export interface DesktopStatusResponse {
  hasOrganization: boolean;
  hasStorage: boolean;
  machineId: string | null;
  machineName: string | null;
}
