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

export type LogLevel = "V" | "D" | "I" | "W" | "E" | "F";

export type LogRow = {
  ts: string;
  tsIso?: string;
  level: LogLevel;
  tag: string;
  pid: number;
  tid: number;
  msg: string;
};

export type LogFilters = {
  tsFrom?: string;
  tsTo?: string;
  levels?: LogLevel[];
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

// V2 API Types

export type CursorDirection = "forward" | "backward";

export type QueryCursor = {
  position: number;
  direction: CursorDirection;
  filterHash: number;
};

export type QueryResponse = {
  rows: LogRow[];
  nextCursor: QueryCursor | null;
  prevCursor: QueryCursor | null;
  hasMoreNext: boolean;
  hasMorePrev: boolean;
  estimatedTotal?: number;
  positionRatio: number;
};

export type LevelCounts = {
  verbose: number;
  debug: number;
  info: number;
  warning: number;
  error: number;
  fatal: number;
};

export type LogcatStats = {
  totalRows: number;
  filteredRows?: number;
  minTimestampMs?: number;
  maxTimestampMs?: number;
  minTsDisplay?: string;
  maxTsDisplay?: string;
  levelCounts: LevelCounts;
};
