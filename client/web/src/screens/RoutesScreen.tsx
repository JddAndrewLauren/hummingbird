import { useState } from "react";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { ItemRow } from "../components/domain/ItemRow";
import { EmptyState } from "../components/feedback/EmptyState";
import { Checkbox } from "../components/forms/Checkbox";
import { demoData } from "../fixtures/demo-data";
import { Aside, Column, TwoColumn } from "./layout";

// #457: this screen's own dev-gated fixture accessor, read directly rather
// than threaded through `App.tsx` as a `demo` prop. Routes reads nothing
// from the store (`docs/SURFACES.md`'s own note on why), so its only
// populated render stays the kit world's, reachable at `?demo=kit`.
export function RoutesScreen() {
  // Lazy initializer, not a plain call: `demoData()` reads
  // `window.location.search`, and this is the same once-per-mount treatment
  // `App.tsx`'s own fixture reads get, not once-per-render. Read before the
  // `!demo` return because hooks cannot sit behind it. The checklist comes
  // from `demo.route` rather than a literal in this file: a module-level
  // fixture in a screen is outside the dead-branch gate, and this one
  // shipped to production until `assert-no-fixtures` learned to look for it.
  const [demo] = useState(demoData);
  const [steps, setSteps] = useState(() => demo?.route.steps ?? []);

  if (!demo) {
    return (
      <Card padding="var(--space-3)" style={{ maxWidth: "var(--content-max)" }}>
        <EmptyState
          icon="route"
          headingLevel={2}
          title="No routes yet"
          body="A Route holds a project's Destination, its Fog, and the actions minted toward it."
        />
      </Card>
    );
  }

  const { route } = demo;
  const actions = demo.items.filter((item) => route.actions.includes(item.id));

  return (
    <TwoColumn>
      <Column>
        <div>
          <span className="hb-meta">route · {route.project}</span>
          <h2
            style={{
              font: "var(--type-h1)",
              letterSpacing: "var(--tracking-heading)",
              marginTop: "var(--space-4)",
              color: "var(--text-primary)",
            }}
          >
            Destination
          </h2>
          <p
            style={{
              font: "var(--type-body)",
              color: "var(--text-secondary)",
              marginTop: "var(--space-3)",
              maxWidth: 560,
            }}
          >
            {route.destination}
          </p>
        </div>

        <div>
          <h3
            style={{
              font: "var(--type-h3)",
              marginBottom: "var(--space-4)",
              color: "var(--text-primary)",
            }}
          >
            Minted actions
          </h3>
          <Card padding="var(--space-3)">
            {actions.map((item) => (
              <ItemRow
                key={item.id}
                title={item.title}
                stage={item.stage}
                urgency={item.urgency}
                deadline={item.deadline}
                scheduled={item.scheduled}
                size={item.size}
                energy={item.energy}
                steps={item.steps}
                blockedBy={item.blockedBy}
              />
            ))}
          </Card>
        </div>

        <div>
          <h3
            style={{
              font: "var(--type-h3)",
              marginBottom: "var(--space-4)",
              color: "var(--text-primary)",
            }}
          >
            Fog
          </h3>
          {route.fog.map((entry) => (
            <Card
              key={entry.q}
              padding="var(--space-5)"
              style={{
                display: "flex",
                gap: "var(--space-5)",
                borderColor: "var(--stage-grilling)",
              }}
            >
              <Icon
                name="cloud-fog"
                size={18}
                color="var(--stage-grilling)"
                style={{ marginTop: 2 }}
              />
              <div>
                <p style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
                  {entry.q}
                </p>
                <p
                  style={{
                    font: "var(--type-body-sm)",
                    color: "var(--text-secondary)",
                    marginTop: 2,
                  }}
                >
                  {entry.note}
                </p>
              </div>
            </Card>
          ))}
        </div>
      </Column>

      <Aside label="Steps and notes">
        <span className="hb-meta">steps · ION-118</span>
        <Card
          padding="var(--space-5)"
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
        >
          {steps.map((step, index) => (
            <Checkbox
              key={step.text}
              checked={step.done}
              label={step.text}
              onChange={() =>
                setSteps((current) =>
                  current.map((entry, entryIndex) =>
                    entryIndex === index ? { ...entry, done: !entry.done } : entry,
                  ),
                )
              }
            />
          ))}
          <Button variant="secondary" size="sm" iconLeft="plus" fullWidth>
            Add a step
          </Button>
        </Card>
        <Card
          padding="var(--space-5)"
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
        >
          <span className="hb-meta">notes</span>
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            Steps are 2–5 minute physical actions. They live on an action&rsquo;s checklist and never
            become actions themselves.
          </p>
        </Card>
      </Aside>
    </TwoColumn>
  );
}
