// Network and blockchain type definitions

export interface NetworkStatus {
  connected: boolean;
  blockHeight: number;
  peers: number;
  syncProgress?: number;
}

export interface BlockInfo {
  height: number;
  hash: string;
  timestamp: number;
  transactions: number;
}

export interface NetworkConfig {
  name: string;
  rpcUrl: string;
  explorerUrl?: string;
  chainId: string;
}
