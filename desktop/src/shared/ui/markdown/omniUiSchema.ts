/**
 * Omni generative UI — parser/validator for agent-emitted blocks.
 *
 * Pure logic (no React): a fenced ```omni-ui block is parsed and validated
 * against a strict, allowlisted catalog. Extra/unknown props are FILTERED (the
 * sanitizer pattern — agents may add fields without breaking rendering, and
 * nothing unknown reaches the renderer). Unknown components, wrong versions,
 * and missing required props are REJECTED (caller shows the source).
 */

export const OMNI_UI_LANGUAGE = "omni-ui";
export const OMNI_UI_VERSION = 1;
export const MAX_ITEMS = 24;
export const MAX_STR = 240;

export type ProvenanceRef = { label: string; source: string; ref?: string };

export type ContactCardProps = {
  name: string;
  title?: string;
  org?: string;
  pubkey?: string;
  status?: "online" | "away" | "offline";
  tags?: string[];
  notes?: string;
  links?: { label: string; url: string }[];
};

export type MetricCardProps = {
  label: string;
  value: string | number;
  unit?: string;
  delta?: { value: number; label?: string };
  sparkline?: number[];
  source?: string;
};

export type ActionItem = {
  label: string;
  kind?: "agent_prompt" | "link";
  prompt?: string;
  url?: string;
};

export type ActionsRowProps = { actions: ActionItem[] };

export type RivePanelProps = {
  asset: string;
  stateMachine?: string;
  bindings?: Record<string, string | number | boolean>;
  caption?: string;
};

export type StatusEntry = { name: string; status: "ok" | "degraded" | "down"; detail?: string };
export type StatusGridProps = { title?: string; services: StatusEntry[] };

export type ProgressProps = {
  label: string;
  percent?: number;
  step?: number;
  totalSteps?: number;
  stepLabel?: string;
  detail?: string;
};

export type ChecklistEntry = { text: string; done?: boolean };
export type ChecklistProps = { title?: string; items: ChecklistEntry[]; summary?: string };

export type TableProps = {
  title?: string;
  columns: { key: string; label: string }[];
  rows: Record<string, string | number>[];
};

export type TimelineEntry = { time?: string; title: string; detail?: string };
export type TimelineProps = { title?: string; entries: TimelineEntry[] };

export type KpiGridProps = { metrics: { label: string; value: string | number; unit?: string; delta?: number }[] };

export type BarChartProps = {
  title?: string;
  bars: { label: string; value: number }[];
  unit?: string;
};

export type AlertBannerProps = {
  level: "info" | "warn" | "error" | "success";
  title?: string;
  message: string;
};

export type PollProps = { question: string; options: { label: string; votes?: number }[]; total?: number };

export type ItemCardProps = {
  title: string;
  subtitle?: string;
  status?: string;
  fields?: { label: string; value: string }[];
  tags?: string[];
  notes?: string;
  links?: { label: string; url: string }[];
};

export type OmniUiProps =
  | ContactCardProps
  | MetricCardProps
  | ActionsRowProps
  | RivePanelProps
  | StatusGridProps
  | ProgressProps
  | ChecklistProps
  | TableProps
  | TimelineProps
  | KpiGridProps
  | BarChartProps
  | AlertBannerProps
  | PollProps
  | ItemCardProps;

export type OmniUiComponent =
  | "contact-card"
  | "metric-card"
  | "actions-row"
  | "rive"
  | "status-grid"
  | "progress"
  | "checklist"
  | "table"
  | "timeline"
  | "kpi-grid"
  | "bar-chart"
  | "alert"
  | "poll"
  | "item-card";

export type OmniUiBlock = {
  v: number;
  component: OmniUiComponent;
  props: OmniUiProps;
  provenance?: ProvenanceRef[];
};

export const OMNI_UI_COMPONENTS: readonly OmniUiComponent[] = [
  "contact-card",
  "metric-card",
  "actions-row",
  "rive",
  "status-grid",
  "progress",
  "checklist",
  "table",
  "timeline",
  "kpi-grid",
  "bar-chart",
  "alert",
  "poll",
  "item-card",
];

function isStr(v: unknown, max = MAX_STR): v is string {
  return typeof v === "string" && v.length > 0 && v.length <= max;
}

function optStr<T extends Record<string, unknown>>(obj: T, key: string, max = MAX_STR): void {
  const v = obj[key];
  if (v !== undefined && !isStr(v, max)) delete obj[key];
}

function isStrArray(v: unknown, max = MAX_ITEMS): v is string[] {
  return Array.isArray(v) && v.length > 0 && v.length <= max && v.every((x) => isStr(x, 120));
}

function isNum(v: unknown): v is number {
  return typeof v === "number" && Number.isFinite(v);
}

function sanitizeLinks(input: unknown): { label: string; url: string }[] | undefined {
  if (!Array.isArray(input)) return undefined;
  const out: { label: string; url: string }[] = [];
  for (const item of input.slice(0, 4)) {
    if (!item || typeof item !== "object") continue;
    const label = (item as { label?: unknown }).label;
    const url = (item as { url?: unknown }).url;
    if (isStr(label, 80) && isStr(url, 300) && /^https?:\/\//i.test(url)) {
      out.push({ label, url });
    }
  }
  return out.length > 0 ? out : undefined;
}

function sanitizeProvenance(input: unknown): ProvenanceRef[] | undefined {
  if (input === undefined) return undefined;
  if (!Array.isArray(input)) return undefined;
  const out: ProvenanceRef[] = [];
  for (const item of input.slice(0, 6)) {
    if (!item || typeof item !== "object" || Array.isArray(item)) continue;
    const label = (item as ProvenanceRef).label;
    const source = (item as ProvenanceRef).source;
    if (isStr(label, 120) && isStr(source, 120)) {
      const ref = (item as ProvenanceRef).ref;
      out.push({ label, source, ref: isStr(ref, 200) ? ref : undefined });
    }
  }
  return out.length > 0 ? out : undefined;
}

function sanitizeTags(input: unknown): string[] | undefined {
  return isStrArray(input, 8) ? input.slice(0, 8) : undefined;
}

const CONTACT_STATUS = new Set(["online", "away", "offline"]);

function contactProps(p: Record<string, unknown>): ContactCardProps | null {
  if (!isStr(p.name)) return null;
  const out: Record<string, unknown> = { name: p.name };
  optStr(p, "title");
  optStr(p, "org");
  optStr(p, "pubkey", 80);
  optStr(p, "notes", 600);
  if (isStr(p.title)) out.title = p.title;
  if (isStr(p.org)) out.org = p.org;
  if (isStr(p.pubkey, 80)) out.pubkey = p.pubkey;
  if (isStr(p.notes, 600)) out.notes = p.notes;
  if (isStr(p.status) && CONTACT_STATUS.has(p.status)) out.status = p.status;
  const tags = sanitizeTags(p.tags);
  if (tags) out.tags = tags;
  const links = sanitizeLinks(p.links);
  if (links) out.links = links;
  return out as unknown as ContactCardProps;
}

function metricProps(p: Record<string, unknown>): MetricCardProps | null {
  if (!isStr(p.label) || !("value" in p) || !(typeof p.value === "string" || isNum(p.value))) return null;
  const out: Record<string, unknown> = { label: p.label, value: p.value };
  if (isStr(p.unit, 40)) out.unit = p.unit;
  if (isStr(p.source, 120)) out.source = p.source;
  if (p.delta && typeof p.delta === "object" && isNum((p.delta as { value?: unknown }).value)) {
    const d = p.delta as { value: number; label?: unknown };
    out.delta = { value: d.value, label: isStr(d.label, 60) ? d.label : undefined };
  }
  if (Array.isArray(p.sparkline) && p.sparkline.length > 1 && p.sparkline.length <= 60 &&
    p.sparkline.every(isNum)) {
    out.sparkline = p.sparkline;
  }
  return out as unknown as MetricCardProps;
}

function actionsProps(p: Record<string, unknown>): ActionsRowProps | null {
  if (!Array.isArray(p.actions) || p.actions.length === 0 || p.actions.length > 6) return null;
  const actions: ActionItem[] = [];
  for (const raw of p.actions) {
    if (!raw || typeof raw !== "object") return null;
    const a = raw as Record<string, unknown>;
    if (!isStr(a.label, 80)) return null;
    const kind = a.kind === "link" ? "link" : a.kind === "agent_prompt" ? "agent_prompt" : undefined;
    const item: Record<string, unknown> = { label: a.label };
    if (kind) item.kind = kind;
    if (isStr(a.prompt, 600)) item.prompt = a.prompt;
    if (isStr(a.url, 300) && /^https?:\/\//i.test(a.url)) item.url = a.url;
    if (item.kind === "link" && !item.url) return null;
    actions.push(item as unknown as ActionItem);
  }
  return { actions };
}

function riveProps(p: Record<string, unknown>): RivePanelProps | null {
  if (!isStr(p.asset, 300)) return null;
  const out: Record<string, unknown> = { asset: p.asset };
  if (isStr(p.stateMachine, 120)) out.stateMachine = p.stateMachine;
  if (p.caption && isStr(p.caption, 300)) out.caption = p.caption;
  if (p.bindings && typeof p.bindings === "object" && !Array.isArray(p.bindings)) {
    const clean: Record<string, string | number | boolean> = {};
    for (const [k, v] of Object.entries(p.bindings as Record<string, unknown>).slice(0, 16)) {
      if (isStr(k, 60) && (isStr(v, 200) || isNum(v) || typeof v === "boolean")) {
        clean[k] = v as string | number | boolean;
      }
    }
    if (Object.keys(clean).length > 0) out.bindings = clean;
  }
  return out as unknown as RivePanelProps;
}

function statusGridProps(p: Record<string, unknown>): StatusGridProps | null {
  if (!Array.isArray(p.services) || p.services.length === 0 || p.services.length > 12) return null;
  const services: StatusEntry[] = [];
  for (const raw of p.services) {
    if (!raw || typeof raw !== "object") return null;
    const s = raw as Record<string, unknown>;
    if (!isStr(s.name, 80) || !isStr(s.status) || !["ok", "degraded", "down"].includes(s.status)) return null;
    const entry: Record<string, unknown> = { name: s.name, status: s.status };
    if (isStr(s.detail, 160)) entry.detail = s.detail;
    services.push(entry as unknown as StatusEntry);
  }
  const out: Record<string, unknown> = { services };
  if (isStr(p.title, 120)) out.title = p.title;
  return out as unknown as StatusGridProps;
}

function progressProps(p: Record<string, unknown>): ProgressProps | null {
  if (!isStr(p.label)) return null;
  const hasPercent = isNum(p.percent) && p.percent >= 0 && p.percent <= 100;
  const hasSteps = isNum(p.step) && isNum(p.totalSteps) && (p.totalSteps as number) > 0;
  if (!hasPercent && !hasSteps) return null;
  const out: Record<string, unknown> = { label: p.label };
  if (hasPercent) out.percent = p.percent;
  if (hasSteps) {
    out.step = p.step;
    out.totalSteps = p.totalSteps;
  }
  if (isStr(p.stepLabel, 80)) out.stepLabel = p.stepLabel;
  if (isStr(p.detail, 300)) out.detail = p.detail;
  return out as unknown as ProgressProps;
}

function checklistProps(p: Record<string, unknown>): ChecklistProps | null {
  if (!Array.isArray(p.items) || p.items.length === 0 || p.items.length > 12) return null;
  const items: ChecklistEntry[] = [];
  for (const raw of p.items) {
    if (!raw || typeof raw !== "object") return null;
    const it = raw as Record<string, unknown>;
    if (!isStr(it.text, 200)) return null;
    items.push({ text: it.text, done: it.done === true });
  }
  const out: Record<string, unknown> = { items };
  if (isStr(p.title, 120)) out.title = p.title;
  if (isStr(p.summary, 120)) out.summary = p.summary;
  return out as unknown as ChecklistProps;
}

function tableProps(p: Record<string, unknown>): TableProps | null {
  if (!Array.isArray(p.columns) || p.columns.length === 0 || p.columns.length > 6) return null;
  const columns: { key: string; label: string }[] = [];
  for (const raw of p.columns) {
    if (!raw || typeof raw !== "object") return null;
    const c = raw as Record<string, unknown>;
    if (!isStr(c.key, 40) || !isStr(c.label, 60)) return null;
    columns.push({ key: c.key, label: c.label });
  }
  if (!Array.isArray(p.rows) || p.rows.length > MAX_ITEMS) return null;
  const rows: Record<string, string | number>[] = [];
  for (const raw of p.rows.slice(0, MAX_ITEMS)) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
    const clean: Record<string, string | number> = {};
    for (const col of columns) {
      const v = (raw as Record<string, unknown>)[col.key];
      if (isStr(v) || isNum(v)) clean[col.key] = v as string | number;
    }
    rows.push(clean);
  }
  const out: Record<string, unknown> = { columns, rows };
  if (isStr(p.title, 120)) out.title = p.title;
  return out as unknown as TableProps;
}

function timelineProps(p: Record<string, unknown>): TimelineProps | null {
  if (!Array.isArray(p.entries) || p.entries.length === 0 || p.entries.length > 12) return null;
  const entries: TimelineEntry[] = [];
  for (const raw of p.entries) {
    if (!raw || typeof raw !== "object") return null;
    const e = raw as Record<string, unknown>;
    if (!isStr(e.title, 160)) return null;
    const entry: Record<string, unknown> = { title: e.title };
    if (isStr(e.time, 60)) entry.time = e.time;
    if (isStr(e.detail, 300)) entry.detail = e.detail;
    entries.push(entry as unknown as TimelineEntry);
  }
  const out: Record<string, unknown> = { entries };
  if (isStr(p.title, 120)) out.title = p.title;
  return out as unknown as TimelineProps;
}

function kpiGridProps(p: Record<string, unknown>): KpiGridProps | null {
  if (!Array.isArray(p.metrics) || p.metrics.length === 0 || p.metrics.length > 6) return null;
  const metrics: { label: string; value: string | number; unit?: string; delta?: number }[] = [];
  for (const raw of p.metrics) {
    if (!raw || typeof raw !== "object") return null;
    const m = raw as Record<string, unknown>;
    if (!isStr(m.label) || !(typeof m.value === "string" || isNum(m.value))) return null;
    const out: Record<string, unknown> = { label: m.label, value: m.value };
    if (isStr(m.unit, 40)) out.unit = m.unit;
    if (isNum(m.delta)) out.delta = m.delta;
    metrics.push(out as { label: string; value: string | number; unit?: string; delta?: number });
  }
  return { metrics };
}

function barChartProps(p: Record<string, unknown>): BarChartProps | null {
  if (!Array.isArray(p.bars) || p.bars.length < 2 || p.bars.length > 12) return null;
  const bars: { label: string; value: number }[] = [];
  for (const raw of p.bars) {
    if (!raw || typeof raw !== "object") return null;
    const b = raw as Record<string, unknown>;
    if (!isStr(b.label, 60) || !isNum(b.value)) return null;
    bars.push({ label: b.label, value: b.value });
  }
  const out: Record<string, unknown> = { bars };
  if (isStr(p.title, 120)) out.title = p.title;
  if (isStr(p.unit, 20)) out.unit = p.unit;
  return out as unknown as BarChartProps;
}

const ALERT_LEVELS = new Set(["info", "warn", "error", "success"]);

function alertProps(p: Record<string, unknown>): AlertBannerProps | null {
  if (!isStr(p.level) || !ALERT_LEVELS.has(p.level) || !isStr(p.message, 400)) return null;
  const out: Record<string, unknown> = { level: p.level, message: p.message };
  if (isStr(p.title, 120)) out.title = p.title;
  return out as unknown as AlertBannerProps;
}

function pollProps(p: Record<string, unknown>): PollProps | null {
  if (!isStr(p.question) || !Array.isArray(p.options) || p.options.length < 2 || p.options.length > 8) return null;
  const options: { label: string; votes?: number }[] = [];
  for (const raw of p.options) {
    if (!raw || typeof raw !== "object") return null;
    const o = raw as Record<string, unknown>;
    if (!isStr(o.label, 120)) return null;
    const opt: Record<string, unknown> = { label: o.label };
    if (isNum(o.votes) && (o.votes as number) >= 0) opt.votes = o.votes;
    options.push(opt as { label: string; votes?: number });
  }
  const out: Record<string, unknown> = { question: p.question, options };
  if (isNum(p.total) && (p.total as number) >= 0) out.total = p.total;
  return out as unknown as PollProps;
}

function itemCardProps(p: Record<string, unknown>): ItemCardProps | null {
  if (!isStr(p.title)) return null;
  const out: Record<string, unknown> = { title: p.title };
  if (isStr(p.subtitle)) out.subtitle = p.subtitle;
  if (isStr(p.status, 60)) out.status = p.status;
  if (isStr(p.notes, 600)) out.notes = p.notes;
  if (isStr(p.pubkey, 80)) out.pubkey = p.pubkey;
  const tags = sanitizeTags(p.tags);
  if (tags) out.tags = tags;
  const links = sanitizeLinks(p.links);
  if (links) out.links = links;
  if (Array.isArray(p.fields) && p.fields.length > 0 && p.fields.length <= 8) {
    const fields: { label: string; value: string }[] = [];
    for (const raw of p.fields) {
      if (!raw || typeof raw !== "object") continue;
      const f = raw as Record<string, unknown>;
      if (isStr(f.label, 60) && (isStr(f.value, 200) || isNum(f.value))) {
        fields.push({ label: f.label, value: String(f.value) });
      }
    }
    if (fields.length > 0) out.fields = fields;
  }
  return out as unknown as ItemCardProps;
}

const VALIDATORS: Record<OmniUiComponent, (p: Record<string, unknown>) => OmniUiProps | null> = {
  "contact-card": contactProps,
  "metric-card": metricProps,
  "actions-row": actionsProps,
  rive: riveProps,
  "status-grid": statusGridProps,
  progress: progressProps,
  checklist: checklistProps,
  table: tableProps,
  timeline: timelineProps,
  "kpi-grid": kpiGridProps,
  "bar-chart": barChartProps,
  alert: alertProps,
  poll: pollProps,
  "item-card": itemCardProps,
};

export function parseOmniUiBlock(code: string): OmniUiBlock | null {
  let raw: unknown;
  try {
    raw = JSON.parse(code);
  } catch {
    return null;
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const obj = raw as Record<string, unknown>;
  const v = obj.v;
  const component = obj.component;
  const props = obj.props;
  if (v !== OMNI_UI_VERSION || typeof component !== "string") return null;
  if (!props || typeof props !== "object" || Array.isArray(props)) return null;
  const validate = VALIDATORS[component as OmniUiComponent];
  if (!validate) return null;
  const clean = validate(props as Record<string, unknown>);
  if (!clean) return null;
  const block: OmniUiBlock = { v: OMNI_UI_VERSION, component: component as OmniUiComponent, props: clean };
  const provenance = sanitizeProvenance(obj.provenance);
  if (provenance) block.provenance = provenance;
  return block;
}
