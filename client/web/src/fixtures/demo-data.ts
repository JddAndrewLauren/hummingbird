// A typed mirror of the design kit's fixtures
// (.claude/skills/hummingbird-design/ui_kits/web/data.js), used only by
// demo mode. Nothing here is real data and nothing here reaches a
// production build — see demo.ts for the gate.

import type { Stage } from "../components/domain/StageBadge";
import type { AlertTier } from "../components/domain/AlertCard";
import type { BindingDTO, KindRegistryDTO, RuleDTO, TaskItemDTO } from "../store/protocol";

export type Urgency = "calm" | "soon" | "now" | "overdue";

/** A deadline string `hoursFromNow` ahead of the moment this module loads —
 * used only by `ruleBacktestItems` below, whose whole purpose is landing
 * inside the demo passport rule's `within_next '3d'` condition regardless
 * of what day the visual gate happens to run on. A frozen calendar date
 * would drift out of that window and silently stop demonstrating the
 * populated backtest state the capture exists to prove. */
function deadlineHoursFromNow(hours: number): string {
  const d = new Date(Date.now() + hours * 60 * 60 * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export interface DemoItem {
  id: string;
  title: string;
  stage: Stage;
  urgency: Urgency;
  deadline?: string;
  scheduled?: string;
  /** The real wire vocabulary, not a free label: `ItemRow` draws it as a
   * glyph now (#446), so a demo row that invented a word would render the
   * wrong ring count. */
  size?: TaskItemDTO["size"];
  energy?: TaskItemDTO["energy"];
  steps?: string;
  blockedBy?: string;
  project: string;
  /** The hero card's descriptive line. Lives on the item so reordering the
   * fixture can never leave the card describing a different action. */
  note?: string;
}

export interface DemoCapture {
  id: string;
  title: string;
  source: string;
  age: string;
}

export interface DemoAlert {
  id: string;
  tier: AlertTier;
  source: string;
  title: string;
  detail: string;
  time: string;
}

export interface DemoFog {
  q: string;
  note: string;
}

export interface DemoStep {
  text: string;
  done: boolean;
}

export interface DemoRoute {
  project: string;
  destination: string;
  fog: DemoFog[];
  actions: string[];
  /** The aside's checklist. Lives here rather than in `RoutesScreen` because
   * a module-level fixture literal in a screen is not behind the dead-branch
   * gate at all — that one shipped to production until `assert-no-fixtures`
   * grew a sentinel for it. Fixtures reach a screen through `demoData()` or
   * they reach a real device. */
  steps: DemoStep[];
}

export interface DemoRule {
  name: string;
  tier: AlertTier;
  description: string;
}

/** One #118 binding row as the kit renders it. Reuses the real
 * `BindingDTO` rather than a demo-shaped twin: the editor's whole job is to
 * render that value's three states, and a parallel fixture type could drift
 * from the shape it exists to preview. Demo rows are never writable — the
 * Settings screen passes no `onSetBinding` for them. */
export type DemoBinding = BindingDTO;

export interface DemoCalendar {
  id: string;
  summary: string;
}

export interface DemoData {
  items: DemoItem[];
  triage: DemoCapture[];
  alerts: DemoAlert[];
  route: DemoRoute;
  bindings: DemoBinding[];
  rules: DemoRule[];
  calendars: DemoCalendar[];
  /** The header's sync readout. Demo-only: no outbound queue exists yet. */
  syncBadge: string;
  /** The Rules screen's own fixtures (#140 review — `?demo` previously left
   * that screen on "Loading rules…" forever, so its light/dark acceptance
   * criterion was never actually exercised by the one gate that checks it).
   * Reuses the real wire DTOs rather than a `DemoRule`-shaped twin —
   * `DemoBinding`'s own precedent — since the whole point is exercising the
   * real `RuleCard`/`RuleEditorForm`/`BacktestPanel` rendering path, not a
   * parallel display-only shape. */
  ruleDetails: RuleDTO[];
  ruleKindRegistry: KindRegistryDTO;
  ruleBacktestItems: TaskItemDTO[];
}

export const DEMO_DATA: DemoData = {
  items: [
    { id: "ION-142", title: "Order the replacement sensor", stage: "ready", urgency: "soon", deadline: "Fri", size: "quick", steps: "2/5", project: "Greenhouse" },
    // ION-118's title is visual/surfaces.spec.ts's kit-world marker
    // (KIT_ONLY_TEXT). The invariant: this exact title must stay on some
    // kit-world item that renders on Now (the landing screen the
    // world-identity check reads) and must never appear in the board world.
    // Hero-ness is NOT load-bearing — NowScreen renders every non-`done`
    // item's title (hero or "Also startable"), so a demoted ION-118 still
    // satisfies the check; renaming this title, marking the item `done`, or
    // reusing the title in demo-task-state.ts is what breaks it.
    { id: "ION-118", title: "Rewrite the sweeper's Gmail adapter", stage: "in_progress", urgency: "now", size: "deep", steps: "3/7", project: "Hummingbird", note: "Started 40 minutes ago. Three of seven steps ticked; the next one is 'delete the two dead label cases'." },
    { id: "ION-151", title: "Hear back from the shop about the part", stage: "blocked", urgency: "calm", blockedBy: "ION-142", project: "Greenhouse" },
    { id: "ION-160", title: "Book the annual boiler service", stage: "ready", urgency: "calm", scheduled: "Mon", size: "quick", project: "House" },
    { id: "ION-161", title: "Draft the vacation itinerary", stage: "ready", urgency: "calm", scheduled: "Sat", size: "normal", project: "Travel" },
    { id: "ION-099", title: "File the insurance renewal", stage: "done", urgency: "calm", project: "House" },
  ],
  triage: [
    { id: "CAP-4", title: "ask dad about the trailer hitch", source: "Google Tasks", age: "2h" },
    { id: "CAP-5", title: "the fence gate is dragging again", source: "Google Tasks", age: "5h" },
    { id: "CAP-6", title: "Re: quote for the greenhouse glazing", source: "Gmail · capture", age: "1d" },
    { id: "CAP-7", title: "renew passport?? check expiry", source: "Google Tasks", age: "2d" },
  ],
  alerts: [
    { id: "A1", tier: "urgent", source: "Fly · hb-worker", title: "Sweeper run failed", detail: "Google Tasks adapter returned 503 twice.", time: "6m" },
    { id: "A2", tier: "normal", source: "Cloudflare · hb.twinion.net", title: "Deploy succeeded", detail: "client/web @ d4105b5 — 412 assets.", time: "1h" },
    { id: "A3", tier: "normal", source: "Gmail · capture", title: "3 new captures swept", detail: "Landed in Triage, acked in the source.", time: "3h" },
  ],
  route: {
    project: "Greenhouse",
    destination:
      "The greenhouse holds temperature overnight through winter, without me checking it.",
    fog: [
      {
        q: "Do the vents need a controller, or is the manual setup enough?",
        note: "Can't define an action until the sensor data says.",
      },
    ],
    actions: ["ION-142", "ION-151", "ION-160"],
    steps: [
      { text: "Open the fixture generator script", done: true },
      { text: "Regenerate the Gmail fixture set", done: true },
      { text: "Run the adapter tests once", done: true },
      { text: "Delete the two dead label cases", done: false },
      { text: "Re-read the sweeper doc's invariants", done: false },
    ],
  },
  bindings: [
    { key: "race-series", known: true, pending: false, value: { state: "text", text: "hyrox-uk" } },
    { key: "trips-calendar", known: true, pending: true, value: { state: "text", text: "travel@group.calendar.google.com" } },
    // Set, not unset: #245's ranked region reads this binding, and the
    // demo world's waste pane is answered — so the Settings editor and the
    // Now aside have to agree about it.
    { key: "city-waste-page", known: true, pending: false, value: { state: "text", text: "https://example.gov/waste/collection-day" } },
  ],
  rules: [
    { name: "Sweeper run failed", tier: "urgent", description: "Two consecutive adapter failures from one source." },
    { name: "Deploy finished", tier: "normal", description: "Any Cloudflare deploy on hb.twinion.net." },
    { name: "Race start", tier: "urgent", description: "90 minutes before a standing-question race start." },
  ],
  calendars: [
    { id: "andrew@…", summary: "Andrew (personal)" },
    { id: "family@group.calendar.google.com", summary: "Family" },
    { id: "twinion@…", summary: "twinion work" },
  ],
  syncBadge: "synced · 0 queued",
  ruleDetails: [
    {
      // First on purpose: the one demo rule that targets `item_threshold`,
      // so the visual gate's default "open the first card" capture lands
      // on a backtest that actually has local material to check against —
      // proving the populated match-count state, not just the "no local
      // event history" one every other demo kind reports.
      id: "rule-passport-renewal",
      name: "Passport renewal due soon",
      eventKind: "item_threshold",
      conditions: [
        { field: "deadline", op: "within_next", value: "3d", negate: false },
        { field: "title", op: "contains", value: "passport", negate: false },
      ],
      severity: "high",
      tier: "urgent",
      enabled: false,
      updatedAt: 1_755_000_000_000,
      version: 2,
    },
    {
      id: "rule-sweeper-fail",
      name: "Sweeper run failed",
      eventKind: "alert_raised",
      conditions: [{ field: "source", op: "eq", value: "fly-worker", negate: false }],
      severity: "urgent",
      tier: "urgent",
      enabled: true,
      updatedAt: 1_755_000_000_000,
      version: 4,
    },
    {
      id: "rule-deploy-finished",
      name: "Deploy finished",
      eventKind: null,
      conditions: [{ field: "source", op: "contains", value: "cloudflare", negate: false }],
      severity: "normal",
      tier: "normal",
      enabled: true,
      updatedAt: 1_755_000_000_000,
      version: 1,
    },
    // #374: last on purpose — the other three each have a reason to stay
    // where they are (the first is the backtest capture's target; the rest
    // are the demo's own narrative order), so the rule that exists only to
    // photograph the unranked-severity badge's wrapping row goes at the
    // end. "urgent-plus" is not in `ruleKindRegistry.severities` below.
    {
      id: "rule-unranked-severity",
      name: "Backup generator test overdue",
      eventKind: null,
      conditions: [{ field: "source", op: "contains", value: "generator", negate: false }],
      severity: "urgent-plus",
      tier: "normal",
      enabled: true,
      updatedAt: 1_755_000_000_000,
      version: 1,
    },
  ],
  ruleKindRegistry: {
    coreFields: [
      { name: "source", fieldType: "string" },
      { name: "source_key", fieldType: "string" },
      { name: "occurred_at", fieldType: "timestamp" },
      { name: "title", fieldType: "string" },
      { name: "body", fieldType: "string" },
      { name: "url", fieldType: "string" },
      { name: "severity", fieldType: "string" },
      { name: "calendar_busy", fieldType: "bool" },
    ],
    kinds: [
      {
        key: "email",
        mints: true,
        fields: [
          { name: "from", fieldType: "string" },
          { name: "to", fieldType: "string_list" },
          { name: "subject", fieldType: "string" },
          { name: "received_at", fieldType: "timestamp" },
          { name: "has_attachment", fieldType: "bool" },
        ],
      },
      {
        key: "calendar_event",
        mints: true,
        fields: [
          { name: "calendar", fieldType: "string" },
          { name: "starts_at", fieldType: "timestamp" },
          { name: "ends_at", fieldType: "timestamp" },
          { name: "is_all_day", fieldType: "bool" },
        ],
      },
      {
        key: "item_threshold",
        mints: true,
        fields: [
          { name: "deadline", fieldType: "timestamp" },
          { name: "scheduled_date", fieldType: "date" },
          { name: "title", fieldType: "string" },
          { name: "stage", fieldType: "string" },
          { name: "size", fieldType: "string" },
          { name: "energy", fieldType: "string" },
          { name: "context", fieldType: "string" },
          { name: "priority", fieldType: "number" },
          { name: "project", fieldType: "string" },
        ],
      },
      {
        key: "snapshot_change",
        mints: true,
        fields: [
          { name: "key", fieldType: "string" },
          { name: "changed_at", fieldType: "timestamp" },
          { name: "value", fieldType: "dynamic" },
          { name: "previous", fieldType: "dynamic" },
        ],
      },
      { key: "alert_raised", mints: false, fields: [] },
    ],
    alarmIntervalMs: 900_000,
    severities: ["low", "normal", "high", "urgent"],
  },
  ruleBacktestItems: [
    {
      id: "ION-142",
      seq: 142,
      title: "renew passport before the trip",
      description: null,
      stage: "ready",
      size: "quick",
      energy: "low",
      context: null,
      priority: 1,
      projectId: null,
      projectPos: null,
      deadline: deadlineHoursFromNow(36),
      scheduledDate: null,
      source: null,
      sourceKey: null,
      sourceUrl: null,
      archivedAt: null,
      createdAt: 1_755_000_000_000,
      updatedAt: 1_755_000_000_000,
      version: 1,
      pending: false,
    },
    {
      id: "ION-160",
      seq: 160,
      title: "book the annual boiler service",
      description: null,
      stage: "ready",
      size: "quick",
      energy: "low",
      context: null,
      priority: 0,
      projectId: null,
      projectPos: null,
      deadline: null,
      scheduledDate: "2026-08-24",
      source: null,
      sourceKey: null,
      sourceUrl: null,
      archivedAt: null,
      createdAt: 1_755_000_000_000,
      updatedAt: 1_755_000_000_000,
      version: 1,
      pending: false,
    },
  ],
};
