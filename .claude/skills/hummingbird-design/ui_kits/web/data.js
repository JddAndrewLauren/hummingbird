// Fixture data for the web kit. Names and vocabulary follow CONTEXT.md.
window.HB_DATA = {
  items: [
    { id: "ION-142", title: "Order the replacement sensor", stage: "ready", urgency: "soon", due: "Fri", size: "quick", steps: "2/5", project: "Greenhouse" },
    { id: "ION-118", title: "Rewrite the sweeper's Gmail adapter", stage: "in_progress", urgency: "now", size: "deep", steps: "3/7", project: "Hummingbird" },
    { id: "ION-151", title: "Hear back from the shop about the part", stage: "blocked", urgency: "calm", blockedBy: "ION-142", project: "Greenhouse" },
    { id: "ION-160", title: "Book the annual boiler service", stage: "ready", urgency: "calm", scheduled: "Mon", size: "quick", project: "House" },
    { id: "ION-161", title: "Draft the vacation itinerary", stage: "ready", urgency: "calm", scheduled: "Sat", size: "normal", project: "Travel" },
    { id: "ION-099", title: "File the insurance renewal", stage: "done", urgency: "calm", project: "House" }
  ],
  triage: [
    { id: "CAP-4", title: "ask dad about the trailer hitch", source: "Google Tasks", age: "2h" },
    { id: "CAP-5", title: "the fence gate is dragging again", source: "Google Tasks", age: "5h" },
    { id: "CAP-6", title: "Re: quote for the greenhouse glazing", source: "Gmail · capture", age: "1d" },
    { id: "CAP-7", title: "renew passport?? check expiry", source: "Google Tasks", age: "2d" }
  ],
  alerts: [
    { id: "A1", tier: "urgent", source: "Fly · hb-worker", title: "Sweeper run failed", detail: "Google Tasks adapter returned 503 twice.", time: "6m" },
    { id: "A2", tier: "normal", source: "Cloudflare · hb.twinion.net", title: "Deploy succeeded", detail: "client/web @ d4105b5 — 412 assets.", time: "1h" },
    { id: "A3", tier: "normal", source: "Gmail · capture", title: "3 new captures swept", detail: "Landed in Triage, acked in the source.", time: "3h" }
  ],
  route: {
    project: "Greenhouse",
    destination: "The greenhouse holds temperature overnight through winter, without me checking it.",
    fog: [
      { q: "Do the vents need a controller, or is the manual setup enough?", note: "Can't define an action until the sensor data says." }
    ],
    actions: ["ION-142", "ION-151", "ION-160"]
  },
  calendars: [
    { id: "andrew@…", summary: "Andrew (personal)" },
    { id: "family@group.calendar.google.com", summary: "Family" },
    { id: "twinion@…", summary: "twinion work" }
  ],
  snapshots: [
    { name: "Fly · machine hours", value: "112 / 160", note: "as of 4m ago", tone: "info" },
    { name: "hb.twinion.net", value: "99.98%", note: "as of 9m ago", tone: "success" },
    { name: "Outbound queue", value: "0 pending", note: "as of just now", tone: "success" }
  ]
};
