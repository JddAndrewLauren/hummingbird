// A typed mirror of the design kit's fixtures
// (.claude/skills/hummingbird-design/ui_kits/web/data.js), used only by
// demo mode. Nothing here is real data and nothing here reaches a
// production build — see demo.ts for the gate.

import type { Stage } from "../components/domain/StageBadge";
import type { AlertTier } from "../components/domain/AlertCard";

export type Urgency = "calm" | "soon" | "now" | "overdue";

export interface DemoItem {
  id: string;
  title: string;
  stage: Stage;
  urgency: Urgency;
  due?: string;
  scheduled?: string;
  size?: string;
  steps?: string;
  blockedBy?: string;
  project: string;
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

export interface DemoRoute {
  project: string;
  destination: string;
  fog: DemoFog[];
  actions: string[];
}

export interface DemoSnapshot {
  name: string;
  value: string;
  note: string;
}

export interface DemoStandingQuestion {
  q: string;
  a: string;
}

export interface DemoRule {
  name: string;
  tier: AlertTier;
  description: string;
}

export interface DemoCalendar {
  id: string;
  summary: string;
}

export interface DemoData {
  items: DemoItem[];
  triage: DemoCapture[];
  alerts: DemoAlert[];
  route: DemoRoute;
  snapshots: DemoSnapshot[];
  standingQuestions: DemoStandingQuestion[];
  rules: DemoRule[];
  calendars: DemoCalendar[];
  /** The header's sync readout. Demo-only: no outbound queue exists yet. */
  syncBadge: string;
}

export const DEMO_DATA: DemoData = {
  items: [
    { id: "ION-142", title: "Order the replacement sensor", stage: "ready", urgency: "soon", due: "Fri", size: "quick", steps: "2/5", project: "Greenhouse" },
    { id: "ION-118", title: "Rewrite the sweeper's Gmail adapter", stage: "in_progress", urgency: "now", size: "deep", steps: "3/7", project: "Hummingbird" },
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
  },
  snapshots: [
    { name: "Fly · machine hours", value: "112 / 160", note: "as of 4m ago" },
    { name: "hb.twinion.net", value: "99.98%", note: "as of 9m ago" },
    { name: "Outbound queue", value: "0 pending", note: "as of just now" },
  ],
  standingQuestions: [
    { q: "Next race", a: "Ridgeline 10k · 16 days" },
    { q: "Vacation", a: "Lisbon · 38 days" },
    { q: "Which cans", a: "Recycling · Tuesday" },
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
};
