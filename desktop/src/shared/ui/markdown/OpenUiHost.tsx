import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check } from "lucide-react";

import type { ProvenanceRef } from "./omniUiSchema";
import { CARD_CLASS, ProvenanceFooter } from "./OmniUi";

/**
 * OpenUI Lang host: evaluates an ```omni-ui fence through the Tauri backend
 * (the openui crate, bounded + deterministic) and renders the evaluated tree
 * from the Omni component catalog. Legacy JSON blocks (the v1 catalog) parse
 * client-side and render directly — one fence, two accepted formats.
 */

type EvalResult = {
  ok: boolean;
  root?: TreeNode;
  diagnostics: { code: string; severity: string; message: string }[];
};

type TreeNode = { component: string; props: Record<string, unknown> };

function ProvenanceFromNode({ node }: { node: TreeNode }) {
  const items = (node.props?.provenance ?? null) as ProvenanceRef[] | null;
  return <ProvenanceFooter items={items ?? undefined} />;
}

function TextNode({ p }: { p: Record<string, unknown> }) {
  return (
    <span className="text-sm leading-6 text-foreground">
      {String(p.text ?? "")}
    </span>
  );
}

function ContactCardNode({
  p,
  children,
}: {
  p: Record<string, unknown>;
  children?: React.ReactNode;
}) {
  const status = String(p.status ?? "");
  const initial =
    String(p.name ?? "?")
      .trim()
      .charAt(0)
      .toUpperCase() || "?";
  return (
    <div className={CARD_CLASS}>
      <div className="flex items-center gap-3">
        <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-primary/15 text-base font-semibold text-primary">
          {initial}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-semibold text-foreground">
              {String(p.name ?? "")}
            </span>
            {status ? (
              <span
                className={`h-2 w-2 shrink-0 rounded-full ${status === "online" ? "bg-lime-500" : status === "away" ? "bg-amber-400" : "bg-foreground/30"}`}
                aria-label={status}
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
      {p.notes ? (
        <p className="mt-2 text-xs leading-5 text-foreground/80">
          {String(p.notes)}
        </p>
      ) : null}
      {Array.isArray(p.tags) && p.tags.length ? (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {p.tags.slice(0, 8).map((t, i) => (
            <span
              key={i}
              className="rounded-full bg-foreground/8 px-2 py-0.5 text-[11px] text-foreground/75"
            >
              {String(t)}
            </span>
          ))}
        </div>
      ) : null}
      {children}
    </div>
  );
}

function MetricCardNode({ p }: { p: Record<string, unknown> }) {
  const delta = p.delta as string | undefined;
  return (
    <div className="flex items-center justify-between gap-3">
      <div className="min-w-0">
        <div className="text-xs font-medium uppercase tracking-wide text-foreground/60">
          {String(p.label ?? "")}
        </div>
        <div className="mt-1 flex items-baseline gap-1.5">
          <span className="text-2xl font-semibold leading-none text-foreground">
            {String(p.value ?? "")}
          </span>
          {p.unit ? (
            <span className="text-xs text-foreground/60">{String(p.unit)}</span>
          ) : null}
          {delta ? (
            <span className="text-xs font-medium text-foreground/70">
              {delta}
            </span>
          ) : null}
        </div>
      </div>
      {Array.isArray(p.sparkline) && p.sparkline.length > 1 ? (
        <Sparkline values={p.sparkline.map(Number)} />
      ) : null}
    </div>
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

function ActionNode({ p }: { p: Record<string, unknown> }) {
  const dispatch = React.useCallback((text: string) => {
    window.dispatchEvent(
      new CustomEvent("omni-agent-prompt", { detail: { text } }),
    );
    void navigator.clipboard.writeText(text).catch(() => {});
  }, []);
  return (
    <button
      className="inline-flex h-9 items-center gap-1.5 rounded-full bg-primary/12 px-4 text-xs font-medium text-primary hover:bg-primary/20"
      onClick={() => dispatch(String(p.prompt ?? p.label ?? ""))}
      type="button"
    >
      {String(p.label ?? "")}
    </button>
  );
}

function RiveNode({ p }: { p: Record<string, unknown> }) {
  return (
    <div>
      <div className="flex h-28 items-center justify-center rounded-xl border border-dashed border-foreground/20 bg-foreground/[0.03]">
        <div className="text-center">
          <div className="text-xs font-medium text-foreground/70">
            Rive panel
          </div>
          <div className="mt-1 font-mono text-[11px] text-foreground/50">
            {String(p.asset ?? "")}
          </div>
        </div>
      </div>
      {p.caption ? (
        <p className="mt-1 text-xs text-foreground/70">{String(p.caption)}</p>
      ) : null}
    </div>
  );
}

const MAP: Record<
  string,
  (args: { p: Record<string, unknown>; node: TreeNode }) => React.ReactNode
> = {
  Text: ({ p }) => <TextNode p={p} />,
  ContactCard: ({ p, node }) => {
    const kids = node.props.children;
    return (
      <ContactCardNode p={p}>
        {Array.isArray(kids)
          ? kids.map((k, i) => <RenderNode key={i} node={k as TreeNode} />)
          : null}
      </ContactCardNode>
    );
  },
  MetricCard: ({ p }) => <MetricCardNode p={p} />,
  Action: ({ p }) => <ActionNode p={p} />,
  Actions: ({ p }) => (
    <div className="flex flex-wrap gap-2">
      {Array.isArray(p.actions)
        ? p.actions.map((k, i) => <RenderNode key={i} node={k as TreeNode} />)
        : null}
    </div>
  ),
  StatusGrid: ({ p }) => (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {String(p.title)}
        </div>
      ) : null}
      <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
        {Array.isArray(p.services)
          ? p.services.map((k, i) => (
              <RenderNode key={i} node={k as TreeNode} />
            ))
          : null}
      </div>
    </div>
  ),
  Status: ({ p }) => (
    <div className="flex items-center gap-2 rounded-lg bg-foreground/[0.04] px-3 py-2">
      <span
        className={`h-2 w-2 shrink-0 rounded-full ${p.status === "ok" ? "bg-lime-500" : p.status === "degraded" ? "bg-amber-400" : "bg-red-500"}`}
        aria-hidden="true"
      />
      <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
        {String(p.name ?? "")}
      </span>
      {p.detail ? (
        <span className="truncate text-[11px] text-foreground/55">
          {String(p.detail)}
        </span>
      ) : null}
    </div>
  ),
  Progress: ({ p }) => {
    const percent = Math.max(0, Math.min(100, Number(p.percent ?? 0)));
    return (
      <div>
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs font-medium text-foreground">
            {String(p.label ?? "")}
          </span>
          <span className="text-xs text-foreground/60">
            {Math.round(percent)}%
          </span>
        </div>
        <div className="mt-2 h-2 overflow-hidden rounded-full bg-foreground/10">
          <div
            className="h-full rounded-full bg-lime-500 transition-[width] duration-500"
            style={{ width: `${percent}%` }}
          />
        </div>
        {p.detail ? (
          <p className="mt-2 text-xs text-foreground/70">{String(p.detail)}</p>
        ) : null}
      </div>
    );
  },
  Checklist: ({ p }) => (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {String(p.title)}
        </div>
      ) : null}
      <div className="space-y-1.5">
        {Array.isArray(p.items)
          ? p.items.map((k, i) => <RenderNode key={i} node={k as TreeNode} />)
          : null}
      </div>
    </div>
  ),
  CheckItem: ({ p }) => {
    const done = p.done === true;
    return (
      <div className="flex items-center gap-2.5 rounded-lg px-2 py-1.5">
        <span
          className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${done ? "border-lime-500 bg-lime-500 text-white" : "border-foreground/30"}`}
        >
          {done ? <Check className="h-3 w-3" aria-hidden="true" /> : null}
        </span>
        <span
          className={`text-xs ${done ? "text-foreground/45 line-through" : "text-foreground"}`}
        >
          {String(p.text ?? "")}
        </span>
      </div>
    );
  },
  Table: ({ p }) => (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {String(p.title)}
        </div>
      ) : null}
      <div className="overflow-x-auto rounded-lg border border-foreground/10">
        <table className="w-full text-left text-xs">
          <thead className="bg-foreground/[0.05] text-foreground/70">
            <tr>
              {Array.isArray(p.columns)
                ? p.columns.map((k, i) => (
                    <RenderNode key={i} node={k as TreeNode} />
                  ))
                : null}
            </tr>
          </thead>
          <tbody>
            {Array.isArray(p.rows)
              ? p.rows.map((k, i) => (
                  <RenderNode key={i} node={k as TreeNode} />
                ))
              : null}
          </tbody>
        </table>
      </div>
    </div>
  ),
  Column: () => null,
  Row: () => null,
  Timeline: ({ p }) => (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {String(p.title)}
        </div>
      ) : null}
      <div className="relative space-y-3 pl-4">
        <span
          className="absolute bottom-1 left-[5px] top-1 w-px bg-foreground/15"
          aria-hidden="true"
        />
        {Array.isArray(p.entries)
          ? p.entries.map((k, i) => <RenderNode key={i} node={k as TreeNode} />)
          : null}
      </div>
    </div>
  ),
  Entry: ({ p }) => (
    <div className="relative">
      <span
        className="absolute -left-4 top-1.5 h-2 w-2 rounded-full bg-lime-500"
        aria-hidden="true"
      />
      <div className="flex items-baseline gap-2">
        <span className="text-xs font-medium text-foreground">
          {String(p.title ?? "")}
        </span>
        {p.time ? (
          <span className="font-mono text-[11px] text-foreground/50">
            {String(p.time)}
          </span>
        ) : null}
      </div>
      {p.detail ? (
        <p className="mt-0.5 text-xs leading-5 text-foreground/70">
          {String(p.detail)}
        </p>
      ) : null}
    </div>
  ),
  KpiGrid: ({ p }) => (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
      {Array.isArray(p.metrics)
        ? p.metrics.map((k, i) => <RenderNode key={i} node={k as TreeNode} />)
        : null}
    </div>
  ),
  Kpi: ({ p }) => (
    <div className="rounded-xl bg-foreground/[0.04] px-3 py-2.5">
      <div className="truncate text-[11px] font-medium uppercase tracking-wide text-foreground/55">
        {String(p.label ?? "")}
      </div>
      <div className="mt-1 flex items-baseline gap-1">
        <span className="text-lg font-semibold leading-none text-foreground">
          {String(p.value ?? "")}
        </span>
        {p.unit ? (
          <span className="text-[11px] text-foreground/60">
            {String(p.unit)}
          </span>
        ) : null}
      </div>
      {p.delta ? (
        <div className="text-[11px] font-medium text-foreground/70">
          {String(p.delta)}
        </div>
      ) : null}
    </div>
  ),
  BarChart: ({ p }) => (
    <div>
      {p.title ? (
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-foreground/60">
          {String(p.title)}
        </div>
      ) : null}
      <div className="space-y-1.5">
        {Array.isArray(p.bars)
          ? p.bars.map((k, i) => <RenderNode key={i} node={k as TreeNode} />)
          : null}
      </div>
      {p.unit ? (
        <div className="mt-1 text-[11px] text-foreground/50">
          {String(p.unit)}
        </div>
      ) : null}
    </div>
  ),
  Bar: () => null,
  Alert: ({ p }) => {
    const level = String(p.level ?? "info");
    const cls =
      level === "error"
        ? "border-red-400/40 bg-red-400/5"
        : level === "warn"
          ? "border-amber-400/40 bg-amber-400/5"
          : level === "success"
            ? "border-lime-400/40 bg-lime-400/5"
            : "border-sky-400/40 bg-sky-400/5";
    return (
      <div className={`flex items-start gap-2.5 rounded-xl border p-3 ${cls}`}>
        <div className="min-w-0">
          {p.title ? (
            <div className="text-xs font-semibold text-foreground">
              {String(p.title)}
            </div>
          ) : null}
          <p className="mt-0.5 text-xs leading-5 text-foreground/80">
            {String(p.message ?? "")}
          </p>
        </div>
      </div>
    );
  },
  Poll: ({ p }) => (
    <div>
      <div className="text-xs font-medium text-foreground">
        {String(p.question ?? "")}
      </div>
      <div className="mt-2 space-y-1.5">
        {Array.isArray(p.options)
          ? p.options.map((k, i) => <RenderNode key={i} node={k as TreeNode} />)
          : null}
      </div>
    </div>
  ),
  PollOption: ({ p }) => (
    <div className="rounded-lg border border-foreground/12 px-3 py-2 text-xs text-foreground">
      {String(p.label ?? "")}
    </div>
  ),
  ItemCard: ({ p }) => (
    <div>
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-foreground">
            {String(p.title ?? "")}
          </div>
          {p.subtitle ? (
            <div className="truncate text-xs text-foreground/70">
              {String(p.subtitle)}
            </div>
          ) : null}
        </div>
        {p.status ? (
          <span className="shrink-0 rounded-full bg-primary/12 px-2 py-0.5 text-[11px] font-medium text-primary">
            {String(p.status)}
          </span>
        ) : null}
      </div>
      {Array.isArray(p.fields) ? (
        <div className="mt-2 space-y-1">
          {p.fields.map((k, i) => (
            <RenderNode key={i} node={k as TreeNode} />
          ))}
        </div>
      ) : null}
      {p.notes ? (
        <p className="mt-2 text-xs leading-5 text-foreground/80">
          {String(p.notes)}
        </p>
      ) : null}
      {Array.isArray(p.tags) ? (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {p.tags.slice(0, 8).map((t, i) => (
            <span
              key={i}
              className="rounded-full bg-foreground/8 px-2 py-0.5 text-[11px] text-foreground/75"
            >
              {String(t)}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  ),
  Field: ({ p }) => (
    <div className="flex gap-2 text-xs">
      <span className="w-28 shrink-0 text-foreground/55">
        {String(p.label ?? "")}
      </span>
      <span className="min-w-0 flex-1 text-foreground/85">
        {String(p.value ?? "")}
      </span>
    </div>
  ),
  Rive: ({ p }) => <RiveNode p={p} />,
};

function RenderNode({ node }: { node: TreeNode }) {
  if (!node || typeof node.component !== "string") return null;
  const render = MAP[node.component];
  if (!render) return null;
  return <>{render({ p: node.props ?? {}, node })}</>;
}

export function OpenUiHost({ code }: { code: string }) {
  const [state, setState] = React.useState<{
    loading: boolean;
    result: EvalResult | null;
    error: string | null;
  }>({ loading: true, result: null, error: null });

  React.useEffect(() => {
    let cancelled = false;
    setState((s) => ({ ...s, loading: true, error: null }));
    invoke<EvalResult>("evaluate_openui", { code })
      .then((r) => {
        if (!cancelled) setState({ loading: false, result: r, error: null });
      })
      .catch((e) => {
        if (!cancelled)
          setState({ loading: false, result: null, error: String(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [code]);

  if (state.loading) {
    return (
      <div className="my-2 h-16 w-full max-w-[420px] animate-pulse rounded-2xl border border-foreground/10 bg-foreground/[0.03]" />
    );
  }
  if (state.error || !state.result?.root) {
    return (
      <div className="my-2 rounded-xl border border-amber-400/40 bg-amber-400/5 p-3 text-xs text-foreground/75">
        Invalid omni-ui block — showing source.
        <pre className="mt-1 overflow-x-auto font-mono text-[11px] text-foreground/60">
          {code}
        </pre>
        {state.error ? (
          <div className="mt-1 text-foreground/50">{state.error}</div>
        ) : null}
      </div>
    );
  }
  const root = state.result.root as TreeNode;
  return (
    <div className="my-2 w-full max-w-[420px] rounded-2xl border border-foreground/10 bg-background/60 p-4 text-left shadow-sm">
      <RenderNode node={root} />
      <ProvenanceFromNode node={root} />
    </div>
  );
}
