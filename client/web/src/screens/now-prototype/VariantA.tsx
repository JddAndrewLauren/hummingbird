// PROTOTYPE — throwaway. Delete with the rest of `now-prototype/`.
//
// Variant A — "Narrow it down". No grouping at all: one flat run of actions
// ordered by attention, under a sticky bar of facet chips (context, size,
// energy, urgency). The primary affordance is *subtraction* — you say where
// you are and what you have, and the list shrinks to what qualifies.

import { useState } from "react";
import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { ItemRow } from "../../components/domain/ItemRow";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { VariantProps } from "./contract";
import { FacetRow } from "./facet-chips";
import {
  byAttention,
  contextsOf,
  ENERGIES,
  matchesFacets,
  NO_FACETS,
  SIZES,
  toggleFacet,
  urgencyOf,
  type FacetSelection,
} from "./facets";

export function VariantA({ items, projects, nowMs, selectedId, onOpenItem }: VariantProps) {
  const [picked, setPicked] = useState<FacetSelection>(NO_FACETS);

  const namesById = new Map(projects.map((project) => [project.id, project.name]));
  const shown = [...items]
    .filter((item) => matchesFacets(item, picked, nowMs))
    .sort(byAttention(nowMs));
  const anyFilter = Object.values(picked).some((set) => set.size > 0);

  return (
    <>
      <Card
        padding="var(--space-5)"
        style={{
          position: "sticky",
          top: 0,
          zIndex: 2,
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-4)",
        }}
      >
        <FacetRow
          title="context"
          values={contextsOf(items)}
          selected={picked.context}
          onToggle={(value) => setPicked(toggleFacet(picked, "context", value))}
        />
        <FacetRow
          title="size"
          values={SIZES}
          selected={picked.size}
          onToggle={(value) => setPicked(toggleFacet(picked, "size", value))}
        />
        <FacetRow
          title="energy"
          values={ENERGIES}
          selected={picked.energy}
          onToggle={(value) => setPicked(toggleFacet(picked, "energy", value))}
        />
        <FacetRow
          title="urgency"
          values={["overdue", "now", "soon"]}
          selected={picked.urgency}
          onToggle={(value) => setPicked(toggleFacet(picked, "urgency", value))}
        />
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            borderTop: "1px solid var(--border-subtle)",
            paddingTop: "var(--space-4)",
          }}
        >
          <span className="hb-meta">
            {shown.length} of {items.length} shown
          </span>
          {anyFilter ? (
            <button
              type="button"
              onClick={() => setPicked(NO_FACETS)}
              style={{
                font: "var(--type-body-sm)",
                background: "none",
                border: "none",
                color: "var(--text-link)",
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                gap: "var(--space-2)",
              }}
            >
              <Icon name="x" size={14} />
              Clear
            </button>
          ) : null}
        </div>
      </Card>

      <Card padding="var(--space-3)">
        {shown.length === 0 ? (
          <EmptyState
            icon="search"
            headingLevel={2}
            title="Nothing matches"
            body="No startable action carries every facet you picked."
          />
        ) : (
          shown.map((item) => (
            // The project is a leading label rather than a section heading —
            // this variant's whole claim is that grouping by project is the
            // wrong axis when you are picking what to do next.
            <div
              key={item.id}
              style={{ display: "flex", alignItems: "center", gap: "var(--space-3)" }}
            >
              <span style={{ width: 104, flex: "0 0 auto", textAlign: "right" }}>
                <Badge mono>
                  {item.projectId === null
                    ? "none"
                    : (namesById.get(item.projectId) ?? "project")}
                </Badge>
              </span>
              <ItemRow
                title={item.title}
                stage={item.stage}
                urgency={urgencyOf(item, nowMs)}
                deadline={item.deadline ?? undefined}
                scheduled={item.scheduledDate ?? undefined}
                size={item.size ?? undefined}
                priority={item.priority}
                selected={item.id === selectedId}
                onClick={() => onOpenItem(item.id)}
                style={{ flex: 1, minWidth: 0 }}
              />
            </div>
          ))
        )}
      </Card>
    </>
  );
}
