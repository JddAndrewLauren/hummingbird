import type { QuestionDef } from "../questions/contract";
import { fantasyAnswer, fantasySubjects } from "./fantasy";
import { FantasyPaneExpanded } from "./FantasyPaneExpanded";

/** "Fantasy football" as the shell's registry sees it (#717): two panes per
 * league named in the `yahoo-leagues` binding — the lineup and the waivers,
 * ranked independently — and, until a league is named, the two setup panes,
 * so both obligations are discoverable (ADR-0015). */
export const fantasyQuestion: QuestionDef = {
  surface: "now",
  subjects: fantasySubjects,
  answer: fantasyAnswer,
  Expanded: FantasyPaneExpanded,
};
