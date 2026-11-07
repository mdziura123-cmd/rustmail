import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo, AuthStatus, DraftContent, OutgoingMessage, SentInfo, ThreadAction, ThreadDetail,
  ThreadPage, UpdateInfo, UpdateState,
} from "./types";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  authStatus: () => invoke<AuthStatus>("auth_status"),
  saveClient: (clientId: string, clientSecret: string) =>
    invoke<void>("save_client", { clientId, clientSecret }),
  clearClient: () => invoke<void>("clear_client"),
  signIn: () => invoke<string>("sign_in"),
  signOut: () => invoke<void>("sign_out"),
  listThreads: (folder: string, pageToken: string | null) =>
    invoke<ThreadPage>("list_threads", { folder, pageToken }),
  getThread: (id: string) => invoke<ThreadDetail>("get_thread", { id }),
  sendMessage: (msg: OutgoingMessage) => invoke<SentInfo>("send_message", { msg }),
  saveDraft: (msg: OutgoingMessage) => invoke<string>("save_draft", { msg }),
  deleteDraft: (draftId: string) => invoke<void>("delete_draft", { draftId }),
  openDraft: (threadId: string) => invoke<DraftContent>("open_draft", { threadId }),
  threadAction: (id: string, action: ThreadAction) => invoke<void>("thread_action", { id, action }),
  getSignature: () => invoke<string>("get_signature"),
  setSignature: (text: string) => invoke<void>("set_signature", { text }),
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  installUpdate: () => invoke<string>("install_update"),
  rollbackUpdate: () => invoke<void>("rollback_update"),
  updateState: () => invoke<UpdateState>("update_state"),
  restartApp: () => invoke<void>("restart_app"),
};

export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Nieznany błąd";
}
