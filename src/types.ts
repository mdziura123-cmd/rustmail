export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  appimage: boolean;
}

export interface AuthStatus {
  configured: boolean;
  builtin?: boolean;
  email: string | null;
  needsReauth?: boolean;
}

export interface Risk {
  level: "none" | "warn" | "high";
  score: number;
  reasons: string[];
}

export interface ThreadSummary {
  id: string;
  subject: string;
  participants: string;
  snippet: string;
  lastDateMs: number;
  unread: boolean;
  starred: boolean;
  count: number;
  risk: Risk;
  dismissed: boolean;
  labels?: string[];
}

export interface ThreadPage {
  threads: ThreadSummary[];
  nextPageToken: string | null;
  /** Prawda, gdy wynik pochodzi z lokalnej pamięci (brak połączenia). */
  offline: boolean;
}

export interface CacheStatus {
  available: boolean;
  enabled: boolean;
  ready: boolean;
  threads: number;
  dbPath: string;
}

export interface Account {
  email: string;
  active: boolean;
}

export interface OutboxItem {
  id: number;
  createdMs: number;
  to: string;
  subject: string;
  /** queued | sending | failed | uncertain */
  state: string;
  error: string | null;
}

export interface FlushResult {
  sent: number;
  failed: number;
}

export interface LabelInfo {
  id: string;
  name: string;
  unread: number;
}

export interface PollResult {
  inboxUnread: number;
  spamUnread: number;
  labels: LabelInfo[];
  notified: number;
  spamMoved: number;
  spamFlagged: FlagInfo[];
}

export type SpamMode = "off" | "flag" | "certain" | "aggressive";

export interface SpamConfig {
  mode: SpamMode;
  headers: boolean;
  rules: boolean;
  bayes: boolean;
  lists: boolean;
  provider: string;
  custom: string[];
}

export interface SpamStatus {
  bayesSpam: number;
  bayesHam: number;
  bayesTokens: number;
  bayesReady: boolean;
  listDomains: number;
  listProvider: string | null;
  listAgeHours: number | null;
  providers: { id: string; label: string }[];
}

export interface SpamLogEntry {
  id: string;
  ts: number;
  action: string;
  subject: string;
  sender: string;
  snippet: string;
  reason: string;
  open: boolean;
}

export interface SpamScanOutcome {
  moved: number;
  flagged: number;
  checked: number;
  notes: string[];
}

export interface TrainItem {
  id: string;
  subject: string;
  snippet: string;
  participants: string;
}

export interface FlagInfo {
  id: string;
  reason: string;
}

export interface SyncResult {
  active: boolean;
  changed: string[];
  reset: boolean;
}

export interface Attachment {
  name: string;
  mime: string;
  size: number;
  id?: string | null;
}

export interface SavedFile {
  /** Brak ścieżki oznacza, że skaner wykrył zagrożenie i plik nie został zapisany. */
  path: string | null;
  risky: boolean;
  scan: "clean" | "infected" | "unavailable" | "failed";
  scanDetail: string;
  typeWarning: string | null;
}

export interface SenderLists {
  blocked: string[];
  allowed: string[];
}

export interface MessageView {
  id: string;
  from: string;
  fromEmail: string;
  mine: boolean;
  dateMs: number;
  body: string;
  htmlOnly: boolean;
  html: string;
  attachments: Attachment[];
  to: string;
  cc: string;
  replyTo: string;
  messageId: string;
  references: string;
  risk: Risk;
}

export interface ThreadDetail {
  id: string;
  subject: string;
  messages: MessageView[];
  unread: boolean;
  starred: boolean;
  inInbox: boolean;
  inSpam: boolean;
  inTrash: boolean;
  isDraft: boolean;
  dismissed: boolean;
  offline?: boolean;
  labels?: string[];
}

export interface OutAttachment {
  name: string;
  mime: string;
  dataBase64: string;
}

export interface OutgoingMessage {
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  html: string;
  inReplyTo: string | null;
  references: string | null;
  threadId: string | null;
  draftId: string | null;
  attachments: OutAttachment[];
}

export interface SentInfo {
  id: string;
  threadId: string;
}

export interface DraftContent {
  draftId: string;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  html: string;
  inReplyTo: string;
  references: string;
}

export type ThreadAction =
  | "archive" | "unarchive" | "read" | "unread" | "star" | "unstar"
  | "spam" | "notspam" | "trash" | "untrash";

export interface UpdateInfo {
  available: boolean;
  current: string;
  latest: string;
  notes: string;
}

export interface UpdateState {
  appimage: boolean;
  hasPrevious: boolean;
}
