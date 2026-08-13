// PROTOTYPE — throwaway. Delete with the rest of `now-prototype/`.

import type { ProjectDTO, TaskItemDTO } from "../../store/protocol";

export interface VariantProps {
  items: readonly TaskItemDTO[];
  projects: readonly ProjectDTO[];
  nowMs: number;
  /** Opens the real `ItemDetailPanel` above the board — see `index.tsx`.
   * Nothing here mutates. */
  onOpenItem: (itemId: string) => void;
  /** The item currently expanded above the board, so the row or card it came
   * from can say so. The board stays on screen while it is open. */
  selectedId: string | null;
}

/** Each variant exports a name for the switcher's label. */
export interface Variant {
  key: string;
  name: string;
  render: (props: VariantProps) => React.ReactNode;
}
