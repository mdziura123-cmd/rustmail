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
}

export interface ThreadPage {
  threads: ThreadSummary[];
  nextPageToken: string | null;
}

export interface Attachment {
  name: string;
  mime: string;
  size: number;
}

export interface MessageView {
  id: string;
  from: string;
  fromEmail: string;
  mine: boolean;
  dateMs: number;
  body: string;
  htmlOnly: boolean;
  attachments: Attachment[];
}

export interface ThreadDetail {
  id: string;
  subject: string;
  messages: MessageView[];
}

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
