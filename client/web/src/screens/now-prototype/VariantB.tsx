// PROTOTYPE — throwaway. Delete with the rest of `now-prototype/`.
//
// Variant B — "Lanes". No filter controls at all: the grouping *is* the
// filter. Project sections are replaced by six computed lanes that partition
// the frontier by what the moment can absorb — Overdue, Due today, Scheduled
// today, Quick wins, Deep work, Everything else — in that fixed order, so the
// page reads top-to-bottom as descending claim on your attention. Lanes
// collapse; an empty lane is stated rather than hidden, because "nothing is
// overdue" is information.

import { useState } from "react";
import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { ItemRow } from "../../components/domain/ItemRow";
import type { TaskItemDTO } from "../../store/protocol";
import type { VariantProps } from "./contract";
import {
  byAttention,
  LANE_ACCENT,
  LANE_BLURBS,
  LANE_ORDER,
  LANE_TITLES,
  laneOf,
  urgencyOf,
  type Lane,
} from "./facets";

export function VariantB({ items, projects, nowMs, selectedId, onOpenItem }: VariantProps) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<Lane>>(new Set<Lane>(["rest"]));
  const namesById = new Map(projects.map((project) => [project.id, project.name]));

  const byLane = new Map<Lane, TaskItemDTO[]>(LANE_ORDER.map((lane) => [lane, []]));
  for (const item of items) {
    byLane.get(laneOf(item, nowMs))?.push(item);
  }

  const toggle = (lane: Lane) =>
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(lane)) {
        next.delete(lane);
      } else {
        next.add(lane);
      }
      return next;
    });

  return (
    <>
      {LANE_ORDER.map((lane) => {
        const laneItems = (byLane.get(lane) ?? []).sort(byAttention(nowMs));
        const isCollapsed = collapsed.has(lane) || laneItems.length === 0;
        return (
          <Card key={lane} padding="0" style={{ overflow: "hidden" }}>
            <button
              type="button"
              onClick={() => (laneItems.length === 0 ? undefined : toggle(lane))}
              disabled={laneItems.length === 0}
              style={{
                width: "100%",
                display: "flex",
                alignItems: "center",
                gap: "var(--space-4)",
                padding: "var(--space-5) var(--space-6)",
                background: "transparent",
                border: "none",
                borderLeft: `3px solid ${LANE_ACCENT[lane]}`,
                textAlign: "left",
                cursor: laneItems.length === 0 ? "default" : "pointer",
              }}
            >
              <span style={{ flex: 1, minWidth: 0 }}>
                <span
                  style={{
                    display: "block",
                    font: "var(--type-h3)",
                    color:
                      laneItems.length === 0 ? "var(--text-muted)" : "var(--text-primary)",
                  }}
                >
                  {LANE_TITLES[lane]}
                </span>
                <span className="hb-meta" style={{ display: "block" }}>
                  {laneItems.length === 0 ? "none" : LANE_BLURBS[lane]}
                </span>
              </span>
              <Badge mono tone={lane === "overdue" && laneItems.length > 0 ? "danger" : "neutral"}>
                {laneItems.length}
              </Badge>
              {laneItems.length > 0 ? (
                <Icon
                  name="chevron-down"
                  size={16}
                  style={{
                    transform: isCollapsed ? "rotate(-90deg)" : "none",
                    transition: "transform var(--dur-fast) var(--ease-flit)",
                    color: "var(--text-muted)",
                  }}
                />
              ) : null}
            </button>

            {isCollapsed ? null : (
              <div style={{ padding: "0 var(--space-3) var(--space-3)" }}>
                {laneItems.map((item) => (
                  <div
                    key={item.id}
                    style={{ display: "flex", alignItems: "center", gap: "var(--space-3)" }}
                  >
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
                    <span style={{ width: 104, flex: "0 0 auto" }}>
                      <Badge mono>
                        {item.projectId === null
                          ? "none"
                          : (namesById.get(item.projectId) ?? "project")}
                      </Badge>
                    </span>
                  </div>
                ))}
              </div>
            )}
          </Card>
        );
      })}
    </>
  );
}
