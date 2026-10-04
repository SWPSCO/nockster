// Chrome extension specific type definitions

export interface ExtensionMessage {
  type: MessageType;
  data?: any;
  payload?: any;
  id?: string;
  rpcUrl?: string;
}

export type MessageType =
  | 'networkStatus'
  | 'broadcastTransaction'
  | 'fetchBalance'
  | 'signMessage'
  | 'getAccounts'
  | 'requestPermission'
  | 'rpcRequest'
  | 'openHandoff';

export interface ExtensionResponse {
  success: boolean;
  data?: any;
  error?: string;
}

export interface TabInfo {
  id: number;
  url: string;
  title: string;
  origin: string;
}

export interface Permission {
  origin: string;
  accounts: string[];
  grantedAt: number;
}
