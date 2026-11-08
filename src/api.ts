import { invoke } from "@tauri-apps/api/core";
import { cleanDisplay } from "./util";
import type {
  Account, AppInfo, AuthStatus, CacheStatus, DraftContent, FlushResult, LabelInfo, OutboxItem, PollResult, OutgoingMessage, SavedFile, SenderLists, SentInfo, ThreadAction, ThreadDetail,
  SyncResult, ThreadPage, UpdateInfo, UpdateState,
} from "./types";

/** Lista nie może wisieć w nieskończoność: po limicie czasu pokazujemy błąd zamiast pustego ekranu. */
function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const t = window.setTimeout(() => reject(new Error("Gmail nie odpowiedział na czas. Spróbuj ponownie.")), ms);
    p.then((v) => { window.clearTimeout(t); resolve(v); }, (e) => { window.clearTimeout(t); reject(e); });
  });
}

function cleanPage(p: ThreadPage): ThreadPage {
  return {
    ...p,
    threads: p.threads.map((t) => ({
      ...t,
      subject: cleanDisplay(t.subject, 200),
      participants: cleanDisplay(t.participants, 120),
      snippet: cleanDisplay(t.snippet, 200),
      risk: { ...t.risk, reasons: t.risk.reasons.map((r) => cleanDisplay(r, 300)) },
    })),
  };
}

function cleanDetail(d: ThreadDetail): ThreadDetail {
  return {
    ...d,
    subject: cleanDisplay(d.subject, 300),
    messages: d.messages.map((m) => ({
      ...m,
      from: cleanDisplay(m.from, 120),
      to: cleanDisplay(m.to, 500),
      cc: cleanDisplay(m.cc, 500),
      risk: { ...m.risk, reasons: m.risk.reasons.map((r) => cleanDisplay(r, 300)) },
    })),
  };
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  authStatus: () => invoke<AuthStatus>("auth_status"),
  saveClient: (clientId: string, clientSecret: string) =>
    invoke<void>("save_client", { clientId, clientSecret }),
  clearClient: () => invoke<void>("clear_client"),
  signIn: () => invoke<string>("sign_in"),
  signOut: () => invoke<void>("sign_out"),
  listThreads: (folder: string, pageToken: string | null) =>
    withTimeout(invoke<ThreadPage>("list_threads", { folder, pageToken }), 60_000).then(cleanPage),
  getThread: (id: string) => invoke<ThreadDetail>("get_thread", { id }).then(cleanDetail),
  searchThreads: (query: string, pageToken: string | null) =>
    invoke<ThreadPage>("search_threads", { query, pageToken }).then(cleanPage),
  syncNow: () => invoke<SyncResult>("sync_now"),
  listAccounts: () => invoke<Account[]>("list_accounts"),
  switchAccount: (email: string) => invoke<string>("switch_account", { email }),
  queueMessage: (msg: OutgoingMessage) => invoke<number>("queue_message", { msg }),
  listOutbox: () => invoke<OutboxItem[]>("list_outbox"),
  retryOutbox: (id: number) => invoke<void>("retry_outbox", { id }),
  deleteOutbox: (id: number) => invoke<void>("delete_outbox", { id }),
  flushOutbox: () => invoke<FlushResult>("flush_outbox"),
  pollMail: (notifyEnabled: boolean, showContent: boolean) =>
    invoke<PollResult>("poll_mail", { notifyEnabled, showContent }),
  createLabel: (name: string) => invoke<LabelInfo>("create_label", { name }),
  setThreadLabel: (id: string, label: string, add: boolean) =>
    invoke<void>("set_thread_label", { id, label, add }),
  testNotification: () => invoke<void>("test_notification"),
  cacheStatus: () => invoke<CacheStatus>("cache_status"),
  setOfflineEnabled: (enabled: boolean) => invoke<CacheStatus>("set_offline_enabled", { enabled }),
  clearCache: () => invoke<CacheStatus>("clear_cache"),
  sendMessage: (msg: OutgoingMessage) => invoke<SentInfo>("send_message", { msg }),
  saveDraft: (msg: OutgoingMessage) => invoke<string>("save_draft", { msg }),
  deleteDraft: (draftId: string) => invoke<void>("delete_draft", { draftId }),
  openDraft: (threadId: string) => invoke<DraftContent>("open_draft", { threadId }),
  threadAction: (id: string, action: ThreadAction) => invoke<void>("thread_action", { id, action }),
  getSenderLists: () => invoke<SenderLists>("get_sender_lists"),
  addSenderRule: (kind: "blocked" | "allowed", entry: string) => invoke<string>("add_sender_rule", { kind, entry }),
  removeSenderRule: (kind: "blocked" | "allowed", entry: string) => invoke<void>("remove_sender_rule", { kind, entry }),
  setThreadSafe: (id: string, safe: boolean) => invoke<void>("set_thread_safe", { id, safe }),
  fetchImage: (url: string) => invoke<string>("fetch_image", { url }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  downloadAttachment: (messageId: string, attachmentId: string, name: string) =>
    invoke<SavedFile>("download_attachment", { messageId, attachmentId, name }),
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
