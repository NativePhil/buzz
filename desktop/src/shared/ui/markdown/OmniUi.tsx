import * as React from "react";
import {
  AlertTriangle,
  Check,
  ExternalLink,
  Info,
  Sparkles,
} from "lucide-react";

import {
  parseOmniUiBlock,
  type ActionItem,
  type ActionsRowProps,
  type AlertBannerProps,
  type BarChartProps,
  type ChecklistProps,
  type ContactCardProps,
  type ItemCardProps,
  type KpiGridProps,
  type MetricCardProps,
  type PollProps,
  type ProgressProps,
  type ProvenanceRef,
  type RivePanelProps,
  type StatusGridProps,
  type TableProps,
  type TimelineProps,
} from "./omniUiSchema";

export const CARD_CLASS =
  "my-2 w-full max-w-[420px] rounded-2xl border border-foreground/10 bg-background/60 p-4 text-left shadow-sm";
const PROVENANCE_CLASS =
  "mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 border-t border-foreground/10 pt-2 text-[11px] text-foreground/55";

export function ProvenanceFooter({ items }: { items?: ProvenanceRef[] }) {
  if (!items?.length) return null;
  return (
    <div className={PROVENANCE_CLASS}>
      <Sparkles className="h-3 w-3 shrink-0" aria-hidden="true" />
      <span>
        {items.map((p, i) => (
          <span
            key={i}
            className="after:content-[' · '] last:after:content-none"
          >
            {p.label}
            {p.ref ? (
              <span className="font-mono text-foreground/40">
                {" "}
                ({p.ref.slice(0, 12)}…)
              </span>
            ) : null}
          </span>
        ))}
      </span>
    </div>
  );
}

const STATUS_DOT: Record<string, string> = {
  online: "bg-lime-500",
  away: "bg-amber-400",
  offline: "bg-foreground/30",
};

function ContactCard({ p }: { p: ContactCardProps }) {
  const initial = p.name.trim().charAt(0).toUpperCase() || "?";
  return (
    <div className="flex items-center gap-3">
      <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-primary/15 text-base font-semibold text-primary">
        {initial}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-semibold text-foreground">
            {p.name}
          </span>
          {p.status ? (
            <span
              className={`h-2 w-2 shrink-0 rounded-full ${STATUS_DOT[p.status] ?? STATUS_DOT.offline}`}
              aria-label={p.status}
            />
          ) : null}
        </div>
        {p.title || p.org ? (
          <div className="truncate text-xs text-foreground/70">
            {[p.title, p.org].filter(Boolean).join(" · ")}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function ContactCardBody({ p }: { p: ContactCardProps }) {
  return (
    <>
      <ContactCard p={p} />
      {p.notes ? (
        <p className="mt-2 text-xs leading-5 text-foreground/80">{p.notes}</p>
      ) : null}
      {p.tags?.length ? (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {p.tags.slice(0, 8).map((t) => (
            <span
              key={t}
              className="rounded-full bg-foreground/8 px-2 py-0.5 text-[11px] text-foreground/75"
            >
              {t}
            </span>
          ))}
        </div>
      ) : null}
      {p.links?.length ? (
        <div className="mt-2 flex flex-wrap gap-2">
          {p.links.slice(0, 4).map((l) => (
            <a
              key={l.url}
              href={l.url}
              target="_blank"
              rel="noreferrer noopener"
              className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
            >
              {l.label}
              <ExternalLink className="h-3 w-3" aria-hidden="true" />
            </a>
          ))}
        </div>
      ) : null}
    </>
  );
}

function Sparkline({ values }: { values: number[] }) {
  const pts = values.slice(-24);
  if (pts.length < 2) return null;
  const min = Math.min(...pts);
  const max = Math.max(...pts);
  const span = max - min || 1;
  const path = pts
    .map(
      (v, i) =>
        `${(i / (pts.length - 1)) * 100},${28 - ((v - min) / span) * 24}`,
    )
    .join(" ");
  return (
    <svg
      viewBox="0 0 100 30"
      className="h-8 w-24"
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      <polyline
        points={path}
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="text-lime-500"
      />
    </svg>
  );
}

function MetricCardBody({ p }: { p: MetricCardProps }) {
  const positive = (p.delta?.value ?? 0) >= 0;
  return (
    <div className="flex items-center justify-between gap-3">
      <div className="min-w-0">
        <div className="text-xs font-medium uppercase tracking-wide text-foreground/60">
          {p.label}
        </div>
        <div className="mt-1 flex items-baseline gap-1.5">
          <span className="text-2xl font-semibold leading-none text-foreground">
            {p.value}
          </span>
          {p.unit ? (
            <span className="text-xs text-foreground/60">{p.unit}</span>
          ) : null}
          {p.delta ? (
            <span
              className={`text-xs font-medium ${positive ? "text-lime-600" : "text-red-500"}`}
            >
              {positive ? "▲" : "▼"} {Math.abs(p.delta.value)}
              {p.delta.label ? ` ${p.delta.label}` : ""}
            </span>
          ) : null}
        </div>
      </div>
      {p.sparkline?.length && p.sparkline.length > 1 ? (
        <Sparkline values={p.sparkline} />
      ) : null}
    </div>
  );
}

function useAgentPrompt() {
  return React.useCallback((text: string) => {
    window.dispatchEvent(
      new CustomEvent("omni-agent-prompt", { detail: { text } }),
    );
  }, []);
}

function ActionsRow({
  p,
  provenance,
}: {
  p: ActionsRowProps;
  provenance?: ProvenanceRef[];
}) {
  const dispatch = useAgentPrompt();
  const [copied, setCopied] = React.useState<string | null>(null);
  const run = (a: ActionItem) => {
    const text = a.prompt ?? a.label;
    dispatch(text);
    void writeTextToClipboardCompat(text).then(() => {
      setCopied(a.label);
      window.setTimeout(() => setCopied(null), 1600);
    });
  };
  return (
    <div className={CARD_CLASS}>
      <div className="flex flex-wrap gap-2">
        {p.actions.slice(0, 6).map((a) =>
          a.kind === "link" && a.url ? (
            <a
              key={a.label}
              href={a.url}
              target="_blank"
              rel="noreferrer noopener"
              className="inline-flex h-9 items-center gap-1.5 rounded-full border border-foreground/15 px-4 text-xs font-medium text-foreground hover:bg-foreground/5"
            >
              {a.label}
              <ExternalLink className="h-3 w-3" aria-hidden="true" />
            </a>
          ) : (
            <button
              key={a.label}
              className="inline-flex h-9 items-center gap-1.5 rounded-full bg-primary/12 px-4 text-xs font-medium text-primary hover:bg-primary/20"
              onClick={() => run(a)}
              type="button"
            >
              {copied === a.label ? (
                <Check className="h-3 w-3" aria-hidden="true" />
              ) : null}
              {a.label}
            </button>
          ),
        )}
      </div>
      {copied ? (
        <div className="mt-2 text-[11px] text-foreground/55">
          Prompt copied — paste it in the composer to run.
        </div>
      ) : null}
      <ProvenanceFooter items={provenance} />
    </div>
  );
}

async function writeTextToClipboardCompat(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // clipboard unavailable — the CustomEvent already fired
  }
}

function RivePanel({ p }: { p: RivePanelProps }) {
  return (
    <div>
      <div className="flex h-28 items-center justify-center rounded-xl border border-dashed border-foreground/20 bg-foreground/[0.03]">
        <div className="text-center">
          <div className="text-xs font-medium text-foreground/70">
            Rive panel
          </div>
          <div className="mt-1 font-mono text-[11px] text-foreground/50">
            {p.asset}
          </div>
        </div>
      </div>
      {p.bindings ? (
        <div className="mt-2 font-mono text-[11px] text-foreground/50">
          {Object.entries(p.bindings)
            .slice(0, 4)
            .map(([k, v]) => `${k}=${v}`)
            .join("  ")}
        </div>
      ) : null}
      {p.caption ? (
        <p className="mt-1 text-xs text-foreground/70">{p.caption}</p>
      ) : null}
    </div>
  );
}

const STATUS_COLOR: Record<string, string> = {
  ok: "bg-lime-500",
  degraded: "bg-amber-400",
  down: "bg-red-500",
};

function StatusGrid({ p }: { p: StatusGridProps }) {
  return (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {p.title}
        </div>
      ) : null}
      <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
        {p.services.map((s) => (
          <div
            key={s.name}
            className="flex items-center gap-2 rounded-lg bg-foreground/[0.04] px-3 py-2"
          >
            <span
              className={`h-2 w-2 shrink-0 rounded-full ${STATUS_COLOR[s.status]}`}
              aria-hidden="true"
            />
            <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
              {s.name}
            </span>
            {s.detail ? (
              <span className="truncate text-[11px] text-foreground/55">
                {s.detail}
              </span>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}

function Progress({ p }: { p: ProgressProps }) {
  const percent =
    p.percent ??
    (typeof p.step === "number" && typeof p.totalSteps === "number"
      ? Math.round((p.step / p.totalSteps) * 100)
      : 0);
  return (
    <div>
      <div className="flex items-center justify-between gap-3">
        <span className="text-xs font-medium text-foreground">{p.label}</span>
        <span className="text-xs text-foreground/60">
          {p.stepLabel ?? `${percent}%`}
        </span>
      </div>
      <div className="mt-2 h-2 overflow-hidden rounded-full bg-foreground/10">
        <div
          className="h-full rounded-full bg-lime-500 transition-[width] duration-500"
          style={{ width: `${percent}%` }}
        />
      </div>
      {p.detail ? (
        <p className="mt-2 text-xs text-foreground/70">{p.detail}</p>
      ) : null}
    </div>
  );
}

function Checklist({ p }: { p: ChecklistProps }) {
  const [done, setDone] = React.useState<Record<number, boolean>>(() =>
    Object.fromEntries((p.items ?? []).map((it, i) => [i, !!it.done])),
  );
  const completed = Object.values(done).filter(Boolean).length;
  return (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {p.title}
        </div>
      ) : null}
      <div className="space-y-1.5">
        {p.items.map((it, i) => (
          <button
            key={i}
            className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left hover:bg-foreground/[0.04]"
            onClick={() => setDone((d) => ({ ...d, [i]: !d[i] }))}
            type="button"
          >
            <span
              className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${done[i] ? "border-lime-500 bg-lime-500 text-white" : "border-foreground/30"}`}
            >
              {done[i] ? (
                <Check className="h-3 w-3" aria-hidden="true" />
              ) : null}
            </span>
            <span
              className={`text-xs ${done[i] ? "text-foreground/45 line-through" : "text-foreground"}`}
            >
              {it.text}
            </span>
          </button>
        ))}
      </div>
      <div className="mt-2 text-[11px] text-foreground/55">
        {p.summary ?? `${completed}/${p.items.length} done`}
      </div>
    </div>
  );
}

function DataTable({ p }: { p: TableProps }) {
  return (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {p.title}
        </div>
      ) : null}
      <div className="overflow-x-auto rounded-lg border border-foreground/10">
        <table className="w-full text-left text-xs">
          <thead className="bg-foreground/[0.05] text-foreground/70">
            <tr>
              {p.columns.map((c) => (
                <th key={c.key} className="px-3 py-2 font-medium">
                  {c.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {p.rows.slice(0, 24).map((row, i) => (
              <tr key={i} className="border-t border-foreground/[0.07]">
                {p.columns.map((c) => (
                  <td key={c.key} className="px-3 py-1.5 text-foreground/85">
                    {String(row[c.key] ?? "")}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function Timeline({ p }: { p: TimelineProps }) {
  return (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {p.title}
        </div>
      ) : null}
      <div className="relative space-y-3 pl-4">
        <span
          className="absolute bottom-1 left-[5px] top-1 w-px bg-foreground/15"
          aria-hidden="true"
        />
        {p.entries.map((e, i) => (
          <div key={i} className="relative">
            <span
              className="absolute -left-4 top-1.5 h-2 w-2 rounded-full bg-lime-500"
              aria-hidden="true"
            />
            <div className="flex items-baseline gap-2">
              <span className="text-xs font-medium text-foreground">
                {e.title}
              </span>
              {e.time ? (
                <span className="font-mono text-[11px] text-foreground/50">
                  {e.time}
                </span>
              ) : null}
            </div>
            {e.detail ? (
              <p className="mt-0.5 text-xs leading-5 text-foreground/70">
                {e.detail}
              </p>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}

function KpiGrid({ p }: { p: KpiGridProps }) {
  return (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
      {p.metrics.slice(0, 6).map((m) => (
        <div
          key={m.label}
          className="rounded-xl bg-foreground/[0.04] px-3 py-2.5"
        >
          <div className="truncate text-[11px] font-medium uppercase tracking-wide text-foreground/55">
            {m.label}
          </div>
          <div className="mt-1 flex items-baseline gap-1">
            <span className="text-lg font-semibold leading-none text-foreground">
              {m.value}
            </span>
            {m.unit ? (
              <span className="text-[11px] text-foreground/60">{m.unit}</span>
            ) : null}
          </div>
          {typeof m.delta === "number" ? (
            <div
              className={`text-[11px] font-medium ${m.delta >= 0 ? "text-lime-600" : "text-red-500"}`}
            >
              {m.delta >= 0 ? "▲" : "▼"} {Math.abs(m.delta)}
            </div>
          ) : null}
        </div>
      ))}
    </div>
  );
}

function BarChart({ p }: { p: BarChartProps }) {
  const max = Math.max(...p.bars.map((b) => b.value), 1);
  return (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {p.title}
        </div>
      ) : null}
      <div className="space-y-1.5">
        {p.bars.map((b) => (
          <div key={b.label} className="flex items-center gap-2">
            <span className="w-24 shrink-0 truncate text-[11px] text-foreground/70">
              {b.label}
            </span>
            <div className="h-4 flex-1 overflow-hidden rounded bg-foreground/[0.06]">
              <div
                className="h-full rounded bg-lime-500/80 transition-[width] duration-500"
                style={{
                  width: `${Math.max(2, Math.round((b.value / max) * 100))}%`,
                }}
              />
            </div>
            <span className="w-14 shrink-0 text-right font-mono text-[11px] text-foreground/60">
              {b.value}
              {p.unit ?? ""}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

const ALERT_STYLE: Record<string, { wrap: string; icon: typeof Info }> = {
  info: { wrap: "border-sky-400/40 bg-sky-400/5", icon: Info },
  warn: { wrap: "border-amber-400/40 bg-amber-400/5", icon: AlertTriangle },
  error: { wrap: "border-red-400/40 bg-red-400/5", icon: AlertTriangle },
  success: { wrap: "border-lime-400/40 bg-lime-400/5", icon: Check },
};

function AlertBanner({ p }: { p: AlertBannerProps }) {
  const style = ALERT_STYLE[p.level] ?? ALERT_STYLE.info;
  const Icon = style.icon;
  return (
    <div>
      <div
        className={`flex items-start gap-2.5 rounded-xl border p-3 ${style.wrap}`}
      >
        <Icon className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
        <div className="min-w-0">
          {p.title ? (
            <div className="text-xs font-semibold text-foreground">
              {p.title}
            </div>
          ) : null}
          <p className="mt-0.5 text-xs leading-5 text-foreground/80">
            {p.message}
          </p>
        </div>
      </div>
    </div>
  );
}

function Poll({ p }: { p: PollProps }) {
  const [voted, setVoted] = React.useState<number | null>(null);
  const total = p.total ?? p.options.reduce((s, o) => s + (o.votes ?? 0), 0);
  return (
    <div>
      <div className="text-xs font-medium text-foreground">{p.question}</div>
      <div className="mt-2 space-y-1.5">
        {p.options.map((o, i) => {
          const pct =
            total > 0 ? Math.round(((o.votes ?? 0) / total) * 100) : 0;
          const mine = voted === i;
          return (
            <button
              key={i}
              className={`relative w-full overflow-hidden rounded-lg border px-3 py-2 text-left text-xs ${mine ? "border-lime-500/60" : "border-foreground/12 hover:bg-foreground/[0.04]"}`}
              onClick={() => setVoted(i)}
              type="button"
            >
              {voted !== null ? (
                <span
                  className="absolute inset-y-0 left-0 bg-lime-500/15 transition-[width] duration-500"
                  style={{ width: `${pct}%` }}
                  aria-hidden="true"
                />
              ) : null}
              <span className="relative flex items-center justify-between gap-2">
                <span className="text-foreground">{o.label}</span>
                {voted !== null ? (
                  <span className="font-mono text-[11px] text-foreground/60">
                    {pct}%
                  </span>
                ) : null}
              </span>
            </button>
          );
        })}
      </div>
      {voted !== null ? (
        <div className="mt-1.5 text-[11px] text-foreground/55">
          Your vote: {p.options[voted].label} (local preview)
        </div>
      ) : null}
    </div>
  );
}

function ItemCard({ p }: { p: ItemCardProps }) {
  return (
    <div>
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-foreground">
            {p.title}
          </div>
          {p.subtitle ? (
            <div className="truncate text-xs text-foreground/70">
              {p.subtitle}
            </div>
          ) : null}
        </div>
        {p.status ? (
          <span className="shrink-0 rounded-full bg-primary/12 px-2 py-0.5 text-[11px] font-medium text-primary">
            {p.status}
          </span>
        ) : null}
      </div>
      {p.fields?.length ? (
        <div className="mt-2 space-y-1">
          {p.fields.slice(0, 6).map((f) => (
            <div key={f.label} className="flex gap-2 text-xs">
              <span className="w-28 shrink-0 text-foreground/55">
                {f.label}
              </span>
              <span className="min-w-0 flex-1 text-foreground/85">
                {f.value}
              </span>
            </div>
          ))}
        </div>
      ) : null}
      {p.notes ? (
        <p className="mt-2 text-xs leading-5 text-foreground/80">{p.notes}</p>
      ) : null}
      {p.tags?.length ? (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {p.tags.slice(0, 8).map((t) => (
            <span
              key={t}
              className="rounded-full bg-foreground/8 px-2 py-0.5 text-[11px] text-foreground/75"
            >
              {t}
            </span>
          ))}
        </div>
      ) : null}
      {p.links?.length ? (
        <div className="mt-2 flex flex-wrap gap-2">
          {p.links.slice(0, 4).map((l) => (
            <a
              key={l.url}
              href={l.url}
              target="_blank"
              rel="noreferrer noopener"
              className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
            >
              {l.label}
              <ExternalLink className="h-3 w-3" aria-hidden="true" />
            </a>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function OmniUiBlockView({ code }: { code: string }) {
  const block = React.useMemo(() => parseOmniUiBlock(code), [code]);
  if (!block) {
    return (
      <div className="my-2 rounded-xl border border-amber-400/40 bg-amber-400/5 p-3 text-xs text-foreground/75">
        Invalid omni-ui block — showing source.
        <pre className="mt-1 overflow-x-auto font-mono text-[11px] text-foreground/60">
          {code}
        </pre>
      </div>
    );
  }
  const prov = block.provenance;
  switch (block.component) {
    case "contact-card":
      return (
        <div className={CARD_CLASS}>
          <ContactCardBody p={block.props as ContactCardProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "metric-card":
      return (
        <div className={CARD_CLASS}>
          <MetricCardBody p={block.props as MetricCardProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "actions-row":
      return (
        <ActionsRow p={block.props as ActionsRowProps} provenance={prov} />
      );
    case "rive":
      return (
        <div className={CARD_CLASS}>
          <RivePanel p={block.props as RivePanelProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "status-grid":
      return (
        <div className={CARD_CLASS}>
          <StatusGrid p={block.props as StatusGridProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "progress":
      return (
        <div className={CARD_CLASS}>
          <Progress p={block.props as ProgressProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "checklist":
      return (
        <div className={CARD_CLASS}>
          <Checklist p={block.props as ChecklistProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "table":
      return (
        <div className={CARD_CLASS}>
          <DataTable p={block.props as TableProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "timeline":
      return (
        <div className={CARD_CLASS}>
          <Timeline p={block.props as TimelineProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "kpi-grid":
      return (
        <div className={CARD_CLASS}>
          <KpiGrid p={block.props as KpiGridProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "bar-chart":
      return (
        <div className={CARD_CLASS}>
          <BarChart p={block.props as BarChartProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "alert":
      return (
        <div className={CARD_CLASS}>
          <AlertBanner p={block.props as AlertBannerProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "poll":
      return (
        <div className={CARD_CLASS}>
          <Poll p={block.props as PollProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
    case "item-card":
      return (
        <div className={CARD_CLASS}>
          <ItemCard p={block.props as ItemCardProps} />
          <ProvenanceFooter items={prov} />
        </div>
      );
  }
}
