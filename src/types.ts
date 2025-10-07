export type BatteryInfo = {
  level: number;
  tempC: number;
  status: string;
};

export type DeviceInfo = {
  brand: string;
  model: string;
  androidVersion: string;
  apiLevel: number;
  buildId: string;
  fingerprint: string;
  uptimeMs: number;
  reportTime: string;
  battery?: BatteryInfo;
};

export type ParseSummary = {
  device: DeviceInfo;
  events: number;
  anrs: number;
  crashes: number;
  efTotal: number;
  efRecent: number;
};

export type LogRow = {
  ts: string;
  tsIso?: string;
  level: string;
  tag: string;
  pid: number;
  tid: number;
  msg: string;
};

export type LogFilters = {
  tsFrom?: string;
  tsTo?: string;
  levels?: string[];
  tag?: string;
  pid?: number;
  tid?: number;
  text?: string;
  notText?: string;
  textMode?: "plain" | "regex";
  caseSensitive?: boolean;
};

export type LogStreamResp = {
  rows: LogRow[];
  nextCursor: number;
  exhausted: boolean;
  fileSize: number;
  totalRows?: number;
  minIsoMs?: number;
  maxIsoMs?: number;
};
