export interface Message {
  id: string;
  from: string;
  mine: boolean;
  date: string;
  body: string;
  attachments?: string[];
}

export interface Thread {
  id: string;
  subject: string;
  participants: string;
  snippet: string;
  date: string;
  unread: boolean;
  starred: boolean;
  messages: Message[];
}

export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  appimage: boolean;
}
