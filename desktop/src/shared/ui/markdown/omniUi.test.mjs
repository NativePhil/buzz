import assert from "node:assert/strict";
import test from "node:test";

import { OMNI_UI_COMPONENTS, parseOmniUiBlock } from "./omniUiSchema.ts";

// --- happy paths: one golden block per component ---

const GOLDEN = {
  "contact-card": {
    name: "Phil",
    title: "CEO",
    org: "Native",
    status: "online",
    tags: ["founder"],
  },
  "metric-card": {
    label: "Graph edges",
    value: 208,
    unit: "edges",
    sparkline: [1, 2, 3],
    source: "mneme-cache",
  },
  "actions-row": {
    actions: [
      {
        label: "Ask about HubSpot",
        kind: "agent_prompt",
        prompt: "How do we plug into HubSpot?",
      },
    ],
  },
  rive: {
    asset: "omni/gauge.riv",
    stateMachine: "live",
    bindings: { value: 42 },
  },
  "status-grid": {
    title: "Services",
    services: [
      { name: "relay", status: "ok", detail: "12ms" },
      { name: "engine", status: "down" },
    ],
  },
  progress: {
    label: "Migration",
    step: 3,
    totalSteps: 4,
    detail: "nearly there",
  },
  checklist: {
    title: "Launch",
    items: [{ text: "wire relay", done: true }, { text: "invite team" }],
  },
  table: {
    title: "Pipelines",
    columns: [
      { key: "name", label: "Name" },
      { key: "rate", label: "Rate" },
    ],
    rows: [{ name: "kestrel", rate: 431 }],
  },
  timeline: {
    entries: [{ time: "10:02", title: "token exchange", detail: "completed" }],
  },
  "kpi-grid": {
    metrics: [
      { label: "Turns", value: 512, delta: 12 },
      { label: "Errors", value: 0 },
    ],
  },
  "bar-chart": {
    title: "Throughput",
    bars: [
      { label: "kestrel", value: 431 },
      { label: "blockade", value: 217 },
    ],
    unit: "/h",
  },
  alert: {
    level: "warn",
    title: "Quota",
    message: "Inception eval tier at 80% for today.",
  },
  poll: {
    question: "Which tier for the demo?",
    options: [
      { label: "Mercury", votes: 3 },
      { label: "Muse", votes: 1 },
    ],
  },
  "item-card": {
    title: "Mneme",
    subtitle: "Deterministic memory engine",
    status: "v1",
    fields: [{ label: "Edges", value: "208" }],
    links: [{ label: "site", url: "https://native.business" }],
  },
};

for (const component of OMNI_UI_COMPONENTS) {
  test(`accepts golden ${component}`, () => {
    const block = parseOmniUiBlock(
      JSON.stringify({ v: 1, component, props: GOLDEN[component] }),
    );
    assert.ok(block, `${component} should parse`);
    assert.equal(block.component, component);
  });
}

test("extra unknown props are filtered, not rendered", () => {
  const block = parseOmniUiBlock(
    JSON.stringify({
      v: 1,
      component: "contact-card",
      props: {
        name: "Phil",
        hijack: "<script>window.pwned=1</script>",
        onclick: "alert(1)",
      },
    }),
  );
  assert.ok(block);
  const props = block.props;
  assert.equal("hijack" in props, false);
  assert.equal("onclick" in props, false);
  assert.equal(props.name, "Phil");
});

// --- hostile inputs ---

test("rejects unknown components", () => {
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({
        v: 1,
        component: "iframe",
        props: { src: "https://evil.example" },
      }),
    ),
    null,
  );
  assert.equal(
    parseOmniUiBlock(JSON.stringify({ v: 1, component: "script", props: {} })),
    null,
  );
});

test("rejects wrong versions", () => {
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({
        v: 2,
        component: "metric-card",
        props: { label: "x", value: 1 },
      }),
    ),
    null,
  );
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({
        component: "metric-card",
        props: { label: "x", value: 1 },
      }),
    ),
    null,
  );
});

test("rejects malformed JSON and non-objects", () => {
  assert.equal(parseOmniUiBlock("not json at all"), null);
  assert.equal(parseOmniUiBlock("[1,2,3]"), null);
  assert.equal(parseOmniUiBlock("42"), null);
  assert.equal(parseOmniUiBlock("null"), null);
});

test("rejects missing required props", () => {
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({ v: 1, component: "contact-card", props: {} }),
    ),
    null,
  );
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({ v: 1, component: "metric-card", props: { label: "x" } }),
    ),
    null,
  );
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({ v: 1, component: "alert", props: { level: "warn" } }),
    ),
    null,
  );
});

test("oversized strings: required fields reject, optional fields filter", () => {
  const huge = "A".repeat(300);
  // required field oversized → whole block rejected
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({
        v: 1,
        component: "metric-card",
        props: { label: huge, value: 1 },
      }),
    ),
    null,
  );
  // optional field oversized → field dropped, block survives
  const block = parseOmniUiBlock(
    JSON.stringify({
      v: 1,
      component: "contact-card",
      props: { name: "x", notes: huge.repeat(3) },
    }),
  );
  assert.ok(block);
  assert.equal("notes" in block.props, false);
});

test("sanitizes javascript: URLs out of links", () => {
  const block = parseOmniUiBlock(
    JSON.stringify({
      v: 1,
      component: "contact-card",
      props: {
        name: "Phil",
        links: [
          { label: "click", url: "javascript:alert(1)" },
          { label: "ok", url: "https://native.business" },
        ],
      },
    }),
  );
  assert.ok(block);
  const props = block.props;
  assert.equal(props.links.length, 1);
  assert.equal(props.links[0].url, "https://native.business");
});

test("rejects non-object props and arrays as props", () => {
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({ v: 1, component: "metric-card", props: "string" }),
    ),
    null,
  );
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({ v: 1, component: "metric-card", props: ["array"] }),
    ),
    null,
  );
});

test("rejects hostile checklist/table entries", () => {
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({
        v: 1,
        component: "checklist",
        props: { items: [{ text: 42 }] },
      }),
    ),
    null,
  );
  assert.equal(
    parseOmniUiBlock(
      JSON.stringify({
        v: 1,
        component: "table",
        props: { columns: [{ key: "a", label: "A" }], rows: "nope" },
      }),
    ),
    null,
  );
});
