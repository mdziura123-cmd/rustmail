import { invoke } from "@tauri-apps/api/core";
import { cleanDisplay, stripEmoji } from "./util";
import type {
  Account, AppInfo, AuthStatus, CacheStatus, DraftContent, FlushResult, LabelInfo, OutboxItem, PollResult, OutgoingMessage, SavedFile, SenderLists, SentInfo, ThreadAction, ThreadDetail,
  SyncResult, ThreadPage, UpdateInfo, UpdateState,
  SpamConfig, SpamLogEntry, SpamScanOutcome, SpamStatus, TrainItem,
  ImapAccount, ImapActOut, ImapFolder, ImapMsg, ImapPage,
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
      body: stripEmoji(m.body ?? ""),
      html: stripEmoji(m.html ?? ""),
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
    invoke<PollResult>("poll_mail", { notifyEnabled, showContent }).then((r) => ({
      ...r,
      labels: r.labels.map((l) => ({ ...l, name: cleanDisplay(l.name, 100) || l.id })),
    })),
  spamGetConfig: () => invoke<SpamConfig>("spam_get_config"),
  spamSetConfig: (config: SpamConfig) => invoke<SpamConfig>("spam_set_config", { config }),
  spamStatus: () => invoke<SpamStatus>("spam_status"),
  spamWipeData: () => invoke<void>("spam_wipe_data"),
  composeBackupSet: (json: string) => invoke<void>("compose_backup_set", { json }),
  composeBackupGet: () => invoke<string | null>("compose_backup_get"),
  composeBackupClear: () => invoke<void>("compose_backup_clear"),
  spamLog: () =>
    invoke<SpamLogEntry[]>("spam_log").then((v) =>
      v.map((e) => ({
        ...e,
        subject: cleanDisplay(e.subject, 120),
        sender: cleanDisplay(e.sender, 80),
        snippet: cleanDisplay(e.snippet, 200),
        reason: cleanDisplay(e.reason, 400),
      })),
    ),
  spamScanNow: () => invoke<SpamScanOutcome>("spam_scan_now"),
  spamTrain: (items: TrainItem[], isSpam: boolean) => invoke<number>("spam_train", { items, isSpam }),
  spamBootstrap: () => invoke<{ spam: number; ham: number }>("spam_bootstrap"),
  spamRefreshLists: () => invoke<number>("spam_refresh_lists"),
  spamRestore: (item: TrainItem) => invoke<void>("spam_restore", { item }),
  spamMove: (item: TrainItem) => invoke<void>("spam_move", { item }),
  createLabel: (name: string) => invoke<LabelInfo>("create_label", { name }),
  imapAccounts: () => invoke<ImapAccount[]>("imap_accounts"),
  imapLogin: (email: string, password: string, host: string) => invoke<ImapAccount>("imap_login", { email, password, host }),
  imapLogout: (account: string) => invoke<void>("imap_logout", { account }),
  imapFolders: async (account: string) => (await invoke<ImapFolder[]>("imap_folders", { account })).map((f) => ({ ...f, label: cleanDisplay(f.label, 80) })),
  imapList: async (account: string, folder: string, offset: number) => {
    const p = await invoke<ImapPage>("imap_list", { account, folder, offset });
    return { ...p, rows: p.rows.map((r) => ({ ...r, from: cleanDisplay(r.from, 120), subject: cleanDisplay(r.subject, 200) })) };
  },
  imapSearch: async (account: string, folder: string, query: string) => {
    const p = await invoke<ImapPage>("imap_search", { account, folder, query });
    return { ...p, rows: p.rows.map((r) => ({ ...r, from: cleanDisplay(r.from, 120), subject: cleanDisplay(r.subject, 200) })) };
  },
  imapSend: (account: string, msg: OutgoingMessage) => invoke<{ warning: string | null }>("imap_send", { account, msg }),
  imapCounts: (account: string) => invoke<{ name: string; unseen: number }[]>("imap_counts", { account }),
  imapGet: async (account: string, folder: string, uid: number) => {
    const m = await invoke<ImapMsg>("imap_get", { account, folder, uid });
    return {
      ...m, from: cleanDisplay(m.from, 200), to: cleanDisplay(m.to, 500), cc: cleanDisplay(m.cc, 500),
      subject: cleanDisplay(m.subject, 300), text: stripEmoji(m.text), html: stripEmoji(m.html),
      attachments: m.attachments.map((x) => ({ ...x, name: cleanDisplay(x.name, 150) })),
    } as ImapMsg;
  },
  imapAct: (account: string, folder: string, uid: number, action: string) => invoke<ImapActOut>("imap_act", { account, folder, uid, action }),
  imapMove: (account: string, folder: string, uid: number, target: string) => invoke<ImapActOut>("imap_move", { account, folder, uid, target }),
  imapSaveDraft: (account: string, msg: OutgoingMessage, oldUid: number | null) => invoke<number>("imap_save_draft", { account, msg, oldUid }),
  imapDeleteDraft: (account: string, uid: number) => invoke<void>("imap_delete_draft", { account, uid }),
  importLabel: (name: string) => invoke<string>("import_label", { name }),
  importMessage: (dataBase64: string, labelId: string) => invoke<string>("import_message", { dataBase64, labelId }),
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
  imapPoll: (account: string, notifyEnabled: boolean, showContent: boolean) =>
    invoke<{ moved: number; notified: number }>("imap_poll", { account, notifyEnabled, showContent }),
  imapDownload: (account: string, folder: string, uid: number, index: number, name: string) =>
    invoke<SavedFile>("imap_download", { account, folder, uid, index, name }),
  translateKeys: () => invoke<{ deepl: boolean; google: boolean; microsoft: boolean }>("translate_keys"),
  translateSetKey: (provider: string, key: string, region?: string) => invoke<void>("translate_set_key", { provider, key, region: region ?? null }),
  translateClearKey: (provider: string) => invoke<void>("translate_clear_key", { provider }),
  translateText: (text: string, provider: string) => invoke<{ translated: string; truncated: boolean }>("translate_text", { text, provider }),
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
